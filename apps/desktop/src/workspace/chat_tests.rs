use super::{PiEvent, Project, Session, Workspace};
use crate::{settings::Preferences, storage::Store};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size};
use serde_json::json;
use std::collections::HashSet;

#[gpui_kit::test]
async fn read_tool_events_keep_one_accessible_message_row(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(temporary.path().join("data")).unwrap();
    let session: Session = serde_json::from_value(json!({
        "id":"s", "project":"p", "title":"Read file", "draft":"",
        "pi_file":store.directory().join("sessions/s.jsonl"), "messages":[]
    }))
    .unwrap();
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    let (handle, workspace) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1000.), px(800.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::new(
                        store.clone(),
                        Preferences::default(),
                        vec![Project {
                            id: "p".into(),
                            name: "Fixture".into(),
                            path: temporary.path().into(),
                        }],
                        vec![session],
                        window,
                        cx,
                    );
                    workspace.active_project = Some("p".into());
                    workspace.active_session = Some("s".into());
                    workspace
                })
            },
        )
        .unwrap()
    });
    let result = json!({"role":"toolResult", "toolCallId":"read-1", "toolName":"read",
        "content":[{"type":"text","text":"# File contents\n\nA file read by Pi."}], "timestamp":3});
    let second_result = json!({"role":"toolResult", "toolCallId":"read-2", "toolName":"read",
        "content":[{"type":"text","text":"Second file contents"}], "timestamp":4});
    let events = [
        json!({"type":"message_start","message":{"role":"user","content":"Read README.md","timestamp":1}}),
        json!({"type":"message_end","message":{"role":"user","content":"Read README.md","timestamp":1}}),
        json!({"type":"message_start","message":{"role":"assistant","content":[],"timestamp":2}}),
        json!({"type":"message_end","message":{"role":"assistant","content":[{"type":"toolCall","id":"read-1","name":"read","arguments":{"path":"README.md"}}],"timestamp":2}}),
        json!({"type":"tool_execution_start","toolCallId":"read-1","toolName":"read","args":{"path":"README.md"}}),
        json!({"type":"tool_execution_start","toolCallId":"read-2","toolName":"read","args":{"path":"NOTES.md"}}),
        json!({"type":"tool_execution_update","toolCallId":"read-1","toolName":"read","partialResult":{"content":[{"type":"text","text":"Partial result"}]}}),
        json!({"type":"tool_execution_end","toolCallId":"read-1","toolName":"read","result":{"content":result["content"]},"isError":false}),
        json!({"type":"message_start","message":result}),
        json!({"type":"message_end","message":result}),
        json!({"type":"message_end","message":result}),
        json!({"type":"tool_execution_end","toolCallId":"read-2","toolName":"read","result":{"content":second_result["content"]},"isError":false}),
        json!({"type":"message_start","message":second_result}),
        json!({"type":"message_end","message":second_result}),
        json!({"type":"message_start","message":{"role":"assistant","content":[],"timestamp":5}}),
        json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"Read both files."}}),
        json!({"type":"message_end","message":{"role":"assistant","content":"Read both files.","timestamp":5}}),
        json!({"type":"agent_settled"}),
    ];
    for event in events {
        cx.update_window(handle, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.pi_event(PiEvent::Json("s".into(), event.clone()), window, cx);
                let messages = &workspace.sessions[0].messages;
                assert_eq!(
                    messages.len(),
                    messages.iter().map(|m| &m.id).collect::<HashSet<_>>().len(),
                    "Duplicate chat row identity after {} produces duplicate accessibility nodes",
                    event["type"]
                );
            });
            window.render_frame(cx);
        })
        .unwrap();
    }
    cx.update(|cx| {
        let messages = &workspace.read(cx).sessions[0].messages;
        assert_eq!(messages.len(), 5);
        assert_eq!(
            messages[2].file.as_deref(),
            Some(std::path::Path::new("README.md"))
        );
        assert_eq!(messages[2].text, "# File contents\n\nA file read by Pi.");
        assert_eq!(
            messages[3].file.as_deref(),
            Some(std::path::Path::new("NOTES.md"))
        );
        assert_eq!(messages[3].text, "Second file contents");
        assert_eq!(messages[4].text, "Read both files.");
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_all_by_label("read").len(), 2);
        window.click("tool-read-1", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update(|cx| {
        assert!(
            workspace
                .read(cx)
                .expanded
                .contains(&("s".into(), "read-1".into()))
        )
    });
}

#[test]
fn recovered_projection_merges_duplicate_rows_without_losing_file_links() {
    futures::executor::block_on(async {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open(temporary.path().into()).unwrap();
        let session: Session = serde_json::from_value(json!({
            "id":"s", "project":"p", "title":"Recovered", "draft":"unsent draft",
            "pi_file":store.directory().join("sessions/s.jsonl"), "messages":[
                {"id":"read-1", "role":"tool", "text":"read", "thinking":"", "file":"README.md"},
                {"id":"read-1", "role":"toolResult", "text":"Completed file contents", "thinking":"", "file":null}
            ]
        })).unwrap();
        store.save("session", "s", &session).await.unwrap();
        let (_, sessions) = Workspace::load(&store).await.unwrap();
        assert_eq!(sessions[0].messages.len(), 1);
        assert_eq!(sessions[0].messages[0].role, "toolResult");
        assert_eq!(sessions[0].messages[0].text, "Completed file contents");
        assert_eq!(
            sessions[0].messages[0].file.as_deref(),
            Some(std::path::Path::new("README.md"))
        );
        assert_eq!(sessions[0].draft, "unsent draft");
        assert!(!sessions[0].running);
    });
}
