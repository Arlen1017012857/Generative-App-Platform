use super::{Appearance, Preferences, dependency_status};
use crate::storage::Store;
use gpui_kit::component::{
    button::*,
    input::{Input, InputState},
    radio::RadioGroup,
    *,
};
use gpui_kit::{prelude::*, *};

#[derive(Clone)]
pub(crate) struct PreferencesChanged(pub(crate) Preferences);

pub(crate) struct Settings {
    store: Store,
    preferences: Preferences,
    pi: Entity<InputState>,
    shell: Entity<InputState>,
    error: Option<String>,
    status: String,
    saving: bool,
}
impl EventEmitter<PreferencesChanged> for Settings {}

pub(crate) fn apply_appearance(preferences: &Preferences, window: &mut Window, cx: &mut App) {
    match preferences.appearance {
        Appearance::System => Theme::sync_system_appearance(Some(window), cx),
        Appearance::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        Appearance::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
    Theme::update(cx, |theme| {
        theme.font_size = px(14. * f32::from(preferences.scale) / 100.);
        // Product palette: neutral Ghostex reference surfaces. Feature views use roles only.
        if theme.mode == ThemeMode::Light {
            theme.colors.foreground = rgb(0x3f3f42).into();
            theme.colors.sidebar = rgb(0xf5f5f7).into();
            theme.colors.sidebar_foreground = theme.colors.foreground;
            theme.colors.muted_foreground = rgb(0x77787c).into();
            theme.colors.border = rgb(0xd8d8da).into();
            theme.colors.sidebar_border = theme.colors.border;
            theme.colors.title_bar = theme.colors.sidebar;
        }
    });
}

impl Settings {
    pub(crate) fn new(
        store: Store,
        preferences: Preferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let pi = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Automatic (pi on PATH)")
                .default_value(preferences.pi_path.clone())
        });
        let shell = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Default login shell")
                .default_value(preferences.shell_path.clone())
        });
        let mut settings = Self {
            store,
            preferences,
            pi,
            shell,
            error: None,
            status: "Checking Pi…".into(),
            saving: false,
        };
        settings.probe(cx);
        settings
    }
    fn probe(&mut self, cx: &mut Context<Self>) {
        let preferences = self.preferences.clone();
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_spawn(async move { dependency_status(&preferences) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.status = status;
                cx.notify();
            });
        })
        .detach();
    }
    pub(crate) fn preferences(&self) -> &Preferences {
        &self.preferences
    }
    #[cfg(test)]
    pub(crate) fn has_validation_error(&self) -> bool {
        self.error.is_some()
    }
    fn save(&mut self, candidate: Preferences, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        self.saving = true;
        self.error = None;
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let validate = candidate.clone();
            let result = cx
                .background_spawn(async move { validate.validate() })
                .await;
            let result = match result {
                Ok(()) => {
                    let record = candidate.clone();
                    cx.background_spawn(async move {
                        store.save("preferences", "desktop", &record).await
                    })
                    .await
                }
                Err(error) => Err(error),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(()) => {
                        this.preferences = candidate.clone();
                        apply_appearance(&candidate, window, cx);
                        cx.emit(PreferencesChanged(candidate));
                        this.probe(cx);
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn save_paths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut candidate = self.preferences.clone();
        candidate.pi_path = self.pi.read(cx).value().trim().into();
        candidate.shell_path = self.shell.read(cx).value().trim().into();
        self.save(candidate, window, cx);
    }
}
impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = match self.preferences.appearance {
            Appearance::System => 0,
            Appearance::Light => 1,
            Appearance::Dark => 2,
        };
        let scale = match self.preferences.scale {
            125 => 1,
            150 => 2,
            _ => 0,
        };
        div()
            .id("settings-panel")
            .w_full()
            .v_flex()
            .gap_4()
            .text_sm()
            .child(
                div()
                    .w_full()
                    .v_flex()
                    .gap_3()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Appearance"))
                    .child(
                        div()
                            .w_full()
                            .h_flex()
                            .gap_4()
                            .child(div().w(rems(72. / 14.)).flex_none().child("Theme"))
                            .child(
                                RadioGroup::horizontal("theme-choice")
                                    .children(["System", "Light", "Dark"])
                                    .selected_index(Some(selected))
                                    .on_change(cx.listener(|this, ix, window, cx| {
                                        let mut next = this.preferences.clone();
                                        next.appearance = match ix {
                                            1 => Appearance::Light,
                                            2 => Appearance::Dark,
                                            _ => Appearance::System,
                                        };
                                        this.save(next, window, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h_flex()
                            .gap_4()
                            .child(div().w(rems(72. / 14.)).flex_none().child("UI Scale"))
                            .child(
                                RadioGroup::horizontal("scale-choice")
                                    .children(["100%", "125%", "150%"])
                                    .selected_index(Some(scale))
                                    .on_change(cx.listener(|this, ix, window, cx| {
                                        let mut next = this.preferences.clone();
                                        next.scale = match ix {
                                            1 => 125,
                                            2 => 150,
                                            _ => 100,
                                        };
                                        this.save(next, window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .child("Pi Executable")
                    .child(
                        Input::new(&self.pi)
                            .id("pi-executable")
                            .aria_label("Pi Executable")
                            .disabled(self.saving),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.status.clone()),
                    ),
            )
            .child(
                div().v_flex().gap_2().child("Shell Executable").child(
                    Input::new(&self.shell)
                        .id("shell-executable")
                        .aria_label("Shell Executable")
                        .disabled(self.saving),
                ),
            )
            .when_some(self.error.clone(), |view, error| {
                view.child(
                    div()
                        .id("settings-error")
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(
                div().w_full().h_flex().justify_end().child(
                    Button::new("save-paths")
                        .primary()
                        .label("Save Paths")
                        .loading(self.saving)
                        .on_click(cx.listener(|this, _, window, cx| this.save_paths(window, cx))),
                ),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .pt_4()
                    .v_flex()
                    .gap_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Generative App Platform {}",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .child("GPUI Kit 0.7.3 · libghostty-vt · Local Pi")
                    .child(
                        div()
                            .w_full()
                            .text_xs()
                            .child(self.store.directory().display().to_string()),
                    )
                    .child(
                        Button::new("open-data-directory")
                            .ghost()
                            .small()
                            .label("Open Data Folder")
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.open_with_system(this.store.directory())
                            })),
                    ),
            )
    }
}
