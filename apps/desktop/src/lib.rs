mod files;
mod settings;
mod shell;
mod storage;
mod terminal;
#[cfg(test)]
mod ui_tests;
mod workspace;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

pub fn run() {
    let startup = futures::executor::block_on(async {
        let store = storage::Store::open(storage::data_directory())?;
        let preferences = store
            .load::<settings::Preferences>("preferences")
            .await?
            .into_iter()
            .next()
            .unwrap_or_default();
        let layout = store
            .load::<shell::Layout>("layout")
            .await?
            .into_iter()
            .next()
            .unwrap_or_default();
        let (projects, sessions) = workspace::Workspace::load(&store).await?;
        Ok::<_, anyhow::Error>((store, preferences, layout, projects, sessions))
    });
    let (store, preferences, layout, projects, sessions) = match startup {
        Ok(state) => state,
        Err(error) => {
            eprintln!("Could not open Generative App Platform data: {error:#}");
            return;
        }
    };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            shell::menus(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1408.), px(770.)),
                    cx,
                ))),
                window_min_size: Some(size(px(760.), px(520.))),
                app_id: Some("dev.generative-app-platform.desktop".into()),
                ..TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, move |window, cx| {
                cx.new(|cx| {
                    shell::Desktop::new(store, preferences, projects, sessions, layout, window, cx)
                })
            })
            .expect("open desktop window");
            cx.activate(true);
        });
}
