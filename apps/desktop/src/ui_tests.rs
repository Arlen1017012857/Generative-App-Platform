use crate::{
    settings::{Appearance, Preferences, Settings},
    shell::{Desktop, Layout},
    storage::Store,
};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size};
mod project_composer;

#[gpui_kit::test]
async fn panel_toggles_stay_at_the_window_edges(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(temporary.path().into()).unwrap();
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update(crate::shell::menus);
    for scale in [100, 125, 150] {
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(1800.), px(900.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        Desktop::new(
                            store.clone(),
                            Preferences {
                                scale,
                                ..Default::default()
                            },
                            vec![],
                            vec![],
                            Layout::default(),
                            window,
                            cx,
                        )
                    })
                },
            )
            .unwrap()
        });
        let (left, right_inset) = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let left = window.find("toggle-sidebar").bounds();
                let right = window.find("toggle-workspace").bounds();
                for control in [
                    "toggle-sidebar",
                    "toggle-workspace",
                    "toggle-sidebar",
                    "toggle-workspace",
                ] {
                    window.click(control, cx);
                    window.render_frame(cx);
                    assert_eq!(
                        window.find("toggle-sidebar").bounds(),
                        left,
                        "Sidebar toggle moved at {scale}% after {control}"
                    );
                    assert_eq!(
                        window.find("toggle-workspace").bounds(),
                        right,
                        "Workspace toggle moved at {scale}% after {control}"
                    );
                }
                (left, window.viewport_size().width - right.right())
            })
            .unwrap();
        cx.simulate_window_resize(handle, size(px(700.), px(900.)));
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("toggle-sidebar").bounds(), left);
            assert_eq!(
                window.viewport_size().width - window.find("toggle-workspace").bounds().right(),
                right_inset
            );
            assert!(window.try_find("files-tab").is_none());
            for control in ["toggle-sidebar", "toggle-workspace"] {
                window.click(control, cx);
                window.render_frame(cx);
                assert_eq!(window.find("toggle-sidebar").bounds(), left);
                assert_eq!(
                    window.viewport_size().width - window.find("toggle-workspace").bounds().right(),
                    right_inset
                );
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
async fn session_archive_restore_and_confirmed_delete_persist(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(temporary.path().join("data")).unwrap();
    let project = crate::workspace::Project {
        id: "p".into(),
        name: "Fixture Project".into(),
        path: temporary.path().into(),
    };
    let sessions = ["a", "b"]
        .map(|id| {
            let file = store
                .directory()
                .join("sessions")
                .join(format!("{id}.jsonl"));
            std::fs::write(&file, "fixture Pi history").unwrap();
            serde_json::from_value::<crate::workspace::Session>(serde_json::json!({
                "id":id,"project":"p","title":id,"draft":format!("draft {id}"),
                "pi_file":file,"messages":[]
            }))
            .unwrap()
        })
        .to_vec();
    for session in &sessions {
        futures::executor::block_on(store.save("session", &session.id, session)).unwrap();
    }
    futures::executor::block_on(store.save("project", "p", &project)).unwrap();
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1600.), px(900.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    Desktop::new(
                        store.clone(),
                        Preferences {
                            pi_path: "/does/not/exist".into(),
                            ..Default::default()
                        },
                        vec![project],
                        sessions,
                        Layout::default(),
                        window,
                        cx,
                    )
                })
            },
        )
        .unwrap()
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("project-p", cx);
        window.click("session-a", cx);
        let composer = window
            .find_by_label("Message Pi")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(composer, cx);
        window.press("cmd-a", cx);
        window.input("keep archived draft 中文", cx);
        window.click("session-actions-sidebar-a", cx);
        let archive = window
            .find_by_label("Archive Session")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(archive, cx);
        window.render_frame(cx);
        assert!(window.find("archived-p").visible());
        assert_eq!(window.find_by_label("Message Pi").value(), Some("draft b"));
    })
    .unwrap();
    let checkpoint = store.clone();
    cx.background_spawn(async move { checkpoint.flush().await.unwrap() })
        .await;
    cx.run_until_parked();
    let checkpoint = store.clone();
    let archived = cx
        .background_spawn(async move {
            checkpoint
                .get::<serde_json::Value>("session", "a")
                .await
                .unwrap()
                .unwrap()
        })
        .await;
    assert_eq!(archived["archived"], true);
    assert_eq!(archived["draft"], "keep archived draft 中文");
    cx.update_window(handle, |_, window, cx| {
        window.click("session-a", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find_by_label("Message Pi").value(),
            Some("keep archived draft 中文")
        );
        assert_eq!(window.find("send-message").disabled(), Some(true));
        let composer = window
            .find_by_label("Message Pi")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(composer, cx);
        window.input("must not edit archive", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find_by_label("Message Pi").value(),
            Some("keep archived draft 中文")
        );
        window.click("session-actions-sidebar-a", cx);
        let restore = window
            .find_by_label("Restore Session")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(restore, cx);
    })
    .unwrap();
    let checkpoint = store.clone();
    cx.background_spawn(async move { checkpoint.flush().await.unwrap() })
        .await;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("archived-p").is_none());
        window.click("session-actions-sidebar-a", cx);
        let delete = window
            .find_by_label("Delete Session…")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(delete, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("confirm-delete-session").visible());
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("confirm-delete-session").is_none());
        assert!(window.find("session-a").visible());
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("session-actions-sidebar-a", cx);
        let delete = window
            .find_by_label("Delete Session…")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(delete, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("confirm-delete-session", cx);
    })
    .unwrap();
    let checkpoint = store.clone();
    cx.background_spawn(async move {
        for _ in 0..200 {
            if checkpoint
                .get::<serde_json::Value>("session", "a")
                .await
                .unwrap()
                .is_none()
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("Session deletion did not complete");
    })
    .await;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("session-a").is_none());
        assert_eq!(window.find_by_label("Message Pi").value(), Some("draft b"));
    })
    .unwrap();
    assert!(!store.directory().join("sessions/a.jsonl").exists());
    assert!(store.directory().join("sessions/b.jsonl").exists());
    let reopened = Store::open(store.directory().into()).unwrap();
    let (_, sessions) = cx
        .background_spawn(
            async move { crate::workspace::Workspace::load(&reopened).await.unwrap() },
        )
        .await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "b");
}

#[gpui_kit::test]
async fn production_sessions_keep_drafts_and_workspace_tabs_switch(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(temporary.path().join("data")).unwrap();
    let projects = ["p1", "p2"]
        .map(|id| crate::workspace::Project {
            id: id.into(),
            name: id.into(),
            path: temporary.path().into(),
        })
        .to_vec();
    let sessions = [
        ("a", "p1", "draft A"),
        ("b", "p1", "draft B"),
        ("c", "p2", "draft C"),
    ]
    .map(|(id, project, draft)| {
        serde_json::from_value::<crate::workspace::Session>(serde_json::json!({
            "id":id,"project":project,"title":id,"draft":draft,"messages":[],
            "pi_file":temporary.path().join(format!("{id}.jsonl"))
        }))
        .unwrap()
    })
    .to_vec();
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update(crate::shell::menus);
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1408.), px(770.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    Desktop::new(
                        store,
                        Preferences {
                            pi_path: "/does/not/exist".into(),
                            shell_path: "/bin/sh".into(),
                            ..Default::default()
                        },
                        projects,
                        sessions,
                        Layout::default(),
                        window,
                        cx,
                    )
                })
            },
        )
        .unwrap()
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("project-p1", cx);
        window.click("session-a", cx);
        window.render_frame(cx);
        assert_eq!(window.find_by_label("Message Pi").value(), Some("draft A"));
        let composer = window
            .find_by_label("Message Pi")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(composer, cx);
        window.press("cmd-a", cx);
        window.input("updated draft", cx);
        window.click("session-b", cx);
        window.render_frame(cx);
        assert_eq!(window.find_by_label("Message Pi").value(), Some("draft B"));
        window.click("project-p2", cx);
        window.render_frame(cx);
        assert_eq!(window.find_by_label("Message Pi").value(), Some("draft C"));
        window.click("project-p1", cx);
        window.click("session-a", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find_by_label("Message Pi").value(),
            Some("updated draft")
        );
        window.press("cmd-shift-t", cx);
        window.render_frame(cx);
        assert_eq!(window.find_all_by_label("Close Terminal").len(), 1);
        window.click("files-tab", cx);
        window.render_frame(cx);
        assert!(window.find("file-path").visible());
        let terminal = window
            .find_by_label("Terminal · p1")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(terminal, cx);
        window.render_frame(cx);
        assert!(window.try_find("file-path").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn opens_projects_and_keeps_only_two_workspace_types(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().join("data")).unwrap();
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, _desktop) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1408.), px(770.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    Desktop::new(
                        store,
                        Preferences {
                            pi_path: "/does/not/exist".into(),
                            ..Default::default()
                        },
                        vec![],
                        vec![],
                        Layout::default(),
                        window,
                        cx,
                    )
                })
            },
        )
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("files-tab").visible());
        window.click("open-first-project", cx);
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.directories);
        assert!(!options.files);
        Some(vec![directory.path().into()])
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("new-session").visible());
        assert!(window.find("files-tab").visible());
        assert!(window.try_find("browser-tab").is_none());
        assert!(window.try_find("code-tab").is_none());
        let divider = window
            .find_all(("resizable-handle", 0usize))
            .into_iter()
            .min_by(|a, b| {
                a.bounds()
                    .origin
                    .x
                    .partial_cmp(&b.bounds().origin.x)
                    .unwrap()
            })
            .unwrap()
            .bounds()
            .center();
        window.drag(divider, gpui_kit::point(divider.x + px(80.), divider.y), cx);
        window.render_frame(cx);
        let resized = window
            .find_all(("resizable-handle", 0usize))
            .into_iter()
            .min_by(|a, b| {
                a.bounds()
                    .origin
                    .x
                    .partial_cmp(&b.bounds().origin.x)
                    .unwrap()
            })
            .unwrap()
            .bounds()
            .center();
        assert!(
            resized.x > divider.x + px(40.),
            "The production divider must move during a drag"
        );
        let close = window.find("close-files").bounds();
        assert!(close.size.width > px(0.) && close.size.height > px(0.));
        let focus = window.focused(cx);
        window.click("open-settings", cx);
        window.render_frame(cx);
        assert!(window.find("save-paths").visible());
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("save-paths").is_none());
        assert_eq!(
            window.focused(cx),
            focus,
            "Closing Settings restores the previous focus"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn settings_reject_invalid_path_through_production_controls(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(temporary.path().into()).unwrap();
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, settings) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Settings::new(store, Preferences::default(), window, cx))
        })
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_ne!(window.find("save-paths").disabled(), Some(true));
        assert!(window.find("open-data-directory").visible());
        window.click("shell-executable", cx);
        window.input("/does/not/exist", cx);
        window.click("save-paths", cx);
    })
    .unwrap();
    cx.condition(&settings, |settings, _| settings.has_validation_error())
        .await;
    cx.update(|cx| {
        assert_eq!(
            settings.read(cx).preferences().appearance,
            Appearance::System
        );
        assert!(settings.read(cx).preferences().shell_path.is_empty());
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("shell-executable").value(),
            Some("/does/not/exist")
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn settings_database_failure_preserves_input_and_last_valid_configuration(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(temporary.path().into()).unwrap();
    // Emulate a genuine failed disk commit without substituting the production Store.
    rusqlite::Connection::open(temporary.path().join("desktop.sqlite"))
        .unwrap()
        .execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON records WHEN NEW.kind='preferences' BEGIN SELECT RAISE(ABORT, 'simulated storage failure'); END;")
        .unwrap();
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, settings) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Settings::new(store, Preferences::default(), window, cx))
        })
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("shell-executable", cx);
        window.input("/bin/sh", cx);
        window.click("save-paths", cx);
    })
    .unwrap();
    cx.condition(&settings, |settings, _| settings.has_validation_error())
        .await;
    cx.update(|cx| assert!(settings.read(cx).preferences().shell_path.is_empty()));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("shell-executable").value(), Some("/bin/sh"));
    })
    .unwrap();
}
