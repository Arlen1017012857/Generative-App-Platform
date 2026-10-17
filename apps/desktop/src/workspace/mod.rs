#[cfg(test)]
mod chat_tests;
mod composer;
mod project_actions;
mod rpc;
mod session_actions;
#[cfg(test)]
mod tool_tests;
pub(crate) use composer::init;

use crate::{settings::Preferences, storage::Store};
use anyhow::Result;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    button::*,
    dialog::{DialogAction, DialogClose, DialogFooter},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu, PopupMenuItem},
    text::TextView,
    *,
};
use gpui_kit::{prelude::*, *};
use rpc::{PiClient, PiEvent};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Project {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    id: String,
    role: String,
    text: String,
    thinking: String,
    file: Option<PathBuf>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    is_error: bool,
}

fn content_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.into();
    }
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

impl Message {
    fn from_pi(value: &Value) -> Self {
        let role = value["role"].as_str().unwrap_or("assistant").to_owned();
        let id = value["toolCallId"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "{}-{}",
                    role,
                    value
                        .get("timestamp")
                        .cloned()
                        .unwrap_or_else(|| json!(Uuid::new_v4().to_string()))
                )
            });
        let text = content_text(&value["content"]);
        let thinking = value["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| b["thinking"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            id,
            role,
            text,
            thinking,
            file: None,
            tool_name: value["toolName"].as_str().map(str::to_owned),
            is_error: value["isError"].as_bool().unwrap_or(false),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Session {
    pub(crate) id: String,
    pub(crate) project: String,
    pub(crate) title: String,
    draft: String,
    pi_file: PathBuf,
    messages: Vec<Message>,
    #[serde(default)]
    attachments: Vec<PathBuf>,
    #[serde(default)]
    archived: bool,
    #[serde(skip)]
    running: bool,
    #[serde(skip)]
    status: String,
}

impl Session {
    /// Execution events and message events describe the same tool call. Keep one
    /// row per identity, retaining the file link when its final message arrives.
    fn upsert_message(&mut self, mut message: Message) {
        if let Some(previous) = self.messages.iter_mut().find(|old| old.id == message.id) {
            if message.file.is_none() {
                message.file = previous.file.take();
            }
            if message.tool_name.is_none() {
                message.tool_name = previous.tool_name.take();
            }
            *previous = message;
        } else {
            self.messages.push(message);
        }
    }

    fn normalize_messages(&mut self) {
        for message in std::mem::take(&mut self.messages) {
            self.upsert_message(message);
        }
    }
}

#[derive(Clone)]
pub(crate) enum WorkspaceEvent {
    Project(Project),
    ProjectRemoved(String),
    Settings,
    OpenFile(PathBuf),
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Selection {
    project: Option<String>,
    sessions: HashMap<String, String>,
}

/// Owns projects, sessions, composer state, and per-session Pi connections.
pub(crate) struct Workspace {
    store: Store,
    preferences: Preferences,
    pub(crate) projects: Vec<Project>,
    sessions: Vec<Session>,
    active_project: Option<String>,
    active_session: Option<String>,
    composer: Entity<TextareaState>,
    completion: composer::Completion,
    commands: HashMap<String, Vec<composer::PiCommand>>,
    engines: HashMap<String, PiClient>,
    starting: HashSet<String>,
    ready: HashSet<String>,
    pending_prompt: HashMap<String, String>,
    sender: async_channel::Sender<PiEvent>,
    lists: HashMap<String, (ListState, usize)>,
    models: HashMap<String, Vec<Value>>,
    selected_model: HashMap<String, String>,
    thinking: HashMap<String, String>,
    thinking_levels: HashMap<String, Vec<String>>,
    selection: Selection,
    expanded: HashSet<(String, String)>,
    collapsed_projects: HashSet<String>,
    archived_projects: HashSet<String>,
    changing_sessions: HashSet<String>,
    deleting_sessions: HashSet<String>,
    deleting_projects: HashSet<String>,
    filter: Entity<InputState>,
    search_open: bool,
    history: Vec<String>,
    history_ix: usize,
    navigating: bool,
    status: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<WorkspaceEvent> for Workspace {}

impl Workspace {
    pub(crate) async fn load(store: &Store) -> Result<(Vec<Project>, Vec<Session>)> {
        let mut sessions: Vec<Session> = store.load("session").await?;
        for session in &mut sessions {
            session.normalize_messages();
        }
        Ok((store.load("project").await?, sessions))
    }

    pub(crate) fn new(
        store: Store,
        preferences: Preferences,
        projects: Vec<Project>,
        sessions: Vec<Session>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Ask Pi to build something…")
                .auto_grow(2, 8)
                .submit_on_enter(true)
        });
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search projects and sessions"));
        let completion = composer::Completion::new(cx.entity().downgrade(), window, cx);
        let subscriptions = vec![
            cx.subscribe_in(
                &composer,
                window,
                |this, input, event, window, cx| match event {
                    InputEvent::Change => {
                        let draft = input.read(cx).value().to_string();
                        if let Some(session) = this.session_mut() {
                            session.draft = draft;
                        }
                        this.persist_session(cx);
                        this.refresh_suggestions(window, cx);
                        cx.notify();
                    }
                    InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                    _ => {}
                },
            ),
            cx.observe(&filter, |_, _, cx| cx.notify()),
            cx.observe_in(&composer, window, |this, _, window, cx| {
                this.refresh_suggestions(window, cx)
            }),
        ];
        let (sender, receiver) = async_channel::bounded(256);
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = cx
                .background_spawn({
                    let receiver = receiver.clone();
                    async move { receiver.recv().await }
                })
                .await
            {
                if this
                    .update_in(cx, |this, window, cx| this.pi_event(event, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(5)).await;
            if this.update(cx, |this, cx| {
                let mut changed=false;
                for (id, engine) in &mut this.engines {
                    let expired = engine.timed_out();
                    if !expired.is_empty() {
                        if expired.iter().any(|command|command=="prompt"){
                            this.pending_prompt.remove(id);
                            let _=engine.request("get_state",json!({}));
                            let _=engine.request("get_messages",json!({}));
                        }
                        if let Some(session) = this.sessions.iter_mut().find(|session| &session.id == id) {
                            session.status = format!("Pi response timed out: {}. Check engine state before retrying.", expired.join(", "));
                            changed=true;
                        }
                    }
                }
                if changed{cx.notify();}
            }).is_err() { break; }
        }).detach();
        Self {
            store,
            preferences,
            projects,
            sessions,
            active_project: None,
            active_session: None,
            composer,
            completion,
            commands: HashMap::new(),
            engines: HashMap::new(),
            starting: HashSet::new(),
            ready: HashSet::new(),
            pending_prompt: HashMap::new(),
            sender,
            lists: HashMap::new(),
            models: HashMap::new(),
            selected_model: HashMap::new(),
            thinking: HashMap::new(),
            thinking_levels: HashMap::new(),
            selection: Selection::default(),
            expanded: HashSet::new(),
            collapsed_projects: HashSet::new(),
            archived_projects: HashSet::new(),
            changing_sessions: HashSet::new(),
            deleting_sessions: HashSet::new(),
            deleting_projects: HashSet::new(),
            filter,
            search_open: false,
            history: vec![],
            history_ix: 0,
            navigating: false,
            status: None,
            _subscriptions: subscriptions,
        }
    }

    pub(crate) fn project(&self) -> Option<&Project> {
        self.projects
            .iter()
            .find(|project| Some(&project.id) == self.active_project.as_ref())
    }
    fn session(&self) -> Option<&Session> {
        self.sessions
            .iter()
            .find(|session| Some(&session.id) == self.active_session.as_ref())
    }
    fn session_mut(&mut self) -> Option<&mut Session> {
        self.sessions
            .iter_mut()
            .find(|session| Some(&session.id) == self.active_session.as_ref())
    }
    pub(crate) fn has_running(&self) -> bool {
        self.sessions.iter().any(|session| session.running)
            || !self.starting.is_empty()
            || !self.deleting_sessions.is_empty()
    }
    pub(crate) fn set_preferences(&mut self, preferences: Preferences) {
        self.preferences = preferences;
    }
    pub(crate) fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = !self.search_open;
        if self.search_open {
            self.filter.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let selection = cx
                .background_spawn(
                    async move { store.get::<Selection>("selection", "desktop").await },
                )
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.active_project.is_some() {
                    return;
                }
                match selection {
                    Ok(Some(selection)) => this.selection = selection,
                    Err(error) => this.status = Some(error.to_string()),
                    _ => {}
                }
                let project = this
                    .projects
                    .iter()
                    .find(|p| Some(&p.id) == this.selection.project.as_ref())
                    .or_else(|| this.projects.first())
                    .cloned();
                if let Some(project) = project {
                    this.select_project(&project.id, window, cx);
                }
            });
        })
        .detach();
    }

    fn persist_selection(&self, cx: &mut Context<Self>) {
        let write = self.store.save("selection", "desktop", &self.selection);
        cx.spawn(async move |this, cx| {
            if let Err(error) = cx.background_spawn(write).await {
                let _ = this.update(cx, |this, cx| {
                    this.status = Some(error.to_string());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn persist_session(&self, cx: &mut Context<Self>) {
        if let Some(session) = self.session().cloned() {
            self.save_session(session, cx);
        }
    }

    fn save_session(&self, session: Session, cx: &mut Context<Self>) {
        if self.deleting_sessions.contains(&session.id) {
            return;
        }
        let write = self.store.save("session", &session.id, &session);
        cx.spawn(async move |this, cx| {
            if let Err(error) = cx.background_spawn(write).await {
                let _ = this.update(cx, |this, cx| {
                    this.status = Some(format!("Could not save session: {error}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project".into()),
        });
        cx.spawn_in(window, async move |this, cx| match picker.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let result = cx
                        .background_spawn(async move { path.canonicalize() })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(path) => {
                            if let Some(project) = this
                                .projects
                                .iter()
                                .find(|project| project.path == path)
                                .cloned()
                            {
                                this.select_project(&project.id, window, cx);
                                return;
                            }
                            let project = Project {
                                id: Uuid::new_v4().to_string(),
                                name: path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string(),
                                path,
                            };
                            let id = project.id.clone();
                            let write = this.store.save("project", &project.id, &project);
                            this.projects.push(project);
                            cx.spawn(async move |this, cx| {
                                if let Err(error) = cx.background_spawn(write).await {
                                    let _ = this.update(cx, |this, cx| {
                                        this.status = Some(error.to_string());
                                        cx.notify();
                                    });
                                }
                            })
                            .detach();
                            this.select_project(&id, window, cx);
                        }
                        Err(error) => {
                            this.status = Some(error.to_string());
                            cx.notify();
                        }
                    });
                }
            }
            Ok(Err(error)) => {
                let _ = this.update(cx, |this, cx| {
                    this.status = Some(error.to_string());
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }

    pub(crate) fn select_project(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.deleting_projects.contains(id) {
            return;
        }
        self.active_project = Some(id.into());
        if let Some(project) = self.project().cloned() {
            cx.emit(WorkspaceEvent::Project(project));
        }
        self.selection.project = Some(id.into());
        if let Some(session) = self
            .sessions
            .iter()
            .find(|session| {
                session.project == id
                    && !session.archived
                    && !self.deleting_sessions.contains(&session.id)
                    && self.selection.sessions.get(id) == Some(&session.id)
            })
            .or_else(|| {
                self.sessions.iter().find(|session| {
                    session.project == id
                        && !session.archived
                        && !self.deleting_sessions.contains(&session.id)
                })
            })
            .cloned()
        {
            self.select_session(&session.id, window, cx);
        } else {
            self.new_session(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.active_project.clone() else {
            self.open_project(window, cx);
            return;
        };
        if self.deleting_projects.contains(&project) {
            return;
        }
        let id = Uuid::new_v4().to_string();
        self.sessions.push(Session {
            id: id.clone(),
            project,
            title: "New Session".into(),
            draft: String::new(),
            pi_file: self
                .store
                .directory()
                .join("sessions")
                .join(format!("{id}.jsonl")),
            messages: vec![],
            attachments: vec![],
            archived: false,
            running: false,
            status: String::new(),
        });
        self.select_session(&id, window, cx);
        self.persist_session(cx);
    }

    fn select_session(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        if self.deleting_sessions.contains(id) {
            return;
        }
        self.capture_draft(cx);
        if self.active_project.as_ref() != Some(&session.project) {
            self.active_project = Some(session.project.clone());
            self.selection.project = Some(session.project.clone());
            if let Some(project) = self.project().cloned() {
                cx.emit(WorkspaceEvent::Project(project));
            }
        }
        if !self.navigating
            && self
                .history
                .get(self.history_ix)
                .is_none_or(|current| current != id)
        {
            if !self.history.is_empty() {
                self.history.truncate(self.history_ix + 1);
            }
            self.history.push(id.into());
            self.history_ix = self.history.len() - 1;
        }
        self.active_session = Some(id.into());
        if let Some(project) = self.active_project.clone().filter(|_| !session.archived) {
            self.selection.sessions.insert(project, id.into());
            self.persist_selection(cx);
        }
        if let Some(session) = self.session() {
            let draft = session.draft.clone();
            self.composer
                .update(cx, |input, cx| input.set_value(draft, window, cx));
        }
        self.ensure_engine(window, cx);
        cx.notify();
    }

    fn ensure_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session().cloned() else {
            return;
        };
        if session.archived || self.deleting_sessions.contains(&session.id) {
            return;
        }
        if self.engines.contains_key(&session.id) || self.starting.contains(&session.id) {
            return;
        }
        let Some(project) = self.project().cloned() else {
            return;
        };
        let preferences = self.preferences.clone();
        let sender = self.sender.clone();
        let id = session.id.clone();
        self.starting.insert(id.clone());
        if let Some(session) = self.session_mut() {
            session.status = "Connecting to Pi…".into();
        }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    PiClient::spawn(
                        &preferences.pi()?,
                        &project.path,
                        &session.pi_file,
                        session.id,
                        sender,
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| this.complete_engine_start(id, result, cx));
        })
        .detach();
    }

    fn complete_engine_start(
        &mut self,
        id: String,
        result: Result<PiClient>,
        cx: &mut Context<Self>,
    ) {
        self.starting.remove(&id);
        match result {
            Ok(mut client) => {
                // Deletion waits for startup, then reaps the process before removing its history.
                if self.deleting_sessions.contains(&id) {
                    self.engines.insert(id, client);
                    cx.notify();
                    return;
                }
                let result = client
                    .request("get_messages", json!({}))
                    .and_then(|_| client.request("get_state", json!({})))
                    .and_then(|_| client.request("get_available_models", json!({})))
                    .and_then(|_| client.request("get_commands", json!({})))
                    .and_then(|_| client.request("get_available_thinking_levels", json!({})));
                if let Err(error) = result {
                    self.status = Some(error.to_string());
                }
                self.engines.insert(id, client);
            }
            Err(error) => {
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                    session.status = error.to_string();
                    session.running = false;
                    self.pending_prompt.remove(&id);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut text = self.composer.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let Some(session) = self.session().cloned() else {
            return;
        };
        if session.archived || self.deleting_sessions.contains(&session.id) {
            return;
        }
        if self.local_command(&text, window, cx) {
            return;
        }
        if !session.attachments.is_empty() {
            text.push_str(
                "\n\nFiles attached by the user (use your file tools to inspect them as needed):\n",
            );
            for path in &session.attachments {
                text.push_str(&format!("- {}\n", path.display()));
            }
        }
        if session.running || self.pending_prompt.contains_key(&session.id) {
            return;
        }
        self.pending_prompt.insert(session.id.clone(), text.clone());
        if let Some(client) = self
            .engines
            .get_mut(&session.id)
            .filter(|_| self.ready.contains(&session.id))
        {
            if let Err(error) = client.request("prompt", json!({"message": text})) {
                self.status = Some(error.to_string());
                self.pending_prompt.remove(&session.id);
            }
        } else {
            self.ensure_engine(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = &self.active_session {
            self.pending_prompt.remove(id);
            if let Some(client) = self.engines.get_mut(id)
                && let Err(error) = client.stop()
            {
                self.status = Some(error.to_string());
            }
        }
        cx.notify();
    }

    pub(crate) fn shutdown(&mut self) {
        self.engines.clear();
    }

    pub(crate) fn has_pending_session_changes(&self) -> bool {
        !self.changing_sessions.is_empty()
            || !self.deleting_sessions.is_empty()
            || !self.deleting_projects.is_empty()
    }

    pub(crate) fn capture_draft(&mut self, cx: &mut Context<Self>) {
        let draft = self.composer.read(cx).value().to_string();
        if let Some(session) = self.session_mut()
            && session.draft != draft
        {
            session.draft = draft;
            self.persist_session(cx);
        }
    }

    fn navigate(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let next = if forward {
            self.history_ix
                .checked_add(1)
                .filter(|ix| *ix < self.history.len())
        } else {
            self.history_ix.checked_sub(1)
        };
        let Some(ix) = next else {
            return;
        };
        let id = self.history[ix].clone();
        if let Some(session) = self
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
        {
            self.history_ix = ix;
            self.navigating = true;
            self.active_project = Some(session.project);
            if let Some(project) = self.project().cloned() {
                cx.emit(WorkspaceEvent::Project(project));
            }
            self.select_session(&id, window, cx);
            self.navigating = false;
        }
    }

    fn attach_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.active_session.clone() else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach Files".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await {
                let _ = this.update(cx, |this, cx| {
                    if let Some(session) = this.sessions.iter_mut().find(|s| s.id == id) {
                        for path in paths {
                            if !session.attachments.contains(&path) {
                                session.attachments.push(path);
                            }
                        }
                        let record = session.clone();
                        this.save_session(record, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        let _ = window;
    }

    pub(crate) fn notifications(&self, cx: &mut Context<Self>) -> AnyElement {
        let alerts = self
            .sessions
            .iter()
            .filter(|s| {
                !s.status.is_empty()
                    && !s.running
                    && !s.status.starts_with("Connecting")
                    && s.status != "Stopped"
            })
            .map(|s| {
                (
                    s.id.clone(),
                    s.project.clone(),
                    s.title.clone(),
                    s.status.clone(),
                )
            })
            .collect::<Vec<_>>();
        let owner = cx.entity().downgrade();
        Button::new("session-notifications")
            .ghost()
            .xsmall()
            .icon(IconName::Bell)
            .tooltip("Session Notifications")
            .accessibility_label("Session Notifications")
            .dropdown_menu(move |mut menu, _, _| {
                if alerts.is_empty() {
                    return menu
                        .item(PopupMenuItem::new("No sessions need attention").disabled(true));
                }
                for (id, project, title, status) in &alerts {
                    let owner = owner.clone();
                    let id = id.clone();
                    let project = project.clone();
                    menu = menu.item(PopupMenuItem::new(format!("{title} · {status}")).on_click(
                        move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.select_project(&project, window, cx);
                                this.select_session(&id, window, cx);
                            });
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }

    fn model_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let id = self.active_session.clone().unwrap_or_default();
        let models = self.models.get(&id).cloned().unwrap_or_default();
        let label = self
            .selected_model
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "Model".into());
        let owner = cx.entity();
        Button::new("model-picker")
            .ghost()
            .xsmall()
            .label(label)
            .disabled(
                models.is_empty()
                    || self.session().is_some_and(|s| s.archived)
                    || self.deleting_sessions.contains(&id),
            )
            .dropdown_menu(move |mut menu, _, _| {
                for model in &models {
                    let model = model.clone();
                    let owner = owner.clone();
                    let id = id.clone();
                    let label = model["name"]
                        .as_str()
                        .or_else(|| model["id"].as_str())
                        .unwrap_or("Model")
                        .to_owned();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        owner.update(cx, |this, cx| {
                            if let Some(client) = this.engines.get_mut(&id)
                                && let Err(error) = client.request(
                                    "set_model",
                                    json!({"provider":model["provider"],"modelId":model["id"]}),
                                )
                            {
                                this.status = Some(error.to_string());
                            }
                            cx.notify();
                        });
                    }));
                }
                menu
            })
            .into_any_element()
    }

    fn thinking_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let id = self.active_session.clone().unwrap_or_default();
        let levels = self.thinking_levels.get(&id).cloned().unwrap_or_default();
        let label = self
            .thinking
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "Thinking".into());
        let owner = cx.entity();
        Button::new("thinking-picker")
            .ghost()
            .xsmall()
            .label(label)
            .disabled(
                levels.len() < 2
                    || self.session().is_some_and(|s| s.archived)
                    || self.deleting_sessions.contains(&id),
            )
            .dropdown_menu(move |mut menu, _, _| {
                for level in &levels {
                    let level = level.clone();
                    let id = id.clone();
                    let owner = owner.clone();
                    menu =
                        menu.item(PopupMenuItem::new(level.clone()).on_click(move |_, _, cx| {
                            owner.update(cx, |this, cx| {
                                if let Some(client) = this.engines.get_mut(&id)
                                    && let Err(error) =
                                        client.request("set_thinking_level", json!({"level":level}))
                                {
                                    this.status = Some(error.to_string());
                                }
                                cx.notify();
                            })
                        }));
                }
                menu
            })
            .into_any_element()
    }

    fn pi_event(&mut self, event: PiEvent, window: &mut Window, cx: &mut Context<Self>) {
        let id = match &event {
            PiEvent::Json(id, _) | PiEvent::Error(id, _) | PiEvent::Exited(id, _) => id,
        };
        if self.deleting_sessions.contains(id) || !self.sessions.iter().any(|s| &s.id == id) {
            return;
        }
        let (id, value) = match event {
            PiEvent::Json(id, value) => (id, value),
            PiEvent::Error(id, error) => {
                if let Some(client) = self.engines.get(&id) {
                    client.terminate();
                }
                self.ready.remove(&id);
                self.pending_prompt.remove(&id);
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                    session.status = error;
                    session.running = false;
                }
                cx.notify();
                return;
            }
            PiEvent::Exited(id, code) => {
                self.engines.remove(&id);
                self.ready.remove(&id);
                self.pending_prompt.remove(&id);
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                    session.running = false;
                    session.status = format!(
                        "Pi exited ({}). Your draft is preserved.",
                        code.map(|c| c.to_string())
                            .unwrap_or_else(|| "signal".into())
                    );
                }
                cx.notify();
                return;
            }
        };
        let Some(ix) = self.sessions.iter().position(|session| session.id == id) else {
            return;
        };
        match value["type"].as_str().unwrap_or("") {
            "response" => {
                let command = self
                    .engines
                    .get_mut(&id)
                    .and_then(|client| client.response(&value))
                    .unwrap_or_default();
                if value["success"] == false {
                    self.sessions[ix].status = value["error"]
                        .as_str()
                        .unwrap_or("Pi rejected the request")
                        .into();
                    if command == "prompt" {
                        self.pending_prompt.remove(&id);
                    }
                } else {
                    match command.as_str() {
                        "get_messages" => {
                            let files = self.sessions[ix]
                                .messages
                                .iter()
                                .filter_map(|message| {
                                    message
                                        .file
                                        .as_ref()
                                        .map(|file| (message.id.clone(), file.clone()))
                                })
                                .collect::<HashMap<_, _>>();
                            self.sessions[ix].messages = value["data"]["messages"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .map(|value| {
                                    let mut message = Message::from_pi(value);
                                    message.file = files.get(&message.id).cloned();
                                    message
                                })
                                .collect();
                            self.sessions[ix].normalize_messages();
                            if self.ready.insert(id.clone())
                                && let Some(prompt) = self.pending_prompt.get(&id).cloned()
                                && let Some(client) = self.engines.get_mut(&id)
                            {
                                let _ = client.request("prompt", json!({"message": prompt}));
                            }
                        }
                        "get_state" => {
                            self.sessions[ix].status.clear();
                            self.sessions[ix].running =
                                value["data"]["isStreaming"].as_bool().unwrap_or(false);
                            if let Some(model) = value["data"]["model"]["id"].as_str() {
                                self.selected_model.insert(id.clone(), model.into());
                            }
                            if let Some(level) = value["data"]["thinkingLevel"].as_str() {
                                self.thinking.insert(id.clone(), level.into());
                            }
                        }
                        "get_available_models" => {
                            self.models.insert(
                                id.clone(),
                                value["data"]["models"]
                                    .as_array()
                                    .cloned()
                                    .unwrap_or_default(),
                            );
                        }
                        "get_commands" => {
                            match serde_json::from_value(value["data"]["commands"].clone()) {
                                Ok(commands) => {
                                    self.commands.insert(id.clone(), commands);
                                }
                                Err(error) => {
                                    self.sessions[ix].status =
                                        format!("Could not read Pi commands: {error}")
                                }
                            }
                            self.completion.input = None;
                            self.refresh_suggestions(window, cx);
                        }
                        "get_available_thinking_levels" => {
                            self.thinking_levels.insert(
                                id.clone(),
                                value["data"]["levels"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect(),
                            );
                        }
                        "prompt" => {
                            self.pending_prompt.remove(&id);
                            self.sessions[ix].draft.clear();
                            self.sessions[ix].attachments.clear();
                            self.sessions[ix].running = value["data"]["disposition"] != "handled";
                            if let Some(client) = self.engines.get_mut(&id) {
                                let _ = client.request("get_state", json!({}));
                            }
                            if self.active_session.as_ref() == Some(&id) {
                                self.composer
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                            }
                        }
                        "abort" => {
                            self.sessions[ix].running = false;
                            self.sessions[ix].status = "Stopped".into();
                        }
                        "set_model" | "set_thinking_level" => {
                            if let Some(client) = self.engines.get_mut(&id) {
                                let _ = client.request("get_state", json!({}));
                                let _ = client.request("get_available_thinking_levels", json!({}));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "agent_start" => {
                self.sessions[ix].running = true;
                self.sessions[ix].status = "Working…".into();
            }
            "agent_settled" => {
                self.sessions[ix].running = false;
                self.sessions[ix].status.clear();
                if let Some(client) = self.engines.get_mut(&id) {
                    let _ = client.request("get_messages", json!({}));
                }
            }
            "message_start" => {
                self.sessions[ix].upsert_message(Message::from_pi(&value["message"]))
            }
            "message_update" => {
                let delta = &value["assistantMessageEvent"];
                if let Some(message) = self.sessions[ix]
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|message| message.role == "assistant")
                {
                    match delta["type"].as_str() {
                        Some("text_delta") => {
                            message.text.push_str(delta["delta"].as_str().unwrap_or(""))
                        }
                        Some("thinking_delta") => message
                            .thinking
                            .push_str(delta["delta"].as_str().unwrap_or("")),
                        _ => {}
                    }
                }
            }
            "message_end" => {
                self.sessions[ix].upsert_message(Message::from_pi(&value["message"]));
            }
            "tool_execution_start" => {
                let file = value["args"]["path"]
                    .as_str()
                    .or_else(|| value["args"]["file_path"].as_str())
                    .map(PathBuf::from);
                self.sessions[ix].upsert_message(Message {
                    id: value["toolCallId"].as_str().unwrap_or("tool").into(),
                    role: "tool".into(),
                    text: format!(
                        "{}\n{}",
                        value["toolName"].as_str().unwrap_or("Tool"),
                        serde_json::to_string_pretty(&value["args"]).unwrap_or_default()
                    ),
                    thinking: String::new(),
                    file,
                    tool_name: value["toolName"].as_str().map(str::to_owned),
                    is_error: false,
                });
            }
            "tool_execution_update" | "tool_execution_end" => {
                let tool_id = value["toolCallId"].as_str().unwrap_or("");
                if let Some(message) = self.sessions[ix]
                    .messages
                    .iter_mut()
                    .find(|message| message.id == tool_id)
                {
                    let result = if value["type"] == "tool_execution_end" {
                        &value["result"]
                    } else {
                        &value["partialResult"]
                    };
                    message.text = content_text(&result["content"]);
                    message.is_error = value["isError"] == true;
                }
            }
            "extension_ui_request" => self.extension_ui(id.clone(), value.clone(), window, cx),
            "auto_retry_start" | "compaction_start" => {
                self.sessions[ix].status = "Pi is recovering context or retrying…".into();
            }
            "extension_error" => {
                self.sessions[ix].status = value["error"]
                    .as_str()
                    .unwrap_or("Pi extension failed")
                    .into();
            }
            _ => {}
        }
        let session = self.sessions[ix].clone();
        if matches!(
            value["type"].as_str(),
            Some("message_end" | "agent_settled" | "response")
        ) {
            self.save_session(session, cx);
        }
        self.refresh_list(&id);
        cx.notify();
    }

    fn refresh_list(&mut self, id: &str) {
        let count = self
            .sessions
            .iter()
            .find(|s| s.id == id)
            .map_or(0, |s| s.messages.len());
        let (list, old) = self
            .lists
            .entry(id.into())
            .or_insert_with(|| (ListState::new(0, ListAlignment::Bottom, px(500.)), 0));
        if count != *old {
            list.splice(0..*old, count);
            *old = count;
        } else if count > 0 {
            list.remeasure_items(count - 1..count);
        }
    }

    fn extension_ui(
        &mut self,
        session: String,
        value: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let method = value["method"].as_str().unwrap_or("");
        if method == "set_editor_text" {
            let mut updated = None;
            if let Some(record) = self.sessions.iter_mut().find(|record| record.id == session) {
                record.draft = value["text"].as_str().unwrap_or("").into();
                updated = Some(record.clone());
                if self.active_session.as_ref() == Some(&session) {
                    let text = record.draft.clone();
                    self.composer
                        .update(cx, |input, cx| input.set_value(text, window, cx));
                }
            }
            if let Some(record) = updated {
                self.save_session(record, cx);
            }
            return;
        }
        if !matches!(method, "select" | "confirm" | "input" | "editor") {
            if method == "notify" {
                self.status = value["message"].as_str().map(str::to_owned);
            }
            return;
        }
        if let Some(record) = self.sessions.iter_mut().find(|record| record.id == session) {
            record.status = "Waiting for your response…".into();
        }
        let request = value["id"].as_str().unwrap_or("").to_owned();
        let title = value["title"]
            .as_str()
            .unwrap_or("Pi needs your input")
            .to_owned();
        let prefill = value["prefill"]
            .as_str()
            .or_else(|| value["text"].as_str())
            .unwrap_or("")
            .to_owned();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(prefill.clone()));
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .default_value(prefill)
                .auto_grow(5, 14)
        });
        let owner = cx.entity().downgrade();
        let answered = std::rc::Rc::new(std::cell::Cell::new(false));
        type Reply = std::rc::Rc<dyn Fn(Value, &mut App)>;
        let reply: Reply = std::rc::Rc::new(move |answer, cx| {
            if answered.replace(true) {
                return;
            }
            let _ = owner.update(cx, |this, cx| {
                if let Some(client) = this.engines.get(&session)
                    && let Err(error) = client.extension_response(&request, answer)
                {
                    this.status = Some(error.to_string());
                }
                if let Some(record) = this.sessions.iter_mut().find(|record| record.id == session) {
                    record.status = "Working…".into();
                }
                cx.notify();
            });
        });
        let options = value["options"].as_array().cloned().unwrap_or_default();
        let is_select = method == "select";
        let is_confirm = method == "confirm";
        let is_editor = method == "editor";
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone(); let editor = editor.clone();
            let mut content = div().v_flex().gap_2().child(value["message"].as_str().unwrap_or("").to_owned());
            if is_select {
                for option in &options {
                    let label = option.as_str().unwrap_or("").to_owned(); let answer = label.clone(); let reply = reply.clone();
                    content = content.child(Button::new(SharedString::from(format!("option-{label}"))).label(label).on_click(move |_, window, cx| { reply(json!({"value":answer}),cx); window.close_dialog(cx); }));
                }
            } else if is_editor { content = content.child(Textarea::new(&editor)); }
            else if !is_confirm { content = content.child(Input::new(&input)); }
            let cancel = reply.clone(); let close = reply.clone(); let submit = reply.clone();
            dialog.title(title.clone()).child(content)
                .on_cancel(move |_, _, cx| { cancel(json!({"cancelled":true}),cx); true })
                .on_close(move |_, _, cx| close(json!({"cancelled":true}),cx))
                .on_ok(move |_, _, cx| {
                    let answer = if is_confirm { json!({"confirmed":true}) }
                        else { json!({"value":if is_editor {editor.read(cx).value().to_string()} else {input.read(cx).value().to_string()}}) };
                    submit(answer,cx); true
                })
                .footer(DialogFooter::new()
                    .child(DialogClose::new().child(Button::new("cancel-pi-input").label("Cancel")))
                    .when(!is_select, |footer| footer.child(DialogAction::new().child(Button::new("submit-pi-input").primary().label(if is_confirm {"Confirm"} else {"Submit"})))))
        });
    }

    pub(crate) fn rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session().cloned() else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(session.title));
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let owner = owner.clone();
            let id = session.id.clone();
            dialog
                .title("Rename Session")
                .child(Input::new(&input))
                .on_ok(move |_, _, cx| {
                    let title = input.read(cx).value().trim().to_owned();
                    if title.is_empty() {
                        return false;
                    }
                    let _ = owner.update(cx, |this, cx| {
                        if let Some(session) = this.sessions.iter_mut().find(|s| s.id == id) {
                            session.title = title.clone();
                            let record = session.clone();
                            this.save_session(record, cx);
                        }
                        cx.notify();
                    });
                    true
                })
                .footer(
                    DialogFooter::new()
                        .child(
                            DialogClose::new().child(Button::new("cancel-rename").label("Cancel")),
                        )
                        .child(
                            DialogAction::new()
                                .child(Button::new("save-rename").primary().label("Rename")),
                        ),
                )
        });
    }

    pub(crate) fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let title = session
            .as_ref()
            .map_or("Generative App Platform", |s| s.title.as_str())
            .to_owned();
        let header = div()
            .w_full()
            .h_10()
            .flex_none()
            .border_b_1()
            .border_color(cx.theme().border)
            .h_flex()
            .px_3()
            .gap_2()
            .child(
                Button::new("previous-session")
                    .ghost()
                    .xsmall()
                    .icon(IconName::ChevronLeft)
                    .tooltip("Back")
                    .accessibility_label("Back")
                    .disabled(self.history_ix == 0)
                    .on_click(cx.listener(|this, _, window, cx| this.navigate(false, window, cx))),
            )
            .child(
                Button::new("next-session")
                    .ghost()
                    .xsmall()
                    .icon(IconName::ChevronRight)
                    .tooltip("Forward")
                    .accessibility_label("Forward")
                    .disabled(self.history_ix + 1 >= self.history.len())
                    .on_click(cx.listener(|this, _, window, cx| this.navigate(true, window, cx))),
            )
            .child(
                Button::new("new-session")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .tooltip("New Session")
                    .accessibility_label("New Session")
                    .on_click(cx.listener(|this, _, window, cx| this.new_session(window, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                Button::new("rename-session")
                    .ghost()
                    .xsmall()
                    .icon(IconName::SquarePen)
                    .tooltip("Rename Session")
                    .accessibility_label("Rename Session")
                    .disabled(session.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.rename(window, cx))),
            )
            .when_some(session, |view, session| {
                view.when(session.archived, |view| {
                    view.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Archived"),
                    )
                })
                .child(self.session_menu(session, "header", cx))
            });
        header.into_any_element()
    }

    fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let filter = self.filter.read(cx).value().to_lowercase();
        let mut content = div()
            .id("project-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_2()
            .py_3()
            .v_flex()
            .gap_4();
        for project in self.projects.clone() {
            let show_project = project.name.to_lowercase().contains(&filter);
            let sessions = self
                .sessions
                .iter()
                .filter(|session| {
                    session.project == project.id
                        && (show_project || session.title.to_lowercase().contains(&filter))
                })
                .cloned()
                .collect::<Vec<_>>();
            if !show_project && sessions.is_empty() {
                continue;
            }
            let id = project.id.clone();
            let expanded = !self.collapsed_projects.contains(&id);
            let mut group = div()
                .v_flex()
                .gap_1()
                .child(self.project_header(&project, expanded, cx));
            if expanded {
                let has_sessions = sessions.iter().any(|session| !session.archived);
                for session in sessions.iter().filter(|session| !session.archived) {
                    group = group.child(self.session_row(session, cx));
                }
                if !has_sessions {
                    group = group.child(
                        div().pl_8().child(
                            Button::new(SharedString::from(format!("new-session-{}", project.id)))
                                .ghost()
                                .accessibility_label("New Session")
                                .child(div().w_full().child("New Session"))
                                .w_full()
                                .on_click({
                                    let id = project.id.clone();
                                    cx.listener(move |this, _, window, cx| {
                                        this.active_project = Some(id.clone());
                                        if let Some(project) = this.project().cloned() {
                                            cx.emit(WorkspaceEvent::Project(project));
                                        }
                                        this.new_session(window, cx);
                                    })
                                }),
                        ),
                    );
                }
            }
            let archived = sessions
                .iter()
                .filter(|session| session.archived)
                .collect::<Vec<_>>();
            if expanded && !archived.is_empty() {
                let project_id = project.id.clone();
                let shown = self.archived_projects.contains(&project.id) || !filter.is_empty();
                group = group.child(
                    div().pl_8().child(
                        Button::new(SharedString::from(format!("archived-{}", project.id)))
                            .ghost()
                            .small()
                            .w_full()
                            .icon(if shown {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .label(format!("Archived ({})", archived.len()))
                            .accessibility_label(format!("Archived Sessions: {}", project.name))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.archived_projects.remove(&project_id) {
                                    this.archived_projects.insert(project_id.clone());
                                }
                                cx.notify();
                            })),
                    ),
                );
                if shown {
                    for session in archived {
                        group = group.child(self.session_row(session, cx));
                    }
                }
            }
            content = content.child(group);
        }
        if self.projects.is_empty() {
            content = content.child(
                Button::new("open-first-project")
                    .ghost()
                    .icon(IconName::Folder)
                    .label("Open Project…")
                    .on_click(cx.listener(|this, _, window, cx| this.open_project(window, cx))),
            );
        }
        div()
            .size_full()
            .v_flex()
            .when(self.search_open, |view| {
                view.child(
                    div().px_2().py_1().child(
                        Input::new(&self.filter)
                            .small()
                            .appearance(false)
                            .prefix(Icon::new(IconName::Search).small()),
                    ),
                )
            })
            .child(content)
            .into_any_element()
    }

    fn toggle_message(&mut self, session: &str, message: &str, cx: &mut Context<Self>) {
        let key = (session.to_owned(), message.to_owned());
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        if let Some(ix) = self
            .sessions
            .iter()
            .find(|s| s.id == session)
            .and_then(|s| s.messages.iter().position(|m| m.id == message))
            && let Some((list, _)) = self.lists.get(session)
        {
            list.remeasure_items(ix..ix + 1);
        }
        cx.notify();
    }

    fn render_message(
        &self,
        session: &str,
        message: &Message,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tool = matches!(message.role.as_str(), "tool" | "toolResult");
        let expanded = self
            .expanded
            .contains(&(session.to_owned(), message.id.clone()));
        let mut row = div()
            .id(SharedString::from(message.id.clone()))
            .w_full()
            .px_5()
            .py_3()
            .v_flex()
            .gap_2();
        if message.role == "user" {
            row = row.bg(cx.theme().muted.opacity(0.35));
        }
        if tool {
            let id = message.id.clone();
            let session = session.to_owned();
            let name = message
                .tool_name
                .as_deref()
                .unwrap_or_else(|| message.text.lines().next().unwrap_or("Tool result"));
            row = row
                .child(
                    Button::new(SharedString::from(format!("tool-{}", message.id)))
                        .ghost()
                        .small()
                        .icon(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .label(format!(
                            "{name}{}",
                            if message.is_error { " · Error" } else { "" }
                        ))
                        .when(message.is_error, |button| {
                            button.text_color(cx.theme().danger)
                        })
                        .justify_start()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_message(&session, &id, cx);
                        })),
                )
                .when(expanded, |row| {
                    row.child(
                        div()
                            .text_sm()
                            .font_family("Menlo")
                            .child(message.text.clone()),
                    )
                });
        } else {
            if !message.thinking.is_empty() {
                let id = message.id.clone();
                let session = session.to_owned();
                row = row
                    .child(
                        Button::new(SharedString::from(format!("thinking-{id}")))
                            .ghost()
                            .xsmall()
                            .label(if expanded {
                                "Hide thinking"
                            } else {
                                "Thinking"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_message(&session, &id, cx);
                            })),
                    )
                    .when(expanded, |row| {
                        row.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(message.thinking.clone()),
                        )
                    });
            }
            if !message.text.is_empty() {
                row = row.child(TextView::markdown(
                    SharedString::from(format!("message-{}", message.id)),
                    message.text.clone(),
                ));
            }
        }
        if let Some(file) = &message.file {
            let label = file.to_string_lossy().to_string();
            let file = if file.is_absolute() {
                file.clone()
            } else {
                self.sessions
                    .iter()
                    .find(|s| s.id == session)
                    .and_then(|s| self.projects.iter().find(|p| p.id == s.project))
                    .map_or_else(|| file.clone(), |project| project.path.join(file))
            };
            row = row.child(
                Button::new(SharedString::from(format!("file-{}", message.id)))
                    .ghost()
                    .small()
                    .icon(IconName::File)
                    .label(label)
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(WorkspaceEvent::OpenFile(file.clone()));
                    })),
            );
        }
        row.into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session().cloned();
        let id = self.active_session.clone().unwrap_or_default();
        let body = if let Some(session) = &session {
            let state = self
                .lists
                .entry(session.id.clone())
                .or_insert_with(|| {
                    (
                        ListState::new(session.messages.len(), ListAlignment::Bottom, px(500.)),
                        session.messages.len(),
                    )
                })
                .0
                .clone();
            let view = cx.entity();
            let session_id = session.id.clone();
            div()
                .id(SharedString::from(format!("chat-messages-{session_id}")))
                .flex_1()
                .min_h_0()
                .child(
                    list(state, move |ix, _, cx| {
                        view.update(cx, |this, cx| {
                            this.sessions
                                .iter()
                                .find(|s| s.id == session_id)
                                .and_then(|s| s.messages.get(ix))
                                .cloned()
                                .map(|message| this.render_message(&session_id, &message, cx))
                                .unwrap_or_else(|| div().into_any_element())
                        })
                    })
                    .size_full(),
                )
                .into_any_element()
        } else {
            div().flex_1().into_any_element()
        };
        let busy =
            session.as_ref().is_some_and(|s| s.running) || self.pending_prompt.contains_key(&id);
        let readonly =
            session.as_ref().is_some_and(|s| s.archived) || self.deleting_sessions.contains(&id);
        let attachments = session
            .as_ref()
            .map_or_else(Vec::new, |session| session.attachments.clone());
        let mut composer = div().p_3().v_flex().gap_2();
        for path in attachments {
            let remove = path.clone();
            composer = composer.child(
                Button::new(SharedString::from(format!("attachment-{}", path.display())))
                    .ghost()
                    .xsmall()
                    .icon(IconName::X)
                    .label(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    )
                    .tooltip("Remove Attachment")
                    .disabled(readonly)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(session) = this.session_mut() {
                            session.attachments.retain(|path| path != &remove);
                        }
                        this.persist_session(cx);
                        cx.notify();
                    })),
            );
        }
        let mut controls = div()
            .h_flex()
            .gap_1()
            .min_w_0()
            .child(
                Button::new("attach-files")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Paperclip)
                    .accessibility_label("Attach Files")
                    .tooltip("Attach Files")
                    .disabled(session.is_none() || readonly)
                    .on_click(cx.listener(|this, _, window, cx| this.attach_files(window, cx))),
            )
            .child(self.model_picker(cx));
        if self
            .thinking_levels
            .get(&id)
            .is_some_and(|levels| levels.len() > 1)
        {
            controls = controls.child(self.thinking_picker(cx));
        }
        let send = Button::new("send-message")
            .primary()
            .small()
            .label(if busy { "Stop" } else { "Send" })
            .disabled(
                session.is_none()
                    || self.deleting_sessions.contains(&id)
                    || (!busy && (readonly || self.composer.read(cx).value().trim().is_empty())),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                if this.session().is_some_and(|session| {
                    session.running || this.pending_prompt.contains_key(&session.id)
                }) {
                    this.stop(cx)
                } else {
                    this.send(window, cx)
                }
            }));
        composer = composer
            .child(self.composer_input(readonly, session.is_none(), cx))
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .gap_2()
                    .child(controls)
                    .child(send),
            );
        div()
            .size_full()
            .min_w_0()
            .v_flex()
            .bg(cx.theme().background)
            .child(body)
            .when_some(self.status.clone(), |view, status| {
                view.child(
                    div()
                        .px_4()
                        .py_2()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(status),
                )
            })
            .when_some(
                session
                    .as_ref()
                    .filter(|s| !s.status.is_empty())
                    .map(|s| s.status.clone()),
                |view, status| {
                    view.child(
                        div()
                            .px_4()
                            .py_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    )
                },
            )
            .child(composer)
    }
}

pub(crate) struct Sidebar {
    workspace: Entity<Workspace>,
    _subscription: Subscription,
}
impl Sidebar {
    pub(crate) fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&workspace, |_, _, cx| cx.notify());
        Self {
            workspace,
            _subscription: subscription,
        }
    }
}
impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.workspace
            .update(cx, |workspace, cx| workspace.sidebar(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::Session;
    use serde_json::json;
    #[test]
    fn restored_sessions_do_not_replay_pending_prompts() {
        let session:Session=serde_json::from_value(json!({"id":"s","project":"p","title":"Session","draft":"keep me","pi_file":"/tmp/session.jsonl","messages":[]})).unwrap();
        assert!(!session.running);
        assert_eq!(session.draft, "keep me");
    }
}
