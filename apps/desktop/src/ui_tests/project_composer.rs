use crate::{
    settings::Preferences,
    shell::{Desktop, Layout},
    storage::Store,
    workspace::{Project, Session},
};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size,
};
use std::{path::Path, time::Duration};

fn desktop(root: &Path, cx: &mut TestAppContext) -> (AnyWindowHandle, Store) {
    let store = Store::open(root.join("data")).unwrap();
    let projects = ["p1", "p2"]
        .map(|id| {
            let path = root.join(id);
            std::fs::create_dir_all(path.join("docs")).unwrap();
            std::fs::write(path.join("README.md"), "Keep project files").unwrap();
            std::fs::write(path.join("docs/note with spaces.md"), "中文").unwrap();
            Project {
                id: id.into(),
                name: id.into(),
                path,
            }
        })
        .to_vec();
    let sessions = [("a", "p1", false), ("archived", "p1", true), ("b", "p2", false)].map(|(id, project, archived)| {
        let file = store.directory().join(format!("sessions/{id}.jsonl"));
        std::fs::write(&file, "fixture history").unwrap();
        serde_json::from_value::<Session>(serde_json::json!({"id":id,"project":project,"title":id,"draft":"", "pi_file":file,"messages":[],"archived":archived})).unwrap()
    }).to_vec();
    for project in &projects {
        futures::executor::block_on(store.save("project", &project.id, project)).unwrap();
    }
    for session in &sessions {
        futures::executor::block_on(store.save("session", &session.id, session)).unwrap();
    }
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update(crate::shell::menus);
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
    (handle, store)
}

async fn wait_label(handle: AnyWindowHandle, label: &str, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    for _ in 0..100 {
        if cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.try_find_by_label(label).is_some()
            })
            .unwrap()
        {
            return;
        }
        cx.background_spawn(async { std::thread::sleep(Duration::from_millis(5)) })
            .await;
        cx.run_until_parked();
    }
    panic!("Missing label {label}");
}

#[gpui_kit::test]
async fn project_delete_cancels_and_then_removes_only_its_records(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let (handle, store) = desktop(temporary.path(), cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("project-p1", cx);
        window.press("cmd-shift-t", cx);
        window.render_frame(cx);
        assert_eq!(window.find_all_by_label("Close Terminal").len(), 1);
        window.click("project-actions-p1", cx);
        let item = window
            .find_by_label("Delete Project…")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(item, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("confirm-delete-project").visible());
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.find("project-p1").visible());
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click("project-actions-p1", cx);
        let item = window
            .find_by_label("Delete Project…")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(item, cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("confirm-delete-project", cx);
        window.render_frame(cx);
    })
    .unwrap();
    for attempt in 0..200 {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(25));
        cx.run_until_parked();
        let checkpoint = store.clone();
        let removed = cx
            .background_spawn(async move {
                checkpoint
                    .get::<Project>("project", "p1")
                    .await
                    .unwrap()
                    .is_none()
            })
            .await;
        if removed {
            break;
        }
        assert!(attempt < 199, "Project deletion did not finish");
    }
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("project-p1").is_none());
        assert!(window.try_find("session-a").is_none());
        assert!(window.find("project-p2").visible());
        assert!(window.find("session-b").visible());
        assert!(window.find_all_by_label("Close Terminal").is_empty());
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(temporary.path().join("p1/README.md")).unwrap(),
        "Keep project files"
    );
    assert!(!store.directory().join("sessions/a.jsonl").exists());
    assert!(!store.directory().join("sessions/archived.jsonl").exists());
    assert!(store.directory().join("sessions/b.jsonl").exists());
    let reopened = Store::open(store.directory().into()).unwrap();
    let (projects, sessions) = cx
        .background_spawn(
            async move { crate::workspace::Workspace::load(&reopened).await.unwrap() },
        )
        .await;
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, "p2");
    assert_eq!(sessions.len(), 1);
}

#[gpui_kit::test]
async fn composer_commands_and_file_mentions_use_production_controls(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let (handle, store) = desktop(temporary.path(), cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("project-p1", cx);
        window.click("session-a", cx);
        let input = window
            .find_by_label("Message Pi")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(input, cx);
        window.input("/", cx);
    })
    .unwrap();
    wait_label(handle, "/new", cx).await;
    cx.update_window(handle, |_, window, cx| {
        window.press("down", cx);
        window.press("tab", cx);
        window.render_frame(cx);
        assert_eq!(window.find_by_label("Message Pi").value(), Some("/rename "));
        window.input("Renamed through command", cx);
        window.press("enter", cx);
    })
    .unwrap();
    let checkpoint = store.clone();
    cx.background_spawn(async move { checkpoint.flush().await.unwrap() })
        .await;
    cx.run_until_parked();
    let checkpoint = store.clone();
    let session = cx
        .background_spawn(async move {
            checkpoint
                .get::<serde_json::Value>("session", "a")
                .await
                .unwrap()
                .unwrap()
        })
        .await;
    assert_eq!(session["title"], "Renamed through command");
    assert_eq!(session["messages"].as_array().unwrap().len(), 0);
    cx.update_window(handle, |_, window, cx| {
        window.input("@note", cx);
    })
    .unwrap();
    wait_label(handle, "docs/note with spaces.md", cx).await;
    cx.update_window(handle, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find_by_label("Message Pi").value(),
            Some("@\"docs/note with spaces.md\" ")
        );
        assert!(window.find_by_label("note with spaces.md").visible());
        window.press("cmd-a", cx);
        window.input("/settings", cx);
        window.press("escape", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("save-paths").visible());
        window.press("escape", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let input = window
            .find_by_label("Message Pi")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(input, cx);
        window.input("@README", cx);
        window.click("project-p2", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find_by_label("README.md").is_none(),
            "A stale file query must not appear in another project"
        );
        assert_eq!(window.find_by_label("Message Pi").value(), Some(""));
    })
    .unwrap();
}
