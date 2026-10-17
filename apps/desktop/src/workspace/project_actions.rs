use super::*;

impl Workspace {
    pub(super) fn project_header(
        &self,
        project: &Project,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = project.id.clone();
        div()
            .h_flex()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!("expand-{id}")))
                    .ghost()
                    .xsmall()
                    .icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .tooltip("Expand Project")
                    .accessibility_label("Expand Project")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed_projects.remove(&id) {
                            this.collapsed_projects.insert(id.clone());
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("project-{}", project.id)))
                    .ghost()
                    .h_auto()
                    .py_2()
                    .flex_1()
                    .min_w_0()
                    .accessibility_label(project.name.clone())
                    .tooltip(project.path.to_string_lossy().to_string())
                    .disabled(self.deleting_projects.contains(&project.id))
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .w_full()
                            .min_w_0()
                            .child(
                                Icon::new(if expanded {
                                    IconName::FolderOpen
                                } else {
                                    IconName::Folder
                                })
                                .small()
                                .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(project.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                if self.deleting_projects.contains(&project.id) {
                                                    "Deleting…".into()
                                                } else {
                                                    project.path.to_string_lossy().to_string()
                                                },
                                            ),
                                    ),
                            ),
                    )
                    .on_click({
                        let id = project.id.clone();
                        cx.listener(move |this, _, window, cx| this.select_project(&id, window, cx))
                    }),
            )
            .child(self.project_menu(project, cx))
            .into_any_element()
    }
    fn project_busy(&self, id: &str) -> bool {
        self.deleting_projects.contains(id)
            || self.sessions.iter().any(|s| {
                s.project == id
                    && (self.changing_sessions.contains(&s.id)
                        || self.deleting_sessions.contains(&s.id))
            })
    }

    pub(super) fn project_menu(&self, project: &Project, cx: &mut Context<Self>) -> AnyElement {
        let id = project.id.clone();
        let owner = cx.entity().downgrade();
        Button::new(SharedString::from(format!("project-actions-{id}")))
            .ghost()
            .xsmall()
            .icon(IconName::Ellipsis)
            .tooltip("Project Actions")
            .accessibility_label(format!("Project Actions: {}", project.name))
            .disabled(self.project_busy(&id))
            .dropdown_menu(move |menu, _, _| {
                let owner = owner.clone();
                let id = id.clone();
                menu.item(
                    PopupMenuItem::new("Delete Project…")
                        .icon(IconName::Trash)
                        .on_click(move |_, window, cx| {
                            let owner = owner.clone();
                            let id = id.clone();
                            window.defer(cx, move |window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.request_delete_project(&id, window, cx)
                                });
                            });
                        }),
                )
            })
            .into_any_element()
    }

    fn request_delete_project(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.project_busy(id) {
            return;
        }
        let Some(project) = self.projects.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        let count = self.sessions.iter().filter(|s| s.project == id).count();
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let owner = owner.clone(); let id = project.id.clone();
            dialog.title(format!("Delete “{}”?", project.name))
                .child(format!("This removes the project from this app and permanently deletes its {count} sessions, including archived sessions and local Pi history. Its Pi tasks and local terminals will stop. Files in the project folder are kept."))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child(project.path.to_string_lossy().to_string()))
                .on_ok(move |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| this.delete_project(&id, window, cx));
                    true
                })
                .footer(DialogFooter::new()
                    .child(DialogClose::new().child(Button::new("cancel-delete-project").label("Cancel")))
                    .child(DialogAction::new().child(Button::new("confirm-delete-project").danger().label("Delete Project"))))
        });
    }

    fn delete_project(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.project_busy(id) {
            return;
        }
        let Some(project) = self.projects.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        self.capture_draft(cx);
        let ids = self
            .sessions
            .iter()
            .filter(|s| s.project == id)
            .map(|s| s.id.clone())
            .collect::<HashSet<_>>();
        self.deleting_projects.insert(id.into());
        self.deleting_sessions.extend(ids.iter().cloned());
        for id in &ids {
            self.ready.remove(id);
            self.pending_prompt.remove(id);
        }
        cx.spawn_in(window, async move |this, cx| {
            while this.update(cx, |this, _| ids.iter().any(|id| this.starting.contains(id))).unwrap_or(false) {
                cx.background_executor().timer(std::time::Duration::from_millis(25)).await;
            }
            let Ok((clients, store)) = this.update(cx, |this, _| {
                (ids.iter().filter_map(|id| this.engines.remove(id)).collect::<Vec<_>>(), this.store.clone())
            }) else { return; };
            let removed = project.clone(); let transcripts = ids.clone();
            let result: Result<usize> = cx.background_spawn(async move {
                for client in clients { client.terminate_and_wait()?; }
                store.remove_project(&removed.id, &removed.path).await?;
                let mut failed = 0;
                for id in transcripts {
                    if let Err(error) = std::fs::remove_file(store.directory().join("sessions").join(format!("{id}.jsonl")))
                        && error.kind() != std::io::ErrorKind::NotFound { failed += 1; }
                }
                Ok(failed)
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.deleting_projects.remove(&project.id);
                for id in &ids { this.deleting_sessions.remove(id); }
                match result {
                    Ok(failed) => {
                        this.sessions.retain(|s| s.project != project.id);
                        this.projects.retain(|p| p.id != project.id);
                        this.selection.sessions.remove(&project.id);
                        this.history.retain(|id| !ids.contains(id));
                        this.history_ix = this.history_ix.min(this.history.len().saturating_sub(1));
                        this.collapsed_projects.remove(&project.id);
                        this.archived_projects.remove(&project.id);
                        for id in &ids { this.forget_session_cache(id); }
                        if failed > 0 { this.status = Some(format!("Project deleted, but {failed} local Pi history files could not be removed.")); }
                        cx.emit(WorkspaceEvent::ProjectRemoved(project.id.clone()));
                        this.finish_project_removal(&project.id, window, cx);
                        cx.notify();
                    }
                    Err(error) => {
                        for session in this.sessions.iter_mut().filter(|s| s.project == project.id) {
                            session.running = false;
                            session.status = "Pi stopped. Project deletion failed.".into();
                        }
                        this.status = Some(format!("Could not delete project: {error}. The project and its sessions have been kept."));
                        cx.notify();
                    }
                }
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn finish_project_removal(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_project.as_deref() == Some(id) {
            self.active_project = None;
            self.active_session = None;
            self.selection.project = None;
            self.composer
                .update(cx, |input, cx| input.set_value("", window, cx));
            if let Some(project) = self.projects.first().cloned() {
                self.select_project(&project.id, window, cx);
            }
        }
        self.persist_selection(cx);
        cx.notify();
    }

    pub(super) fn forget_session_cache(&mut self, id: &str) {
        self.expanded.retain(|(session, _)| session != id);
        self.lists.remove(id);
        self.models.remove(id);
        self.selected_model.remove(id);
        self.thinking.remove(id);
        self.thinking_levels.remove(id);
        self.commands.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        settings::Preferences,
        storage::Store,
        workspace::{Project, Session, Workspace},
    };
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};
    use serde_json::json;
    #[gpui_kit::test]
    async fn failed_project_delete_keeps_project_sessions_and_selection(cx: &mut TestAppContext) {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open(temporary.path().into()).unwrap();
        let project = Project {
            id: "p".into(),
            name: "Project".into(),
            path: temporary.path().into(),
        };
        let session: Session = serde_json::from_value(json!({"id":"a","project":"p","title":"Keep", "draft":"keep draft","pi_file":temporary.path().join("sessions/a.jsonl"),"messages":[],"archived":true})).unwrap();
        futures::executor::block_on(store.save("project", "p", &project)).unwrap();
        futures::executor::block_on(store.save("session", "a", &session)).unwrap();
        let database = rusqlite::Connection::open(temporary.path().join("desktop.sqlite")).unwrap();
        database.execute_batch("CREATE TRIGGER reject_project_delete BEFORE DELETE ON records WHEN OLD.kind='project' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
        cx.dispatcher.allow_parking();
        cx.update(gpui_kit::init);
        let (handle, workspace) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    Workspace::new(
                        store,
                        Preferences::default(),
                        vec![project],
                        vec![session],
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            workspace.update(cx, |this, cx| {
                this.active_project = Some("p".into());
                this.selection.project = Some("p".into());
                this.delete_project("p", window, cx);
            })
        })
        .unwrap();
        cx.condition(&workspace, |this, _| this.deleting_projects.is_empty())
            .await;
        cx.update(|cx| {
            let state = workspace.read(cx);
            assert_eq!(state.projects.len(), 1);
            assert_eq!(state.sessions.len(), 1);
            assert_eq!(state.selection.project.as_deref(), Some("p"));
            assert_eq!(state.sessions[0].draft, "keep draft");
            assert!(state.deleting_sessions.is_empty());
            assert!(
                state
                    .status
                    .as_ref()
                    .unwrap()
                    .contains("Could not delete project")
            );
        });
    }
}
