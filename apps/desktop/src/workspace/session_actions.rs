use super::*;

impl Workspace {
    pub(super) fn session_menu(
        &self,
        session: &Session,
        location: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = session.id.clone();
        let archived = session.archived;
        let owner = cx.entity().downgrade();
        Button::new(SharedString::from(format!(
            "session-actions-{location}-{id}"
        )))
        .ghost()
        .xsmall()
        .icon(IconName::Ellipsis)
        .tooltip("Session Actions")
        .accessibility_label(format!("Session Actions: {}", session.title))
        .disabled(self.changing_sessions.contains(&id) || self.deleting_sessions.contains(&id))
        .dropdown_menu(move |menu, _, _| {
            let archive_owner = owner.clone();
            let archive_id = id.clone();
            let delete_owner = owner.clone();
            let delete_id = id.clone();
            menu.item(
                PopupMenuItem::new(if archived {
                    "Restore Session"
                } else {
                    "Archive Session"
                })
                .icon(IconName::Archive)
                .on_click(move |_, window, cx| {
                    let _ = archive_owner.update(cx, |this, cx| {
                        this.set_archived(&archive_id, !archived, window, cx);
                    });
                }),
            )
            .separator()
            .item(
                PopupMenuItem::new("Delete Session…")
                    .icon(IconName::Trash)
                    .on_click(move |_, window, cx| {
                        let owner = delete_owner.clone();
                        let id = delete_id.clone();
                        // Let the menu dismiss and return focus to its trigger
                        // before the dialog captures its restoration target.
                        window.defer(cx, move |window, cx| {
                            let _ =
                                owner.update(cx, |this, cx| this.request_delete(&id, window, cx));
                        });
                    }),
            )
        })
        .into_any_element()
    }

    pub(super) fn session_row(&self, session: &Session, cx: &mut Context<Self>) -> AnyElement {
        let id = session.id.clone();
        let deleting = self.deleting_sessions.contains(&id);
        let title = format!(
            "{}{}{}",
            if session.running { "● " } else { "" },
            session.title,
            if deleting { " · Deleting…" } else { "" }
        );
        div()
            .pl_8()
            .h_flex()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!("session-{id}")))
                    .ghost()
                    .accessibility_label(title.clone())
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .child(
                                Icon::new(IconName::MessageSquare)
                                    .xsmall()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .font_weight(FontWeight::NORMAL)
                                    .child(title),
                            ),
                    )
                    .selected(self.active_session.as_ref() == Some(&id))
                    .disabled(deleting)
                    .flex_1()
                    .min_w_0()
                    .on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.select_session(&id, window, cx)
                        }),
                    ),
            )
            .child(self.session_menu(session, "sidebar", cx))
            .into_any_element()
    }

    fn set_archived(
        &mut self,
        id: &str,
        archived: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.changing_sessions.contains(id) || self.deleting_sessions.contains(id) {
            return;
        }
        self.capture_draft(cx);
        let Some(session) = self
            .sessions
            .iter_mut()
            .find(|s| s.id == id && s.archived != archived)
        else {
            return;
        };
        session.archived = archived;
        let record = session.clone();
        let project = session.project.clone();
        self.changing_sessions.insert(id.into());
        if archived {
            self.archived_projects.insert(project);
            self.vacate_session(id, window, cx);
        } else if self.active_session.as_deref() == Some(id) {
            self.select_session(id, window, cx);
        }
        let write = self.store.save("session", id, &record);
        let id = id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(write).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.changing_sessions.remove(&id);
                if let Err(error) = result {
                    if let Some(session) = this.sessions.iter_mut().find(|s| s.id == id) {
                        session.archived = !archived;
                        let record = session.clone();
                        // A later streaming save may have included the optimistic flag.
                        this.save_session(record, cx);
                    }
                    this.status = Some(format!(
                        "Could not {} session: {error}",
                        if archived { "archive" } else { "restore" }
                    ));
                    if archived && this.active_session.is_none() {
                        this.select_session(&id, window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn request_delete(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        if self.changing_sessions.contains(id) || self.deleting_sessions.contains(id) {
            return;
        }
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let id = session.id.clone();
            dialog.title(format!("Delete “{}”?", session.title))
                .child("This permanently deletes the session's messages, draft, and local Pi history. Any running Pi task and its tools will be terminated. Project files are kept.")
                .on_ok(move |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| this.delete_session(&id, window, cx));
                    true
                })
                .footer(DialogFooter::new()
                    .child(DialogClose::new().child(Button::new("cancel-delete-session").label("Cancel")))
                    .child(DialogAction::new().child(Button::new("confirm-delete-session").danger().label("Delete"))))
        });
    }

    fn delete_session(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.changing_sessions.contains(id) || self.deleting_sessions.contains(id) {
            return;
        }
        self.capture_draft(cx);
        self.deleting_sessions.insert(id.into());
        self.pending_prompt.remove(id);
        self.ready.remove(id);
        let id = id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            // Startup owns the process until it returns. Do not delete its file
            // while a late startup could still create/write it.
            while this.update(cx, |this, _| this.starting.contains(&id)).unwrap_or(false) {
                cx.background_executor().timer(std::time::Duration::from_millis(25)).await;
            }
            let Ok((client, store)) = this.update(cx, |this, _| (this.engines.remove(&id), this.store.clone())) else { return; };
            let deleting = id.clone();
            let result: Result<Option<String>> = cx.background_spawn(async move {
                if let Some(client) = client { client.terminate_and_wait()?; }
                store.remove("session", &deleting).await?;
                // Only the app-owned transcript is removable; never trust an arbitrary stored path.
                let file = store.directory().join("sessions").join(format!("{deleting}.jsonl"));
                match std::fs::remove_file(&file) {
                    Ok(()) => Ok(None),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Ok(Some(format!("Session deleted, but its local Pi history could not be removed: {error}"))),
                }
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.deleting_sessions.remove(&id);
                match result {
                    Ok(cleanup_error) => {
                        this.sessions.retain(|s| s.id != id);
                        this.vacate_session(&id, window, cx);
                        this.forget_session_cache(&id);
                        if let Some(error) = cleanup_error { this.status = Some(error); }
                    }
                    Err(error) => {
                        if let Some(session) = this.sessions.iter_mut().find(|s| s.id == id) {
                            session.running = false;
                            session.status = "Pi stopped. Session deletion failed.".into();
                        }
                        this.status = Some(format!("Could not delete session: {error}. The session has been kept."));
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    /// Remove navigation references without creating an unsolicited empty session.
    fn vacate_session(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.history.retain(|entry| entry != id);
        self.selection.sessions.retain(|_, session| session != id);
        if self.active_session.as_deref() == Some(id) {
            self.active_session = None;
            self.composer
                .update(cx, |input, cx| input.set_value("", window, cx));
            let next = self
                .sessions
                .iter()
                .find(|s| {
                    Some(&s.project) == self.active_project.as_ref()
                        && s.id != id
                        && !s.archived
                        && !self.deleting_sessions.contains(&s.id)
                })
                .map(|s| s.id.clone());
            if let Some(next) = next {
                self.select_session(&next, window, cx);
            }
        }
        self.history_ix = self
            .active_session
            .as_ref()
            .and_then(|id| self.history.iter().position(|entry| entry == id))
            .unwrap_or(0);
        self.persist_selection(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{PiClient, PiEvent, Preferences, Project, Session, Store, Workspace};
    use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, WindowOptions};
    use serde_json::json;

    fn fixture(store: &Store, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>) {
        let session: Session = serde_json::from_value(json!({
            "id":"a", "project":"p", "title":"Fixture Session", "draft":"keep draft 中文",
            "pi_file":store.directory().join("sessions/a.jsonl"), "messages":[]
        }))
        .unwrap();
        std::fs::write(&session.pi_file, "fixture transcript").unwrap();
        futures::executor::block_on(store.save("session", "a", &session)).unwrap();
        cx.dispatcher.allow_parking();
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    Workspace::new(
                        store.clone(),
                        Preferences {
                            pi_path: "/does/not/exist".into(),
                            ..Default::default()
                        },
                        vec![Project {
                            id: "p".into(),
                            name: "Fixture Project".into(),
                            path: store.directory().into(),
                        }],
                        vec![session],
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        })
    }

    #[gpui_kit::test]
    async fn failed_archive_and_delete_keep_the_session(cx: &mut TestAppContext) {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open(temporary.path().into()).unwrap();
        let (handle, workspace) = fixture(&store, cx);
        let database = rusqlite::Connection::open(temporary.path().join("desktop.sqlite")).unwrap();
        database.execute_batch("CREATE TRIGGER fail_session_save BEFORE INSERT ON records WHEN NEW.kind='session' BEGIN SELECT RAISE(ABORT,'fixture write failure'); END;").unwrap();
        cx.update_window(handle, |_, window, cx| {
            workspace.update(cx, |this, cx| {
                this.active_project = Some("p".into());
                this.set_archived("a", true, window, cx);
            })
        })
        .unwrap();
        cx.condition(&workspace, |this, _| this.changing_sessions.is_empty())
            .await;
        cx.update(|cx| {
            let workspace = workspace.read(cx);
            assert!(!workspace.sessions[0].archived);
            assert_eq!(workspace.sessions[0].draft, "keep draft 中文");
        });
        let checkpoint = store.clone();
        cx.background_spawn(async move { checkpoint.flush().await.unwrap() })
            .await;
        database.execute_batch("DROP TRIGGER fail_session_save; CREATE TRIGGER fail_session_delete BEFORE DELETE ON records WHEN OLD.kind='session' BEGIN SELECT RAISE(ABORT,'fixture delete failure'); END;").unwrap();
        cx.update_window(handle, |_, window, cx| {
            workspace.update(cx, |this, cx| this.delete_session("a", window, cx))
        })
        .unwrap();
        cx.condition(&workspace, |this, _| this.deleting_sessions.is_empty())
            .await;
        cx.update(|cx| {
            let workspace = workspace.read(cx);
            assert_eq!(workspace.sessions.len(), 1);
            assert!(
                workspace
                    .status
                    .as_ref()
                    .unwrap()
                    .contains("Could not delete session")
            );
            assert!(!workspace.sessions[0].running);
        });
        assert!(store.directory().join("sessions/a.jsonl").exists());
        let checkpoint = store.clone();
        let saved = cx
            .background_spawn(async move {
                checkpoint
                    .get::<Session>("session", "a")
                    .await
                    .unwrap()
                    .unwrap()
            })
            .await;
        assert!(!saved.archived);
        assert_eq!(saved.draft, "keep draft 中文");
    }

    #[gpui_kit::test]
    async fn deletion_waits_for_late_startup_and_reaps_pi(cx: &mut TestAppContext) {
        use std::os::unix::fs::PermissionsExt;
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open(temporary.path().into()).unwrap();
        let (handle, workspace) = fixture(&store, cx);
        let executable = temporary.path().join("fixture-pi");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s' \"$$\" > pi.pid\nprintf 'engine history' > \"$4\"\nexec sleep 60\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let project = temporary.path().to_path_buf();
        let file = project.join("sessions/a.jsonl");
        let sender = cx.update(|cx| workspace.read(cx).sender.clone());
        let client = cx
            .background_spawn(async move {
                let client =
                    PiClient::spawn(&executable, &project, &file, "a".into(), sender).unwrap();
                for _ in 0..200 {
                    if project.join("pi.pid").exists() {
                        return client;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                panic!("Fixture engine did not start");
            })
            .await;
        let pid: i32 = std::fs::read_to_string(temporary.path().join("pi.pid"))
            .unwrap()
            .parse()
            .unwrap();
        cx.update_window(handle, |_, window, cx| {
            workspace.update(cx, |this, cx| {
                this.starting.insert("a".into());
                this.delete_session("a", window, cx);
                this.pi_event(
                    PiEvent::Json("a".into(), json!({"type":"agent_start"})),
                    window,
                    cx,
                );
                assert!(
                    !this.sessions[0].running,
                    "Events for a deleting session must be ignored"
                );
            })
        })
        .unwrap();
        let checkpoint = store.clone();
        assert!(
            cx.background_spawn(
                async move { checkpoint.get::<Session>("session", "a").await.unwrap() }
            )
            .await
            .is_some()
        );
        assert!(store.directory().join("sessions/a.jsonl").exists());
        // Deliver the actual startup result after deletion has already begun.
        cx.update(|cx| {
            workspace.update(cx, |this, cx| {
                this.complete_engine_start("a".into(), Ok(client), cx)
            })
        });
        cx.condition(&workspace, |this, _| this.deleting_sessions.is_empty())
            .await;
        cx.update(|cx| {
            let workspace = workspace.read(cx);
            assert!(workspace.sessions.is_empty());
            assert!(workspace.engines.is_empty());
            assert!(workspace.pending_prompt.is_empty());
        });
        assert!(!store.directory().join("sessions/a.jsonl").exists());
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "Deleted Pi process must be reaped"
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
}
