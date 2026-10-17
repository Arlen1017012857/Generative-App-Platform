// VT ABI bindings are kept together intentionally, including unused exported constants.
#[allow(dead_code)]
mod ghostty_vt;
mod model;
mod view;
pub(crate) use view::{BackTab, Tab};

use crate::{settings::Preferences, workspace::Project};
use gpui_kit::component::*;
use gpui_kit::{prelude::*, *};
use model::TerminalModel;
use view::TerminalView;

struct Entry {
    id: String,
    project: String,
    title: String,
    view: Option<Entity<TerminalView>>,
    error: Option<String>,
    _subscription: Option<Subscription>,
}

/// Terminal ownership survives panel hiding and project changes.
pub(crate) struct Terminals {
    entries: Vec<Entry>,
    pub(crate) active: Option<String>,
}

impl Terminals {
    pub(crate) fn new() -> Self {
        Self {
            entries: vec![],
            active: None,
        }
    }
    pub(crate) fn add(
        &mut self,
        project: Project,
        preferences: Preferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.entries.push(Entry {
            id: id.clone(),
            project: project.id.clone(),
            title: format!("Terminal · {}", project.name),
            view: None,
            error: None,
            _subscription: None,
        });
        self.active = Some(id.clone());
        let slot = id.clone();
        let (send, receive) = async_channel::bounded(1);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    TerminalModel::spawn(&preferences.shell()?, &project.path, send)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(ix) = this.entries.iter().position(|entry| entry.id == slot) else {
                    return;
                };
                match result {
                    Ok(model) => {
                        let view = cx.new(|cx| TerminalView::new(model, receive, window, cx));
                        let subscription = cx.observe(&view, |_, _, cx| cx.notify());
                        this.entries[ix].view = Some(view);
                        this.entries[ix]._subscription = Some(subscription);
                    }
                    Err(error) => this.entries[ix].error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        id
    }
    pub(crate) fn tabs(&self, project: &str) -> Vec<(String, String)> {
        self.entries
            .iter()
            .filter(|entry| entry.project == project)
            .map(|entry| (entry.id.clone(), entry.title.clone()))
            .collect()
    }
    pub(crate) fn has_running(&self, cx: &App) -> bool {
        self.entries.iter().any(|entry| {
            entry
                .view
                .as_ref()
                .is_some_and(|view| view.read(cx).is_running())
        })
    }
    pub(crate) fn is_running(&self, id: &str, cx: &App) -> bool {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.view.as_ref())
            .is_some_and(|view| view.read(cx).is_running())
    }
    pub(crate) fn close(&mut self, id: &str, cx: &mut Context<Self>) {
        self.entries.retain(|entry| entry.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = None;
        }
        cx.notify();
    }
    pub(crate) fn shutdown(&mut self) {
        self.entries.clear();
    }
    pub(crate) fn close_project(&mut self, project: &str, cx: &mut Context<Self>) {
        let removed_active = self
            .entries
            .iter()
            .any(|entry| entry.project == project && self.active.as_ref() == Some(&entry.id));
        self.entries.retain(|entry| entry.project != project);
        if removed_active {
            self.active = None;
        }
        cx.notify();
    }
    pub(crate) fn appearance_changed(&mut self, cx: &mut Context<Self>) {
        for entry in &self.entries {
            if let Some(view) = &entry.view {
                view.update(cx, |view, cx| view.appearance_changed(cx));
            }
        }
    }
}
impl Render for Terminals {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self
            .entries
            .iter()
            .find(|entry| Some(&entry.id) == self.active.as_ref());
        let mut panel = div()
            .id("terminal-panel")
            .size_full()
            .min_h_0()
            .min_w_0()
            .v_flex();
        if let Some(entry) = entry {
            panel = panel.child(
                div()
                    .h_8()
                    .px_3()
                    .h_flex()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(entry.title.clone()),
            );
            if let Some(view) = &entry.view {
                panel = panel.child(div().flex_1().min_h_0().child(view.clone()));
            } else {
                panel = panel.child(
                    div().flex_1().p_4().child(
                        entry
                            .error
                            .clone()
                            .unwrap_or_else(|| "Starting terminal…".into()),
                    ),
                );
            }
        } else {
            panel = panel.child(
                div()
                    .flex_1()
                    .h_flex()
                    .justify_center()
                    .text_color(cx.theme().muted_foreground)
                    .child("Open a terminal from +"),
            );
        }
        panel
    }
}
