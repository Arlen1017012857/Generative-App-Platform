use crate::{
    files::Files,
    settings::{self, Preferences, PreferencesChanged, Settings},
    storage::Store,
    terminal::Terminals,
    workspace::{Project, Session, Sidebar, Workspace, WorkspaceEvent},
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    button::*,
    menu::{DropdownMenu, PopupMenuItem},
    resizable::{ResizableState, h_resizable, resizable_panel},
    *,
};
use gpui_kit::{prelude::*, *};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

gpui_kit::actions!(
    desktop,
    [
        Quit,
        OpenProject,
        NewSession,
        OpenSettings,
        ToggleSidebar,
        ToggleWorkspace,
        NewTerminal,
        ShowFiles,
        Search
    ]
);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Layout {
    sidebar: bool,
    right: bool,
    files: bool,
    maximized: bool,
    widths: Vec<f32>,
    terminals: Vec<String>,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            sidebar: true,
            right: true,
            files: true,
            maximized: false,
            widths: vec![424., 528., 456.],
            terminals: vec![],
        }
    }
}

pub(crate) struct Desktop {
    store: Store,
    workspace: Entity<Workspace>,
    sidebar: Entity<Sidebar>,
    files: Entity<Files>,
    terminals: Entity<Terminals>,
    settings: Entity<Settings>,
    layout: Layout,
    split: Entity<ResizableState>,
    active_terminal: Option<String>,
    tabs_scroll: ScrollHandle,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
    files_by_project: HashMap<String, Entity<Files>>,
}

impl Desktop {
    pub(crate) fn new(
        store: Store,
        preferences: Preferences,
        projects: Vec<Project>,
        sessions: Vec<Session>,
        mut layout: Layout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if layout.widths.len() != 3
            || layout
                .widths
                .iter()
                .any(|width| !width.is_finite() || *width < 0.)
        {
            layout.widths = Layout::default().widths;
        }
        settings::apply_appearance(&preferences, window, cx);
        let workspace = cx.new(|cx| {
            Workspace::new(
                store.clone(),
                preferences.clone(),
                projects,
                sessions,
                window,
                cx,
            )
        });
        let sidebar = cx.new(|cx| Sidebar::new(workspace.clone(), cx));
        let files = cx.new(|cx| Files::new(store.clone(), window, cx));
        let terminals = cx.new(|_| Terminals::new());
        let settings = cx.new(|cx| Settings::new(store.clone(), preferences, window, cx));
        let split = cx.new(|_| ResizableState::default());
        let subscriptions = vec![
            cx.subscribe_in(
                &workspace,
                window,
                |this, _, event, window, cx| match event {
                    WorkspaceEvent::Project(project) => {
                        this.tabs_scroll.scroll_to_item(0);
                        if let Some(files) = this.files_by_project.get(&project.id) {
                            this.files = files.clone();
                        } else {
                            let files = cx.new(|cx| Files::new(this.store.clone(), window, cx));
                            files.update(cx, |files, cx| {
                                files.set_project(project.path.clone(), window, cx)
                            });
                            this.files_by_project
                                .insert(project.id.clone(), files.clone());
                            this.files = files;
                        }
                        let tabs = this.terminals.read(cx).tabs(&project.id);
                        if this
                            .active_terminal
                            .as_ref()
                            .is_some_and(|id| !tabs.iter().any(|(tab, _)| tab == id))
                        {
                            this.active_terminal = None;
                        }
                        if this.active_terminal.is_none() && !this.layout.files {
                            this.layout.right = false;
                            this.layout.maximized = false;
                        }
                        cx.notify();
                    }
                    WorkspaceEvent::OpenFile(path) => {
                        this.layout.right = true;
                        this.layout.files = true;
                        this.active_terminal = None;
                        this.files
                            .update(cx, |files, cx| files.open(path.clone(), window, cx));
                        cx.notify();
                    }
                    WorkspaceEvent::Settings => this.open_settings(window, cx),
                    WorkspaceEvent::ProjectRemoved(id) => {
                        let removed_active = this
                            .terminals
                            .read(cx)
                            .tabs(id)
                            .iter()
                            .any(|(id, _)| this.active_terminal.as_ref() == Some(id));
                        this.terminals
                            .update(cx, |terminals, cx| terminals.close_project(id, cx));
                        if removed_active {
                            this.active_terminal = None;
                        }
                        this.files_by_project.remove(id);
                        this.layout.terminals.retain(|project| project != id);
                        if this.workspace.read(cx).project().is_none() {
                            this.files = cx.new(|cx| Files::new(this.store.clone(), window, cx));
                        }
                        this.persist(cx);
                        cx.notify();
                    }
                },
            ),
            cx.subscribe(
                &settings,
                |this, settings, event: &PreferencesChanged, cx| {
                    this.workspace.update(cx, |workspace, _| {
                        workspace.set_preferences(event.0.clone())
                    });
                    this.terminals
                        .update(cx, |terminals, cx| terminals.appearance_changed(cx));
                    let _ = settings;
                    this.reveal_active_tab(cx);
                    cx.notify();
                },
            ),
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.observe(&terminals, |_, _, cx| cx.notify()),
        ];
        let owner = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = owner.update(cx, |this, cx| this.request_quit(window, cx));
            false
        });
        let weak = cx.entity().downgrade();
        let appearance = window.observe_window_appearance(move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                let preferences = this.settings.read(cx).preferences().clone();
                if preferences.appearance == settings::Appearance::System {
                    settings::apply_appearance(&preferences, window, cx);
                }
            });
        });
        let mut desktop = Self {
            store,
            workspace,
            sidebar,
            files,
            terminals,
            settings,
            layout,
            split,
            active_terminal: None,
            tabs_scroll: ScrollHandle::new(),
            focus: cx.focus_handle().tab_stop(true),
            _subscriptions: subscriptions,
            files_by_project: HashMap::new(),
        };
        desktop._subscriptions.push(appearance);
        desktop.focus.focus(window, cx);
        desktop
            .workspace
            .update(cx, |workspace, cx| workspace.restore(window, cx));
        for project in desktop.layout.terminals.clone() {
            if let Some(project) = desktop
                .workspace
                .read(cx)
                .projects
                .iter()
                .find(|p| p.id == project)
                .cloned()
            {
                let preferences = desktop.settings.read(cx).preferences().clone();
                desktop.terminals.update(cx, |terminals, cx| {
                    terminals.add(project, preferences, window, cx);
                });
            }
        }
        desktop
    }

    fn persist(&self, cx: &mut Context<Self>) {
        let write = self.store.save("layout", "desktop", &self.layout);
        cx.spawn(async move |this, cx| {
            if let Err(error) = cx.background_spawn(write).await {
                let _ = this.update(cx, |_, cx| {
                    eprintln!("Could not save desktop layout: {error}");
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Settings")
                .width(px(540.))
                .child(settings.clone())
        });
    }
    fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(project) = self.workspace.read(cx).project().cloned() {
            let preferences = self.settings.read(cx).preferences().clone();
            let project_id = project.id.clone();
            let id = self.terminals.update(cx, |terminals, cx| {
                terminals.add(project, preferences, window, cx)
            });
            self.active_terminal = Some(id);
            self.reveal_active_tab(cx);
            self.layout.right = true;
            self.layout.terminals.push(project_id);
            self.persist(cx);
            cx.notify();
        }
    }
    fn close_terminal(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminals.read(cx).is_running(&id, cx) {
            let owner = cx.entity().downgrade();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let owner = owner.clone();
                let id = id.clone();
                dialog
                    .title("Terminate this terminal?")
                    .description("Its running shell and foreground process will be stopped.")
                    .confirm()
                    .ok_text("Terminate")
                    .on_ok(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| this.remove_terminal(&id, cx));
                        true
                    })
            });
        } else {
            self.remove_terminal(&id, cx);
        }
    }
    fn remove_terminal(&mut self, id: &str, cx: &mut Context<Self>) {
        let project = self.workspace.read(cx).project().map(|p| p.id.clone());
        self.terminals
            .update(cx, |terminals, cx| terminals.close(id, cx));
        if self.active_terminal.as_deref() == Some(id) {
            self.active_terminal = None;
        }
        if let Some(project) = project
            && let Some(ix) = self.layout.terminals.iter().position(|p| p == &project)
        {
            self.layout.terminals.remove(ix);
        }
        self.persist(cx);
        cx.notify();
    }
    fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace.read(cx).has_running() || self.terminals.read(cx).has_running(cx) {
            let owner = cx.entity().downgrade();
            window.open_alert_dialog(cx,move|dialog,_,_|{let owner=owner.clone();dialog.title("Quit Generative App Platform?").description("Running Pi sessions and local terminals will stop. Saved history and drafts remain available.").confirm().ok_text("Quit").on_ok(move|_,_,cx|{let _=owner.update(cx,|this,cx|this.shutdown(cx));true})});
        } else {
            self.shutdown(cx);
        }
    }
    fn shutdown(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.capture_draft(cx);
        });
        self.terminals
            .update(cx, |terminals, _| terminals.shutdown());
        let store = self.store.clone();
        let workspace = self.workspace.clone();
        cx.spawn(async move |_, cx| {
            // Archive rollback and deletion own pending writes/process cleanup.
            // Let those finish before flushing SQLite and tearing down the app.
            while workspace.update(cx, |workspace, _| workspace.has_pending_session_changes()) {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(25))
                    .await;
            }
            workspace.update(cx, |workspace, _| workspace.shutdown());
            let _ = cx
                .background_spawn(async move { store.flush().await })
                .await;
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.layout.sidebar = !self.layout.sidebar;
        self.persist(cx);
        cx.notify();
    }
    fn toggle_workspace(&mut self, cx: &mut Context<Self>) {
        self.layout.right = !self.layout.right;
        if !self.layout.right {
            self.layout.maximized = false;
        } else if self.active_terminal.is_none() {
            self.layout.files = true;
        }
        self.persist(cx);
        cx.notify();
    }

    fn sidebar_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let navigation = div()
            .h_flex()
            .gap_1()
            // Reserve the lane for the window-owned sidebar toggle.
            .child(div().w_5().flex_none())
            .child(
                Button::new("new-session-toolbar")
                    .ghost()
                    .xsmall()
                    .icon(IconName::MessageSquare)
                    .tooltip("New Session")
                    .accessibility_label("New Session")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.new_session(window, cx))
                    })),
            );
        let menu = Button::new("sidebar-menu")
            .ghost()
            .xsmall()
            .icon(IconName::Menu)
            .tooltip("Project Commands")
            .accessibility_label("Project Commands")
            .dropdown_menu({
                let owner = cx.entity().downgrade();
                move |menu, _, _| {
                    let new = owner.clone();
                    let open = owner.clone();
                    menu.item(
                        PopupMenuItem::new("Open Project…").on_click(move |_, window, cx| {
                            let _ = open.update(cx, |this, cx| {
                                this.workspace
                                    .update(cx, |workspace, cx| workspace.open_project(window, cx))
                            });
                        }),
                    )
                    .item(
                        PopupMenuItem::new("New Session").on_click(move |_, window, cx| {
                            let _ = new.update(cx, |this, cx| {
                                this.workspace
                                    .update(cx, |workspace, cx| workspace.new_session(window, cx))
                            });
                        }),
                    )
                }
            });
        let notifications = self
            .workspace
            .update(cx, |workspace, cx| workspace.notifications(cx));
        let tools = div()
            .h_flex()
            .gap_1()
            .child(
                Button::new("search-projects")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Search)
                    .tooltip("Search Projects and Sessions")
                    .accessibility_label("Search Projects and Sessions")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.focus_search(window, cx))
                    })),
            )
            .child(notifications)
            .child(menu);
        TitleBar::new()
            .h_10()
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .h_flex()
                    .size_full()
                    .justify_between()
                    .child(navigation)
                    .child(tools),
            )
            .into_any_element()
    }
    fn right_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let project = self
            .workspace
            .read(cx)
            .project()
            .map(|p| p.id.clone())
            .unwrap_or_default();
        let tabs = self.terminals.read(cx).tabs(&project);
        let scale = f32::from(self.settings.read(cx).preferences().scale) / 100.;
        let panel_width = self
            .split
            .read(cx)
            .sizes()
            .get(2)
            .copied()
            .map(f32::from)
            .unwrap_or(self.layout.widths[2]);
        let count = (tabs.len() + usize::from(self.layout.files)).clamp(1, 2) as f32;
        let tab_width = ((panel_width - 92. * scale) / count).clamp(96. * scale, 160. * scale);
        let mut tabbar = div()
            .id("workspace-tabs")
            .flex_1()
            .min_w_0()
            .overflow_x_scroll()
            .track_scroll(&self.tabs_scroll)
            .h_flex()
            .gap_1()
            .when(self.layout.files, |bar| {
                bar.child(
                    div()
                        .w(px(tab_width))
                        .flex_none()
                        .h_flex()
                        .gap_0()
                        .child(
                            Button::new("files-tab")
                                .ghost()
                                .small()
                                .icon(IconName::File)
                                .label("Files")
                                .w(px(tab_width - 24. * scale))
                                .flex_none()
                                .min_w_0()
                                .selected(self.active_terminal.is_none())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.active_terminal = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("close-files")
                                .ghost()
                                .xsmall()
                                .icon(IconName::X)
                                .tooltip("Close Files")
                                .accessibility_label("Close Files")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.layout.files = false;
                                    let project = this
                                        .workspace
                                        .read(cx)
                                        .project()
                                        .map(|project| project.id.clone())
                                        .unwrap_or_default();
                                    if let Some((id, _)) =
                                        this.terminals.read(cx).tabs(&project).first()
                                    {
                                        this.active_terminal = Some(id.clone());
                                        this.terminals.update(cx, |terminals, _| {
                                            terminals.active = Some(id.clone())
                                        });
                                    } else {
                                        this.layout.right = false;
                                        this.layout.maximized = false;
                                    }
                                    this.persist(cx);
                                    cx.notify();
                                })),
                        ),
                )
            });
        for (id, title) in tabs {
            let select = id.clone();
            let close = id.clone();
            tabbar = tabbar.child(
                div()
                    .w(px(tab_width))
                    .flex_none()
                    .min_w_0()
                    .h_flex()
                    .gap_0()
                    .child(
                        Button::new(SharedString::from(format!("terminal-tab-{id}")))
                            .ghost()
                            .small()
                            .icon(IconName::Terminal)
                            .label(title)
                            .w(px(tab_width - 24. * scale))
                            .flex_none()
                            .min_w_0()
                            .selected(self.active_terminal.as_ref() == Some(&id))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active_terminal = Some(select.clone());
                                this.terminals.update(cx, |terminals, _| {
                                    terminals.active = Some(select.clone())
                                });
                                this.reveal_active_tab(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("close-terminal-{id}")))
                            .ghost()
                            .xsmall()
                            .icon(IconName::X)
                            .tooltip("Close Terminal")
                            .accessibility_label("Close Terminal")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_terminal(close.clone(), window, cx)
                            })),
                    ),
            );
        }
        div()
            .h_10()
            .flex_none()
            .h_flex()
            .px_2()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(tabbar)
            .child(
                Button::new("add-workspace-view")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .tooltip("Open View")
                    .accessibility_label("Open View")
                    .dropdown_menu(|menu, _, _| {
                        menu.menu("Files", Box::new(ShowFiles))
                            .menu("Terminal", Box::new(NewTerminal))
                    }),
            )
            .child(
                Button::new("maximize-workspace")
                    .ghost()
                    .xsmall()
                    .icon(if self.layout.maximized {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .tooltip("Maximize / Restore Workspace")
                    .accessibility_label("Maximize / Restore Workspace")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.layout.maximized = !this.layout.maximized;
                        this.persist(cx);
                        cx.notify();
                    })),
            )
            .child(div().w_5().flex_none())
            .into_any_element()
    }

    fn reveal_active_tab(&self, cx: &App) {
        let Some(project) = self.workspace.read(cx).project() else {
            return;
        };
        let index = self
            .active_terminal
            .as_ref()
            .and_then(|active| {
                self.terminals
                    .read(cx)
                    .tabs(&project.id)
                    .iter()
                    .position(|(id, _)| id == active)
                    .map(|index| index + usize::from(self.layout.files))
            })
            .unwrap_or(0);
        self.tabs_scroll.scroll_to_item(index);
    }
}

impl Render for Desktop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = f32::from(window.viewport_size().width);
        let scale = f32::from(self.settings.read(cx).preferences().scale) / 100.;
        let right = self.layout.right && width >= 1050. * scale;
        let sidebar = self.layout.sidebar && width >= 760. * scale;
        let left = div()
            .size_full()
            .min_h_0()
            .v_flex()
            .bg(cx.theme().sidebar)
            .child(self.sidebar_header(cx))
            .child(div().flex_1().min_h_0().child(self.sidebar.clone()))
            .child(
                div().h_9().px_2().h_flex().justify_end().child(
                    Button::new("open-settings")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Settings)
                        .tooltip("Settings · ⌘,")
                        .accessibility_label("Settings · ⌘,")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_settings(window, cx)),
                        ),
                ),
            );
        let workspace_header = self
            .workspace
            .update(cx, |workspace, cx| workspace.render_header(cx));
        let heading = div()
            .h_flex()
            .w_full()
            .h_10()
            .items_stretch()
            .when(!sidebar, |view| view.child(div().w_6().flex_none()))
            .child(div().flex_1().min_w_0().child(workspace_header))
            .when(!right, |view| view.pr_8());
        let header = if sidebar {
            heading.into_any_element()
        } else {
            TitleBar::new().h_10().child(heading).into_any_element()
        };
        let main = div()
            .size_full()
            .v_flex()
            .child(header)
            .child(div().flex_1().min_h_0().child(self.workspace.clone()));
        let content = if self.active_terminal.is_some() {
            self.terminals.clone().into_any_element()
        } else {
            self.files.clone().into_any_element()
        };
        let right_view = div()
            .size_full()
            .v_flex()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().sidebar)
            .child(self.right_header(cx))
            .child(div().flex_1().min_h_0().child(content));
        div()
            .id("desktop")
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .track_focus(&self.focus)
            .key_context("Desktop")
            .on_action(cx.listener(|this, _: &Quit, window, cx| this.request_quit(window, cx)))
            .on_action(cx.listener(|this, _: &OpenProject, window, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.open_project(window, cx))
            }))
            .on_action(cx.listener(|this, _: &NewSession, window, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.new_session(window, cx))
            }))
            .on_action(
                cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &ToggleWorkspace, _, cx| this.toggle_workspace(cx)))
            .on_action(
                cx.listener(|this, _: &NewTerminal, window, cx| this.new_terminal(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowFiles, _, cx| {
                this.layout.right = true;
                this.layout.files = true;
                this.active_terminal = None;
                this.tabs_scroll.scroll_to_item(0);
                this.persist(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Search, window, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.focus_search(window, cx))
            }))
            .child(
                h_resizable("desktop-split")
                    .with_state(&self.split)
                    .child(
                        resizable_panel()
                            .visible(sidebar)
                            .size(px(self.layout.widths[0]))
                            .size_range(px(200. * scale)..px(650. * scale))
                            .child(left),
                    )
                    .child(
                        resizable_panel()
                            .visible(!self.layout.maximized || !right)
                            .size(px(self.layout.widths[1]))
                            .size_range(px(280. * scale)..px(3000.))
                            .child(main),
                    )
                    .child(
                        resizable_panel()
                            .visible(right)
                            .size(px(self.layout.widths[2]))
                            .size_range(px(360. * scale)..px(3000.))
                            .child(right_view),
                    )
                    .on_resize({
                        let owner = cx.entity().downgrade();
                        move |state, _, cx| {
                            let widths = state
                                .read(cx)
                                .sizes()
                                .iter()
                                .map(|value| f32::from(*value))
                                .collect::<Vec<_>>();
                            let _ = owner.update(cx, |this, cx| {
                                if widths.len() == 3 {
                                    this.layout.widths = widths;
                                    this.persist(cx);
                                }
                            });
                        }
                    }),
            )
            // Window chrome lives outside the optional panes. The leading physical
            // inset matches Kit's macOS traffic-light lane; control geometry scales.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(80.))
                    .h_10()
                    .h_flex()
                    .child(
                        Button::new("toggle-sidebar")
                            .ghost()
                            .xsmall()
                            .icon(IconName::PanelLeft)
                            .tooltip(if sidebar {
                                "Hide Sidebar · ⌘B"
                            } else {
                                "Show Sidebar · ⌘B"
                            })
                            .accessibility_label("Toggle Sidebar")
                            .selected(sidebar)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
                    ),
            )
            .child(
                div().absolute().top_0().right_2().h_10().h_flex().child(
                    Button::new("toggle-workspace")
                        .ghost()
                        .xsmall()
                        .icon(IconName::PanelRight)
                        .tooltip(if right {
                            "Hide Workspace · ⌘J"
                        } else {
                            "Show Workspace · ⌘J"
                        })
                        .accessibility_label("Toggle Workspace")
                        .selected(right)
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_workspace(cx))),
                ),
            )
    }
}

pub(crate) fn menus(cx: &mut App) {
    crate::workspace::init(cx);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, Some("Desktop")),
        KeyBinding::new("cmd-o", OpenProject, Some("Desktop")),
        KeyBinding::new("cmd-n", NewSession, Some("Desktop")),
        KeyBinding::new("cmd-,", OpenSettings, Some("Desktop")),
        KeyBinding::new("cmd-b", ToggleSidebar, Some("Desktop")),
        KeyBinding::new("cmd-j", ToggleWorkspace, Some("Desktop")),
        KeyBinding::new("cmd-shift-t", NewTerminal, Some("Desktop")),
        KeyBinding::new("cmd-shift-f", Search, Some("Desktop")),
        KeyBinding::new("tab", crate::terminal::Tab, Some("Terminal")),
        KeyBinding::new("shift-tab", crate::terminal::BackTab, Some("Terminal")),
    ]);
    cx.set_menus(vec![
        Menu {
            disabled: false,
            name: "Generative App Platform".into(),
            items: vec![
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            disabled: false,
            name: "File".into(),
            items: vec![
                MenuItem::action("Open Project…", OpenProject),
                MenuItem::action("New Session", NewSession),
                MenuItem::action("New Terminal", NewTerminal),
            ],
        },
        Menu {
            disabled: false,
            name: "View".into(),
            items: vec![
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::action("Toggle Workspace", ToggleWorkspace),
                MenuItem::action("Files", ShowFiles),
                MenuItem::action("Search", Search),
            ],
        },
    ]);
}

#[cfg(test)]
mod tests {
    use super::Layout;
    use crate::storage::Store;
    #[test]
    fn layout_restores_widths_visibility_and_terminal_project_identity() {
        futures::executor::block_on(async {
            let temporary = tempfile::tempdir().unwrap();
            let store = Store::open(temporary.path().into()).unwrap();
            let saved = Layout {
                sidebar: false,
                files: false,
                maximized: true,
                widths: vec![300., 600., 508.],
                terminals: vec!["project-a".into(), "project-b".into()],
                ..Default::default()
            };
            store.save("layout", "desktop", &saved).await.unwrap();
            let restored = store
                .get::<Layout>("layout", "desktop")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(restored.widths, saved.widths);
            assert_eq!(restored.terminals, saved.terminals);
            assert!(!restored.sidebar && !restored.files && restored.maximized && restored.right);
            let migrated: Layout = serde_json::from_str("{\"sidebar\":false}").unwrap();
            assert!(migrated.files && migrated.right);
        });
    }
}
