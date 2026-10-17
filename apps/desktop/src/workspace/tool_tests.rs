use super::{PiClient, PiEvent, Project, Session, Sidebar, Workspace};
use crate::{settings::Preferences, storage::Store};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Context, Entity, IntoElement, ParentElement, Render,
    Styled, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::mpsc::Receiver,
};

// Only composes production feature views; clicks and text input use their real handlers.
struct ChatWindow {
    workspace: Entity<Workspace>,
    sidebar: Entity<Sidebar>,
}

impl Render for ChatWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_stretch()
            .child(div().w(px(280.)).h_full().child(self.sidebar.clone()))
            .child(div().flex_1().min_w_0().child(self.workspace.clone()))
    }
}

struct Harness {
    window: AnyWindowHandle,
    workspace: Entity<Workspace>,
    receive: HashMap<String, Receiver<Value>>,
    store: Store,
}

fn harness(root: &Path, cx: &mut TestAppContext) -> Harness {
    let store = Store::open(root.join("data")).unwrap();
    let projects = ["p", "q"]
        .map(|id| Project {
            id: id.into(),
            name: id.into(),
            path: root.join(id),
        })
        .to_vec();
    let sessions = [("a", "p"), ("b", "q")]
        .map(|(id, project)| {
            serde_json::from_value::<Session>(json!({"id":id,"project":project,
            "title":id,"draft":format!("draft {id}"),"messages":[],
            "pi_file":store.directory().join(format!("sessions/{id}.jsonl"))}))
            .unwrap()
        })
        .to_vec();
    cx.dispatcher.allow_parking();
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut receive = HashMap::new();
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1300.), px(1000.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let workspace = cx.new(|cx| {
                    let mut workspace = Workspace::new(
                        store.clone(),
                        Preferences {
                            pi_path: "/does/not/exist".into(),
                            ..Default::default()
                        },
                        projects,
                        sessions,
                        window,
                        cx,
                    );
                    for id in ["a", "b"] {
                        let (client, channel) = PiClient::fixture_connection();
                        workspace.engines.insert(id.into(), client);
                        workspace.ready.insert(id.into());
                        receive.insert(id.into(), channel);
                    }
                    workspace.select_project("p", window, cx);
                    workspace
                });
                let sidebar = cx.new(|cx| Sidebar::new(workspace.clone(), cx));
                cx.new(|_| ChatWindow { workspace, sidebar })
            },
        )
        .unwrap()
    });
    let workspace = cx.update(|cx| view.read(cx).workspace.clone());
    Harness {
        window,
        workspace,
        receive,
        store,
    }
}

fn event(h: &Harness, session: &str, value: Value, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.workspace.update(cx, |workspace, cx| {
            workspace.pi_event(PiEvent::Json(session.into(), value), window, cx);
            for record in &workspace.sessions {
                assert_eq!(
                    record.messages.len(),
                    record
                        .messages
                        .iter()
                        .map(|m| &m.id)
                        .collect::<HashSet<_>>()
                        .len()
                );
            }
        });
        window.render_frame(cx);
    })
    .unwrap();
}

fn result(
    h: &Harness,
    session: &str,
    call: &str,
    tool: &str,
    text: &str,
    error: bool,
    cx: &mut TestAppContext,
) {
    let content = json!([{"type":"text","text":text}]);
    event(
        h,
        session,
        json!({"type":"tool_execution_end","toolCallId":call,"toolName":tool,"isError":error,"result":{"content":content}}),
        cx,
    );
    let message = json!({"role":"toolResult","toolCallId":call,"toolName":tool,"isError":error,"content":content,"timestamp":10});
    event(
        h,
        session,
        json!({"type":"message_start","message":message}),
        cx,
    );
    event(
        h,
        session,
        json!({"type":"message_end","message":message}),
        cx,
    );
}

#[gpui_kit::test]
async fn tools_keep_errors_and_identity_through_final_results(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let h = harness(root.path(), cx);
    for (ix, tool) in [
        "read",
        "read",
        "read",
        "write",
        "edit",
        "bash",
        "custom-tool",
    ]
    .iter()
    .enumerate()
    {
        let call = format!("call-{ix}");
        event(
            &h,
            "a",
            json!({"type":"tool_execution_start","toolCallId":call,"toolName":tool,"args":{"path":format!("{ix}.md")}}),
            cx,
        );
        event(
            &h,
            "a",
            json!({"type":"tool_execution_update","toolCallId":call,"toolName":tool,"partialResult":{"content":[{"type":"text","text":"partial 中文\n"}]}}),
            cx,
        );
        result(
            &h,
            "a",
            &call,
            tool,
            &format!("{tool} final output\n中文第二行"),
            *tool == "bash",
            cx,
        );
    }
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find_all_by_label("bash · Error").len(),
            1,
            "A failed command must remain visibly failed after its final toolResult"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
async fn switching_sessions_keeps_running_tools_drafts_and_expansion_separate(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let h = harness(root.path(), cx);
    for session in ["a", "b"] {
        event(&h, session, json!({"type":"agent_start"}), cx);
        event(
            &h,
            session,
            json!({"type":"message_start","message":{"role":"assistant","timestamp":1,"content":[{"type":"thinking","thinking":format!("private thinking {session}")},{"type":"text","text":format!("answer {session}")}]}}),
            cx,
        );
        event(
            &h,
            session,
            json!({"type":"tool_execution_start","toolCallId":"shared-call","toolName":"read","args":{"path":format!("{session}.md")}}),
            cx,
        );
    }
    cx.update_window(h.window, |_, window, cx| {
        window.click("thinking-assistant-1", cx);
        window.render_frame(cx);
        assert_eq!(window.find_all_by_label("Hide thinking").len(), 1);
        window.click("session-b", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find_all_by_label("Hide thinking").len(),
            0,
            "Expansion must belong to its session even when message IDs overlap"
        );
        assert_eq!(
            window.find_by_label("Message Pi").value(),
            Some("draft b")
        );
    })
    .unwrap();
    for ix in 0..12 {
        let session = if ix % 2 == 0 { "a" } else { "b" };
        event(
            &h,
            session,
            json!({"type":"tool_execution_update","toolCallId":"shared-call","toolName":"read","partialResult":{"content":[{"type":"text","text":format!("{session} chunk {ix}")}]}}),
            cx,
        );
        cx.update_window(h.window, |_, window, cx| {
            window.click(format!("session-{session}"), cx);
            window.render_frame(cx);
            assert_eq!(
                window.find_by_label("Message Pi").value(),
                Some(format!("draft {session}").as_str())
            );
        })
        .unwrap();
    }
    result(&h, "a", "shared-call", "read", "file a", false, cx);
    result(&h, "b", "shared-call", "bash", "command b", false, cx);
    event(&h, "a", json!({"type":"agent_settled"}), cx);
    event(&h, "b", json!({"type":"agent_settled"}), cx);
    cx.update(|cx| {
        let workspace = h.workspace.read(cx);
        assert!(workspace.sessions.iter().all(|s| !s.running));
        assert_eq!(workspace.sessions[0].messages[1].text, "file a");
        assert_eq!(workspace.sessions[1].messages[1].text, "command b");
        assert_eq!(workspace.engines.len(), 2);
    });
    for requests in h.receive.values() {
        assert!(
            requests.try_iter().all(|v| v["type"] == "get_messages"),
            "Switching must not prompt, abort, or restart an engine"
        );
    }
    let store = h.store.clone();
    let (_, sessions) = cx
        .background_spawn(async move {
            store.flush().await.unwrap();
            Workspace::load(&store).await.unwrap()
        })
        .await;
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].draft, "draft a");
    assert_eq!(sessions[1].draft, "draft b");
}

#[gpui_kit::test]
async fn delayed_prompt_ack_and_stop_target_only_their_session(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let h = harness(root.path(), cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("send-message", cx);
        window.click("session-b", cx);
        let input = window
            .find_by_label("Message Pi")
            .path()
            .last()
            .unwrap()
            .clone();
        window.click(input, cx);
        window.press("cmd-a", cx);
        window.input("draft B edited while A is working", cx);
    })
    .unwrap();
    let prompt = h.receive["a"].try_recv().unwrap();
    assert_eq!(prompt["type"], "prompt");
    event(&h, "a", json!({"type":"agent_start"}), cx);
    event(
        &h,
        "a",
        json!({"type":"response","id":prompt["id"],"success":true,"data":{"disposition":"started"}}),
        cx,
    );
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find_by_label("Message Pi").value(),
            Some("draft B edited while A is working")
        );
        window.click("session-a", cx);
        assert_eq!(window.find_by_label("Message Pi").value(), Some(""));
        window.click("send-message", cx);
    })
    .unwrap();
    let requests = h.receive["a"].try_iter().collect::<Vec<_>>();
    assert_eq!(
        requests
            .iter()
            .map(|v| v["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["get_state", "clear_queue", "abort"]
    );
    assert!(h.receive["b"].try_recv().is_err());
    let abort = requests.last().unwrap();
    event(
        &h,
        "a",
        json!({"type":"response","id":abort["id"],"success":true,"data":{}}),
        cx,
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click("session-b", cx);
        assert_eq!(
            window.find_by_label("Message Pi").value(),
            Some("draft B edited while A is working")
        );
    })
    .unwrap();
    cx.update(|cx| {
        let workspace = h.workspace.read(cx);
        assert!(!workspace.sessions[0].running);
        assert!(workspace.sessions[0].draft.is_empty());
        assert_eq!(
            workspace.sessions[1].draft,
            "draft B edited while A is working"
        );
    });
}

#[gpui_kit::test]
async fn history_sync_keeps_tool_metadata_and_background_session_isolated(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let h = harness(root.path(), cx);
    event(
        &h,
        "a",
        json!({"type":"tool_execution_start","toolCallId":"history-read","toolName":"read","args":{"path":"first.md"}}),
        cx,
    );
    cx.update_window(h.window, |_, window, cx| window.click("session-b", cx))
        .unwrap();
    let request = cx.update(|cx| {
        h.workspace.update(cx, |w, _| {
            w.engines
                .get_mut("a")
                .unwrap()
                .request("get_messages", json!({}))
                .unwrap()
        })
    });
    let message = json!({"role":"toolResult","toolCallId":"history-read","toolName":"read","isError":true,"content":[{"type":"text","text":"File missing"}],"timestamp":2});
    event(
        &h,
        "a",
        json!({"type":"response","id":request,"success":true,"data":{"messages":[message,message]}}),
        cx,
    );
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(window.find_by_label("Message Pi").value(), Some("draft b"));
        assert!(window.try_find("tool-history-read").is_none());
        window.click("session-a", cx);
        window.render_frame(cx);
        assert_eq!(window.find_all_by_label("read · Error").len(), 1);
        assert_eq!(window.find_all_by_label("first.md").len(), 1);
    })
    .unwrap();
    cx.update(|cx| {
        let workspace = h.workspace.read(cx);
        assert_eq!(workspace.sessions[0].messages.len(), 1);
        assert!(workspace.sessions[0].messages[0].is_error);
        assert!(workspace.sessions[1].messages.is_empty());
    });
    assert_eq!(h.receive["a"].try_iter().count(), 1);
    assert!(h.receive["b"].try_recv().is_err());
}
