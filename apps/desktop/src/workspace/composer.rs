use super::*;
use gpui_kit::component::list::{
    List as SuggestionList, ListDelegate, ListItem, ListState as SuggestionListState,
};
use std::{ops::Range, path::Path};

gpui_kit::actions!(
    composer,
    [
        NextSuggestion,
        PreviousSuggestion,
        FirstSuggestion,
        LastSuggestion,
        AcceptSuggestion,
        DismissSuggestions
    ]
);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", NextSuggestion, Some("ComposerSuggestions > Input")),
        KeyBinding::new(
            "up",
            PreviousSuggestion,
            Some("ComposerSuggestions > Input"),
        ),
        KeyBinding::new("home", FirstSuggestion, Some("ComposerSuggestions > Input")),
        KeyBinding::new("end", LastSuggestion, Some("ComposerSuggestions > Input")),
        KeyBinding::new(
            "enter",
            AcceptSuggestion,
            Some("ComposerSuggestions > Input"),
        ),
        KeyBinding::new("tab", AcceptSuggestion, Some("ComposerSuggestions > Input")),
        KeyBinding::new(
            "escape",
            DismissSuggestions,
            Some("ComposerSuggestions > Input"),
        ),
    ]);
}

#[derive(Clone, Debug, PartialEq)]
enum TriggerKind {
    Command,
    File,
}

#[derive(Clone, Debug, PartialEq)]
struct Trigger {
    kind: TriggerKind,
    range: Range<usize>,
    query: String,
}

impl Trigger {
    fn at(text: &str, cursor: usize) -> Option<Self> {
        let prefix = text.get(..cursor)?;
        if let Some(query) = prefix.strip_prefix('/')
            && !query.chars().any(char::is_whitespace)
        {
            return Some(Self {
                kind: TriggerKind::Command,
                range: 0..cursor,
                query: query.into(),
            });
        }
        let start = prefix.rfind('@')?;
        if start > 0 && !prefix[..start].chars().next_back()?.is_whitespace() {
            return None;
        }
        let query = &prefix[start + 1..];
        let query = if let Some(quoted) = query.strip_prefix('"') {
            if quoted.contains('"') || quoted.contains('\n') {
                return None;
            }
            quoted
        } else {
            if query.chars().any(char::is_whitespace) {
                return None;
            }
            query
        };
        Some(Self {
            kind: TriggerKind::File,
            range: start..cursor,
            query: query.into(),
        })
    }
}

#[derive(Clone, Deserialize)]
pub(super) struct PiCommand {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    source: String,
}

const LOCAL_COMMANDS: &[(&str, &str)] = &[
    ("new", "Start a new session"),
    ("rename", "Rename this session"),
    ("model", "Choose the Pi model"),
    ("settings", "Open desktop settings"),
    ("stop", "Stop the current Pi task"),
    ("help", "Show available commands"),
];

#[derive(Clone)]
struct Suggestion {
    key: String,
    label: String,
    detail: String,
    insert: String,
    file: Option<PathBuf>,
}

pub(super) struct Suggestions {
    owner: WeakEntity<Workspace>,
    rows: Vec<Suggestion>,
    selected: Option<IndexPath>,
    message: String,
}

impl ListDelegate for Suggestions {
    type Item = ListItem;
    fn items_count(&self, _: usize, _: &App) -> usize {
        self.rows.len()
    }
    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<SuggestionListState<Self>>,
    ) -> Option<Self::Item> {
        let item = self.rows.get(ix.row)?;
        Some(
            ListItem::new(SharedString::from(format!("suggestion-{}", item.key)))
                .accessibility_label(item.label.clone())
                .role(Role::ListItem)
                .selected(self.selected == Some(ix))
                .child(
                    div()
                        .h_flex()
                        .gap_2()
                        .w_full()
                        .min_w_0()
                        .child(
                            Icon::new(if item.file.is_some() {
                                IconName::File
                            } else {
                                IconName::Command
                            })
                            .small(),
                        )
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_sm().truncate().child(item.label.clone()))
                                .child(
                                    div()
                                        .text_xs()
                                        .truncate()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(item.detail.clone()),
                                ),
                        ),
                ),
        )
    }
    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<SuggestionListState<Self>>,
    ) {
        self.selected = ix;
        cx.notify();
    }
    fn confirm(
        &mut self,
        _: bool,
        window: &mut Window,
        cx: &mut Context<SuggestionListState<Self>>,
    ) {
        if let Some(item) = self.selected.and_then(|ix| self.rows.get(ix.row)).cloned() {
            let owner = self.owner.clone();
            window.defer(cx, move |window, cx| {
                let _ = owner.update(cx, |this, cx| this.apply_suggestion(item, window, cx));
            });
        }
    }
    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<SuggestionListState<Self>>,
    ) -> impl IntoElement {
        div()
            .p_3()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(self.message.clone())
    }
}

pub(super) struct Completion {
    pub(super) list: Entity<SuggestionListState<Suggestions>>,
    trigger: Option<Trigger>,
    pub(super) input: Option<(String, String, usize)>,
    revision: u64,
    task: Task<()>,
}

impl Completion {
    pub(super) fn new(
        owner: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Self {
        let list = cx.new(|cx| {
            SuggestionListState::new(
                Suggestions {
                    owner,
                    rows: vec![],
                    selected: None,
                    message: String::new(),
                },
                window,
                cx,
            )
        });
        Self {
            list,
            trigger: None,
            input: None,
            revision: 0,
            task: Task::ready(()),
        }
    }
}

/// Bounded traversal runs in the background. Symlinks are not followed.
fn find_files(root: &Path, query: &str) -> Result<Vec<Suggestion>> {
    let root = root.canonicalize()?;
    let mut directories = vec![root.clone()];
    let mut visited = 0;
    let mut results = Vec::new();
    let query = query.to_lowercase();
    while let Some(directory) = directories.pop() {
        let mut entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
            Err(_) => continue,
        };
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            visited += 1;
            if visited > 20_000 {
                break;
            }
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                if !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | ".tools" | "node_modules" | "target" | "dist" | "vendor")
                ) {
                    directories.push(path);
                }
            } else if kind.is_file() {
                let relative = path.strip_prefix(&root)?.to_string_lossy().to_string();
                if relative.to_lowercase().contains(&query) && !relative.contains(['"', '\n', '\r'])
                {
                    let insert = if relative.chars().any(char::is_whitespace) {
                        format!("@\"{relative}\" ")
                    } else {
                        format!("@{relative} ")
                    };
                    results.push(Suggestion {
                        key: format!("file-{relative}"),
                        label: relative.clone(),
                        detail: "Project file".into(),
                        insert,
                        file: Some(path),
                    });
                }
            }
        }
        if visited > 20_000 {
            break;
        }
    }
    results.sort_by_key(|item| (item.label.len(), item.label.clone()));
    results.truncate(80);
    Ok(results)
}

impl Workspace {
    pub(super) fn refresh_suggestions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            self.dismiss_suggestions(cx);
            return;
        };
        if session.archived || self.deleting_sessions.contains(&session.id) {
            self.dismiss_suggestions(cx);
            return;
        }
        let id = session.id.clone();
        let (text, cursor, selected) = {
            let input = self.composer.read(cx);
            (
                input.value().to_string(),
                input.cursor(),
                !input.selected_range().is_empty(),
            )
        };
        let composing = self.composer.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        });
        if composing {
            self.completion.input = None;
            self.dismiss_suggestions(cx);
            return;
        }
        if self.completion.input.as_ref() == Some(&(id.clone(), text.clone(), cursor)) {
            return;
        }
        self.completion.input = Some((id.clone(), text.clone(), cursor));
        self.completion.revision += 1;
        self.completion.trigger = (!selected).then(|| Trigger::at(&text, cursor)).flatten();
        let Some(trigger) = self.completion.trigger.clone() else {
            cx.notify();
            return;
        };
        match trigger.kind {
            TriggerKind::Command => {
                let mut rows = LOCAL_COMMANDS
                    .iter()
                    .map(|(name, description)| Suggestion {
                        key: format!("command-{name}"),
                        label: format!("/{name}"),
                        detail: (*description).into(),
                        insert: format!("/{name} "),
                        file: None,
                    })
                    .collect::<Vec<_>>();
                for command in self.commands.get(&id).into_iter().flatten() {
                    if LOCAL_COMMANDS.iter().any(|(name, _)| name == &command.name) {
                        continue;
                    }
                    rows.push(Suggestion {
                        key: format!("command-{}", command.name),
                        label: format!("/{}", command.name),
                        detail: format!("{} · {}", command.source, command.description),
                        insert: format!("/{} ", command.name),
                        file: None,
                    });
                }
                rows.retain(|item| {
                    item.label
                        .to_lowercase()
                        .contains(&trigger.query.to_lowercase())
                });
                self.set_suggestions(rows, "No matching commands", window, cx);
            }
            TriggerKind::File => {
                let Some(project) = self.project().cloned() else {
                    return;
                };
                self.set_suggestions(vec![], "Searching project files…", window, cx);
                let revision = self.completion.revision;
                self.completion.task = cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(120))
                        .await;
                    let result = cx
                        .background_spawn(async move { find_files(&project.path, &trigger.query) })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.completion.revision != revision
                            || this.active_session.as_ref() != Some(&id)
                        {
                            return;
                        }
                        match result {
                            Ok(rows) => {
                                this.set_suggestions(rows, "No matching project files", window, cx)
                            }
                            Err(error) => this.set_suggestions(
                                vec![],
                                &format!("Could not search files: {error}"),
                                window,
                                cx,
                            ),
                        }
                    });
                });
            }
        }
        cx.notify();
    }

    fn set_suggestions(
        &self,
        rows: Vec<Suggestion>,
        message: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.completion.list.update(cx, |list, cx| {
            let first = (!rows.is_empty()).then(|| IndexPath::new(0));
            list.delegate_mut().rows = rows;
            list.delegate_mut().message = message.into();
            list.set_selected_index(first, window, cx);
            cx.notify();
        });
        cx.notify();
    }

    fn dismiss_suggestions(&mut self, cx: &mut Context<Self>) {
        if self.completion.trigger.take().is_some() {
            self.completion.revision += 1;
            cx.notify();
        }
    }

    fn move_suggestion(
        &mut self,
        position: isize,
        absolute: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.completion.list.update(cx, |list, cx| {
            let count = list.delegate().rows.len();
            if count == 0 {
                return;
            }
            let current = list.selected_index().map_or(0, |ix| ix.row) as isize;
            let row = if absolute {
                if position < 0 { count - 1 } else { 0 }
            } else {
                (current + position).rem_euclid(count as isize) as usize
            };
            list.set_selected_index(Some(IndexPath::new(row)), window, cx);
            list.scroll_to_selected_item(window, cx);
        });
    }

    fn accept_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let list = self.completion.list.read(cx);
        let item = list
            .selected_index()
            .and_then(|ix| list.delegate().rows.get(ix.row))
            .cloned();
        if let Some(item) = item {
            self.apply_suggestion(item, window, cx);
        }
    }

    fn apply_suggestion(&mut self, item: Suggestion, window: &mut Window, cx: &mut Context<Self>) {
        let Some(trigger) = self.completion.trigger.clone() else {
            return;
        };
        let mut text = self.composer.read(cx).value().to_string();
        if Trigger::at(&text, self.composer.read(cx).cursor()).as_ref() != Some(&trigger) {
            return;
        }
        let cursor = trigger.range.start + item.insert.len();
        text.replace_range(trigger.range, &item.insert);
        self.dismiss_suggestions(cx);
        self.composer.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.set_selected_range(cursor..cursor, cx);
            input.focus(window, cx);
        });
        if let Some(file) = item.file
            && let Some(session) = self.session_mut()
            && !session.attachments.contains(&file)
        {
            session.attachments.push(file);
        }
        self.capture_draft(cx);
        self.persist_session(cx);
        cx.notify();
    }

    pub(super) fn composer_input(
        &self,
        readonly: bool,
        missing: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.completion.trigger.is_some() && !readonly && !missing;
        let list = self.completion.list.clone();
        let rows = list.read(cx).delegate().rows.len().clamp(1, 7);
        // Base owns anchoring/dismissal; the standard Kit List owns row behavior.
        // A zero-height anchor keeps typing and IME focus in the ordinary Textarea.
        let popup = gpui_kit::base::Popover::new("composer-suggestions")
            .trigger_with(|_, _, _| div().w_full().h_0().into_any_element())
            .anchor(Anchor::BottomLeft)
            .open(open)
            .track_focus(&self.composer.focus_handle(cx))
            .on_open_change({
                let owner = cx.entity().downgrade();
                move |open, _, cx| {
                    if !open {
                        let _ = owner.update(cx, |this, cx| this.dismiss_suggestions(cx));
                    }
                }
            })
            .content(move |_, _, cx| {
                div()
                    .w_96()
                    .max_w_full()
                    .h(rems(rows as f32 * 3.25 + 2.))
                    .popover_style(cx)
                    .p_1()
                    .v_flex()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("↑↓ select · Enter / Tab insert · Esc close"),
                    )
                    .child(div().flex_1().min_h_0().child(SuggestionList::new(&list)))
            });
        div()
            .when(open, |view| view.key_context("ComposerSuggestions"))
            .capture_action(cx.listener(|this, _: &NextSuggestion, window, cx| {
                this.move_suggestion(1, false, window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &PreviousSuggestion, window, cx| {
                this.move_suggestion(-1, false, window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &FirstSuggestion, window, cx| {
                this.move_suggestion(0, true, window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &LastSuggestion, window, cx| {
                this.move_suggestion(-1, true, window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &AcceptSuggestion, window, cx| {
                this.accept_suggestion(window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &DismissSuggestions, _, cx| {
                this.dismiss_suggestions(cx);
                cx.stop_propagation();
            }))
            .when(open, |view| view.child(popup))
            .child(
                Textarea::new(&self.composer)
                    .disabled(missing)
                    .readonly(readonly)
                    .aria_label("Message Pi"),
            )
            .into_any_element()
    }

    /// Returns true for slash syntax, so unknown commands never silently become model prompts.
    pub(super) fn local_command(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(command) = text.trim().strip_prefix('/') else {
            return false;
        };
        let (name, arguments) = command
            .split_once(char::is_whitespace)
            .unwrap_or((command, ""));
        let arguments = arguments.trim();
        if self
            .commands
            .get(self.active_session.as_deref().unwrap_or(""))
            .into_iter()
            .flatten()
            .any(|command| command.name == name)
            && !LOCAL_COMMANDS.iter().any(|(local, _)| *local == name)
        {
            return false;
        }
        if !LOCAL_COMMANDS.iter().any(|(local, _)| *local == name) {
            self.status = Some(format!(
                "Unknown command /{name}. Type / to see available commands."
            ));
            cx.notify();
            return true;
        }
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.capture_draft(cx);
        self.dismiss_suggestions(cx);
        self.status = None;
        match name {
            "new" => self.new_session(window, cx),
            "rename" if !arguments.is_empty() => {
                if let Some(session) = self.session_mut() { session.title = arguments.into(); }
                self.persist_session(cx);
            }
            "rename" => self.rename(window, cx),
            "settings" => cx.emit(WorkspaceEvent::Settings),
            "stop" => self.stop(cx),
            "model" => self.choose_model(window, cx),
            "help" => window.open_dialog(cx, |dialog, _, _| dialog.title("Chat Commands")
                .child("Type / to search desktop and Pi commands. Use ↑↓ and Enter or Tab to insert, then Enter to run. Type @ to attach a project file. Shift+Enter adds a line.")),
            _ => {}
        }
        cx.notify();
        true
    }

    fn choose_model(&self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.active_session.clone().unwrap_or_default();
        let models = self.models.get(&id).cloned().unwrap_or_default();
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let mut content = div().id("command-model-list").max_h_80().overflow_y_scroll().v_flex().gap_1();
            if models.is_empty() { content = content.child("No models available. Check the Pi connection."); }
            for model in &models {
                let model = model.clone(); let owner = owner.clone(); let id = id.clone();
                let label = format!("{} / {}", model["provider"].as_str().unwrap_or(""), model["id"].as_str().unwrap_or(""));
                content = content.child(Button::new(SharedString::from(format!("command-model-{label}"))).ghost().label(label)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            if let Some(client) = this.engines.get_mut(&id) {
                                if let Err(error) = client.request("set_model", json!({"provider":model["provider"], "modelId":model["id"]})) { this.status = Some(error.to_string()); }
                                let _ = client.request("get_state", json!({}));
                            }
                            cx.notify();
                        }); window.close_dialog(cx);
                    }));
            }
            dialog.title("Choose Model").child(content)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{Trigger, TriggerKind, find_files};
    use crate::{
        settings::Preferences,
        storage::Store,
        workspace::{PiClient, PiEvent, Session, Workspace},
    };
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};
    use serde_json::json;

    #[test]
    fn trigger_uses_utf8_cursor_and_ignores_email_and_completed_tokens() {
        assert_eq!(Trigger::at("/ren", 4).unwrap().query, "ren");
        let text = "参考 @\"文档/note with";
        let trigger = Trigger::at(text, text.len()).unwrap();
        assert_eq!(trigger.kind, TriggerKind::File);
        assert_eq!(trigger.query, "文档/note with");
        assert_eq!(&text[trigger.range], "@\"文档/note with");
        assert!(Trigger::at("user@example.org", 16).is_none());
        assert!(Trigger::at("@README.md ", 11).is_none());
        assert!(Trigger::at("/rename title", 13).is_none());
        assert!(Trigger::at("中文", 1).is_none());
    }

    #[test]
    fn file_search_skips_build_trees_and_symlinks_and_quotes_spaces() {
        use std::os::unix::fs::symlink;
        let temporary = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("target")).unwrap();
        std::fs::write(temporary.path().join("target/hidden.md"), "build").unwrap();
        std::fs::write(temporary.path().join("note 中文.md"), "file").unwrap();
        std::fs::write(outside.path().join("secret.md"), "outside").unwrap();
        symlink(outside.path(), temporary.path().join("external")).unwrap();
        let rows = find_files(temporary.path(), "").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].insert, "@\"note 中文.md\" ");
        assert_eq!(
            rows[0].file.as_deref(),
            Some(
                temporary
                    .path()
                    .canonicalize()
                    .unwrap()
                    .join("note 中文.md")
                    .as_path()
            )
        );
    }

    #[gpui_kit::test]
    async fn pi_command_catalog_routes_through_rpc_without_tui_fallback(cx: &mut TestAppContext) {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open(temporary.path().into()).unwrap();
        let session: Session = serde_json::from_value(json!({"id":"a","project":"p","title":"Test","draft":"", "pi_file":temporary.path().join("sessions/a.jsonl"),"messages":[]})).unwrap();
        cx.dispatcher.allow_parking();
        cx.update(gpui_kit::init);
        cx.update(crate::shell::menus);
        let (handle, workspace) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    Workspace::new(
                        store,
                        Preferences::default(),
                        vec![],
                        vec![session],
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        });
        let (mut client, receive) = PiClient::fixture_connection();
        let request = client.request("get_commands", json!({})).unwrap();
        receive.recv().unwrap();
        cx.update_window(handle, |_, window, cx| workspace.update(cx, |this, cx| {
            this.active_session = Some("a".into()); this.engines.insert("a".into(), client); this.ready.insert("a".into());
            this.pi_event(PiEvent::Json("a".into(), json!({"type":"response","id":request,"success":true,"data":{"commands":[{"name":"fixture-task","description":"Fixture extension","source":"extension"}]}})), window, cx);
            this.composer.update(cx, |input, cx| input.set_value("/fixture-task hello", window, cx));
            this.send(window, cx);
        })).unwrap();
        let sent = receive.recv().unwrap();
        assert_eq!(sent["type"], "prompt");
        assert_eq!(sent["message"], "/fixture-task hello");
        cx.update_window(handle, |_, window, cx| workspace.update(cx, |this, cx| {
            this.pi_event(PiEvent::Json("a".into(), json!({"type":"response","id":sent["id"],"success":true,"data":{"disposition":"handled"}})), window, cx);
            assert!(!this.sessions[0].running, "A handled extension command must not leave Stop stuck on");
            this.composer.update(cx, |input, cx| input.set_value("/unknown-tui-command", window, cx)); this.send(window, cx);
            assert!(this.pending_prompt.is_empty()); assert!(this.status.as_ref().unwrap().contains("Unknown command"));
        })).unwrap();
        assert_eq!(receive.recv().unwrap()["type"], "get_state");
        assert!(receive.try_recv().is_err());
    }
}
