use crate::storage::Store;
use anyhow::{Context as _, Result, bail};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants},
    input::{Editor, EditorState, Input, InputEvent, InputState},
    list::ListItem,
    resizable::{ResizableState, h_resizable, resizable_panel},
    text::TextView,
    tree::{Tree, TreeEvent, TreeItem, TreeState},
};
use gpui_kit::{prelude::*, *};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Navigation {
    selected: Option<PathBuf>,
    expanded: BTreeSet<PathBuf>,
    widths: Vec<f32>,
}

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    directory: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Preview {
    Text(String, String),
    Image(PathBuf),
    External(String),
}

pub(crate) fn read_preview(root: &Path, path: &Path) -> Result<Preview> {
    let path = path.canonicalize().context("File is no longer available")?;
    if !path.starts_with(root.canonicalize()?) {
        bail!("This file is outside the opened project. Open its project first.");
    }
    let metadata = path.metadata()?;
    if !metadata.is_file() {
        bail!("Select a file to preview.");
    }
    if metadata.len() > 8 * 1024 * 1024 {
        return Ok(Preview::External(
            "File exceeds the 8 MiB preview limit.".into(),
        ));
    }
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
    ) {
        let (width, height) = image::image_dimensions(&path)?;
        if u64::from(width) * u64::from(height) > 40_000_000 {
            return Ok(Preview::External("Image is too large to preview.".into()));
        }
        return Ok(Preview::Image(path));
    }
    let bytes = std::fs::read(&path)?;
    if bytes.contains(&0) {
        return Ok(Preview::External(
            "This format opens in its own app.".into(),
        ));
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(Preview::Text(text, extension)),
        Err(_) => Ok(Preview::External(
            "This format opens in its own app.".into(),
        )),
    }
}

fn entries(root: &Path, directory: &Path) -> Result<Vec<Entry>> {
    let directory = directory.canonicalize()?;
    if !directory.starts_with(root) {
        bail!("Folder points outside the opened project.");
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let directory = entry.file_type()?.is_dir();
        result.push(Entry { path, directory });
    }
    result.sort_by_key(|entry| {
        (
            !entry.directory,
            entry.path.file_name().map(|s| s.to_ascii_lowercase()),
        )
    });
    Ok(result)
}

/// Owns navigation, watcher lifetime, and one retained readonly preview.
pub(crate) struct Files {
    store: Store,
    split: Entity<ResizableState>,
    widths: Vec<f32>,
    root: Option<PathBuf>,
    selected: Option<PathBuf>,
    expanded: BTreeSet<PathBuf>,
    folders: BTreeMap<PathBuf, Vec<Entry>>,
    tree: Entity<TreeState>,
    path: Entity<InputState>,
    editor: Option<Entity<EditorState>>,
    preview: Option<Preview>,
    status: Option<String>,
    revision: u64,
    search_revision: u64,
    watcher: Option<RecommendedWatcher>,
    _subscriptions: Vec<Subscription>,
}

impl Files {
    pub(crate) fn new(store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tree = cx.new(|cx| TreeState::new(cx));
        let path = cx.new(|cx| InputState::new(window, cx).placeholder("Search or paste a path"));
        let subscriptions = vec![
            cx.observe_in(&tree, window, |this, tree, window, cx| {
                let selected = tree
                    .read(cx)
                    .selected_item()
                    .map(|item| PathBuf::from(item.id.as_str()));
                if let Some(path) = selected {
                    let folder = this
                        .folders
                        .values()
                        .flatten()
                        .any(|entry| entry.path == path && entry.directory);
                    if !folder
                        && this.selected.as_ref() != Some(&path)
                        && !path.to_string_lossy().ends_with("::pending")
                    {
                        this.open(path, window, cx);
                    }
                }
            }),
            cx.subscribe_in(&tree, window, |this, _, event, window, cx| match event {
                TreeEvent::Expanded(id) => {
                    let path = PathBuf::from(id.as_str());
                    this.expanded.insert(path.clone());
                    this.load_folder(path, window, cx);
                    this.persist(cx);
                }
                TreeEvent::Collapsed(id) => {
                    this.expanded.remove(&PathBuf::from(id.as_str()));
                    this.persist(cx);
                }
            }),
            cx.subscribe_in(&path, window, |this, state, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let query = state.read(cx).value().to_string();
                    this.locate(query, window, cx);
                }
            }),
        ];
        Self {
            store,
            split: cx.new(|_| ResizableState::default()),
            widths: vec![190., 250.],
            root: None,
            selected: None,
            expanded: BTreeSet::new(),
            folders: BTreeMap::new(),
            tree,
            path,
            editor: None,
            preview: None,
            status: None,
            revision: 0,
            search_revision: 0,
            watcher: None,
            _subscriptions: subscriptions,
        }
    }

    fn persist(&self, cx: &mut Context<Self>) {
        let Some(root) = &self.root else {
            return;
        };
        let key = root.to_string_lossy().to_string();
        let navigation = Navigation {
            selected: self.selected.clone(),
            expanded: self.expanded.clone(),
            widths: self.widths.clone(),
        };
        let write = self.store.save("file-navigation", &key, &navigation);
        cx.spawn(async move |this, cx| {
            if let Err(error) = cx.background_spawn(write).await {
                let _ = this.update(cx, |this, cx| {
                    this.status = Some(format!("Could not save file navigation: {error}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn set_project(
        &mut self,
        root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.root.as_ref() == Some(&root) {
            return;
        }
        self.revision += 1;
        self.search_revision += 1;
        let revision = self.revision;
        self.root = Some(root.clone());
        self.selected = None;
        self.preview = None;
        self.editor = None;
        self.folders.clear();
        self.expanded.clear();
        self.path
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.load_folder(root.clone(), window, cx);
        let store = self.store.clone();
        let restore_root = root.clone();
        cx.spawn_in(window, async move |this, cx| {
            let key = restore_root.to_string_lossy().to_string();
            let result = cx
                .background_spawn(
                    async move { store.get::<Navigation>("file-navigation", &key).await },
                )
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.root.as_ref() != Some(&restore_root) || this.revision != revision {
                    return;
                }
                if let Ok(Some(navigation)) = result {
                    this.expanded = navigation.expanded;
                    if navigation.widths.len() == 2
                        && navigation.widths.iter().all(|w| w.is_finite() && *w > 0.)
                    {
                        this.widths = navigation.widths;
                    }
                    for folder in this.expanded.clone() {
                        this.load_folder(folder, window, cx);
                    }
                    if let Some(path) = navigation.selected {
                        this.open(path, window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        let (send, receive) = async_channel::bounded(64);
        let overflow = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let overflowed = overflow.clone();
        match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = event
                && send.try_send(event).is_err()
            {
                overflowed.store(true, std::sync::atomic::Ordering::Release);
            }
        }) {
            Ok(mut watcher) => {
                if let Err(error) = watcher.watch(&root, RecursiveMode::NonRecursive) {
                    self.status = Some(error.to_string());
                }
                self.watcher = Some(watcher);
            }
            Err(error) => self.status = Some(error.to_string()),
        }
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = cx
                .background_spawn({
                    let receive = receive.clone();
                    async move { receive.recv().await }
                })
                .await
            {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(180))
                    .await;
                let mut paths = event.paths;
                while let Ok(event) = receive.try_recv() {
                    paths.extend(event.paths);
                }
                let refresh_all = overflow.swap(false, std::sync::atomic::Ordering::AcqRel);
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.root.as_ref() != Some(&root) {
                        return;
                    }
                    for folder in this.folders.keys().cloned().collect::<Vec<_>>() {
                        if refresh_all
                            || paths.iter().any(|path| {
                                path.parent() == Some(folder.as_path()) || path == &folder
                            })
                        {
                            this.load_folder(folder, window, cx);
                        }
                    }
                    if let Some(path) = this.selected.clone()
                        && (refresh_all || paths.contains(&path))
                    {
                        this.open(path, window, cx);
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn rebuild_tree(&mut self, cx: &mut Context<Self>) {
        fn build(
            path: &Path,
            folders: &BTreeMap<PathBuf, Vec<Entry>>,
            expanded: &BTreeSet<PathBuf>,
        ) -> Vec<TreeItem> {
            folders
                .get(path)
                .into_iter()
                .flatten()
                .map(|entry| {
                    let id = entry.path.to_string_lossy().to_string();
                    let mut item = TreeItem::new(
                        id.clone(),
                        entry
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    );
                    if entry.directory {
                        let children = if folders.contains_key(&entry.path) {
                            build(&entry.path, folders, expanded)
                        } else {
                            vec![TreeItem::new(format!("{id}::pending"), "Loading…").disabled(true)]
                        };
                        item = item
                            .children(children)
                            .expanded(expanded.contains(&entry.path));
                    }
                    item
                })
                .collect()
        }
        if let Some(root) = &self.root {
            let items = build(root, &self.folders, &self.expanded);
            let selected = self.tree.read(cx).selected_item().cloned();
            self.tree.update(cx, |tree, cx| {
                tree.set_items(items, cx);
                tree.set_selected_item(selected.as_ref(), cx);
            });
        }
    }

    fn load_folder(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let expected = root.clone();
        cx.spawn_in(window, async move |this, cx| {
            let folder = path.clone();
            let result = cx
                .background_spawn(async move { entries(&root, &folder) })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.root.as_ref() != Some(&expected) {
                    return;
                }
                match result {
                    Ok(entries) => {
                        if let Some(watcher) = this.watcher.as_mut() {
                            let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
                        }
                        this.folders.insert(path, entries);
                        this.rebuild_tree(cx);
                    }
                    Err(error) => this.status = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.revision += 1;
        self.search_revision += 1;
        let revision = self.revision;
        let same_file = self.selected.as_ref() == Some(&path);
        self.selected = Some(path.clone());
        self.status = Some("Loading file…".into());
        self.persist(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { read_preview(&root, &path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.revision != revision {
                    return;
                }
                let previous = if same_file {
                    this.editor.take()
                } else {
                    this.editor = None;
                    None
                };
                match result {
                    Ok(preview) => {
                        if let Some(parent) = this.selected.as_ref().and_then(|path| path.parent())
                            && let Some(watcher) = this.watcher.as_mut()
                        {
                            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
                        }
                        if let Preview::Text(text, extension) = &preview
                            && !matches!(extension.as_str(), "md" | "markdown")
                        {
                            let language = match extension.as_str() {
                                "rs" => "rust",
                                "js" => "javascript",
                                "ts" => "typescript",
                                "py" => "python",
                                "sh" => "bash",
                                "html" => "html",
                                "json" => "json",
                                "toml" => "toml",
                                "css" => "css",
                                _ => "plain",
                            };
                            this.editor = if let Some(editor) = previous {
                                editor.update(cx, |state, cx| {
                                    if state.value().as_str() != text {
                                        let offset = state.scroll_offset();
                                        state.set_value(text.clone(), window, cx);
                                        state.set_scroll_offset(offset, cx);
                                    }
                                });
                                Some(editor)
                            } else {
                                Some(cx.new(|cx| {
                                    EditorState::new(window, cx)
                                        .language(language)
                                        .default_value(text.clone())
                                }))
                            };
                        }
                        this.preview = Some(preview);
                        this.status = None;
                    }
                    Err(error) => {
                        this.preview = None;
                        this.status = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn locate(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        self.search_revision += 1;
        let revision = self.search_revision;
        let Some(root) = self.root.clone() else {
            return;
        };
        if query.trim().is_empty() {
            self.rebuild_tree(cx);
            return;
        }
        let candidate = if Path::new(&query).is_absolute() {
            PathBuf::from(&query)
        } else {
            root.join(&query)
        };
        cx.spawn_in(window, async move |this, cx| {
            let expected = root.clone();
            let result = cx
                .background_spawn(async move {
                    if candidate.exists() {
                        return Ok::<_, anyhow::Error>((Some(candidate.canonicalize()?), vec![]));
                    }
                    let mut queue = vec![root.clone()];
                    let mut found = vec![];
                    let mut visited = 0;
                    while let Some(directory) = queue.pop() {
                        for entry in entries(&root, &directory).unwrap_or_default() {
                            visited += 1;
                            if !entry.directory
                                && entry
                                    .path
                                    .to_string_lossy()
                                    .to_lowercase()
                                    .contains(&query.to_lowercase())
                            {
                                found.push(entry.clone());
                            }
                            if entry.directory
                                && !matches!(
                                    entry.path.file_name().and_then(|s| s.to_str()),
                                    Some(".git" | "target" | "node_modules")
                                )
                            {
                                queue.push(entry.path);
                            }
                            if visited >= 20_000 || found.len() >= 300 {
                                return Ok((None, found));
                            }
                        }
                    }
                    Ok((None, found))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.root.as_ref() != Some(&expected) || this.search_revision != revision {
                    return;
                }
                match result {
                    Ok((Some(path), _)) if path.starts_with(&expected) => {
                        this.open(path, window, cx)
                    }
                    Ok((None, found)) => {
                        let items = found
                            .into_iter()
                            .map(|entry| {
                                TreeItem::new(
                                    entry.path.to_string_lossy().to_string(),
                                    entry
                                        .path
                                        .strip_prefix(&expected)
                                        .unwrap_or(&entry.path)
                                        .to_string_lossy()
                                        .to_string(),
                                )
                            })
                            .collect::<Vec<_>>();
                        this.tree.update(cx, |tree, cx| tree.set_items(items, cx));
                    }
                    Ok(_) => this.status = Some("Path is outside the opened project.".into()),
                    Err(error) => this.status = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn external(&self, reveal: bool, cx: &mut App) {
        if let Some(path) = &self.selected {
            if reveal {
                cx.reveal_path(path);
            } else {
                cx.open_with_system(path);
            }
        }
    }
}

impl Render for Files {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = if let Some(status) = &self.status {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .text_color(cx.theme().muted_foreground)
                .child(status.clone())
                .into_any_element()
        } else {
            match &self.preview {
                Some(Preview::Text(text, extension))
                    if matches!(extension.as_str(), "md" | "markdown") =>
                {
                    div()
                        .id("markdown-preview")
                        .size_full()
                        .overflow_y_scroll()
                        .p_4()
                        .child(TextView::markdown("file-markdown", text.clone()))
                        .into_any_element()
                }
                Some(Preview::Text(_, _)) => div()
                    .id("code-preview")
                    .test_support()
                    .size_full()
                    .children(self.editor.as_ref().map(|state| {
                        Editor::new(state)
                            .aria_label("Read-only File Preview")
                            .readonly(true)
                            .appearance(false)
                            .bordered(false)
                            .h_full()
                    }))
                    .into_any_element(),
                Some(Preview::Image(path)) => div()
                    .size_full()
                    .p_4()
                    .child(img(path.clone()).size_full().object_fit(ObjectFit::Contain))
                    .into_any_element(),
                Some(Preview::External(reason)) => div()
                    .size_full()
                    .v_flex()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(reason.clone())
                    .child(
                        Button::new("open-external-preview")
                            .label("Open in Default App")
                            .on_click(cx.listener(|this, _, _, cx| this.external(false, cx))),
                    )
                    .into_any_element(),
                None => div()
                    .size_full()
                    .h_flex()
                    .justify_center()
                    .gap_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(Icon::new(IconName::File).small())
                    .child("Select a file")
                    .into_any_element(),
            }
        };
        let tree = Tree::new(&self.tree, |_, entry, _, _, _| {
            ListItem::new(entry.item().id.clone()).child(
                div()
                    .h_flex()
                    .gap_2()
                    .pl(rems(entry.depth() as f32 * 0.75))
                    .child(
                        Icon::new(if entry.is_folder() {
                            if entry.is_expanded() {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            }
                        } else {
                            IconName::File
                        })
                        .small(),
                    )
                    .child(entry.item().label.clone()),
            )
        });
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .v_flex()
            .text_sm()
            .child(
                div()
                    .h_8()
                    .flex_none()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .h_flex()
                    .justify_end()
                    .px_2()
                    .gap_1()
                    .child(
                        Button::new("reveal-file")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Folder)
                            .tooltip("Reveal in Finder")
                            .accessibility_label("Reveal in Finder")
                            .disabled(self.selected.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.external(true, cx))),
                    )
                    .child(
                        Button::new("copy-file-path")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .tooltip("Copy Path")
                            .accessibility_label("Copy Path")
                            .disabled(self.selected.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(path) = &this.selected {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        path.to_string_lossy().to_string(),
                                    ));
                                }
                            })),
                    ),
            )
            .child(
                h_resizable("files-split")
                    .with_state(&self.split)
                    .child(
                        resizable_panel()
                            .size(px(self.widths[0]))
                            .size_range(px(100.)..px(1500.))
                            .child(preview),
                    )
                    .child(
                        resizable_panel()
                            .size(px(self.widths[1]))
                            .size_range(px(120.)..px(1500.))
                            .child(
                                div()
                                    .size_full()
                                    .v_flex()
                                    .child(
                                        div().p_2().child(
                                            Input::new(&self.path)
                                                .id("file-path")
                                                .aria_label("File Path")
                                                .small()
                                                .appearance(false)
                                                .prefix(Icon::new(IconName::Search).small()),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .px_3()
                                            .py_2()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Project Docs"),
                                    )
                                    .child(div().flex_1().min_h_0().child(tree)),
                            ),
                    )
                    .on_resize({
                        let owner = cx.entity().downgrade();
                        move |state, _, cx| {
                            let sizes = state
                                .read(cx)
                                .sizes()
                                .iter()
                                .map(|s| f32::from(*s))
                                .collect::<Vec<_>>();
                            let _ = owner.update(cx, |this, cx| {
                                if sizes.len() == 2 {
                                    this.widths = sizes;
                                    this.persist(cx);
                                }
                            });
                        }
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Files, Preview, read_preview};
    use crate::storage::Store;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};
    #[gpui_kit::test]
    async fn rapid_file_and_project_changes_discard_obsolete_reads(cx: &mut TestAppContext) {
        let temporary = tempfile::tempdir().unwrap();
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("old.txt"), "OLD".repeat(100_000)).unwrap();
        std::fs::write(second.join("current.txt"), "CURRENT").unwrap();
        let store = Store::open(temporary.path().join("data")).unwrap();
        cx.dispatcher.allow_parking();
        cx.update(gpui_kit::init);
        let (handle, files) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Files::new(store, window, cx))
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            files.update(cx, |files, cx| {
                files.set_project(first.clone(), window, cx);
                files.open(first.join("old.txt"), window, cx);
                files.set_project(second.clone(), window, cx);
                files.open(second.join("current.txt"), window, cx);
            });
        })
        .unwrap();
        cx.condition(
            &files,
            |files, _| matches!(&files.preview, Some(Preview::Text(text, _)) if text == "CURRENT"),
        )
        .await;
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(files.read(cx).selected, Some(second.join("current.txt")));
            assert!(
                matches!(&files.read(cx).preview, Some(Preview::Text(text, _)) if text == "CURRENT")
            );
        });
        use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("code-preview", cx);
            window.input("FORBIDDEN_EDIT", cx);
            window.render_frame(cx);
            assert_eq!(
                window.find_by_label("Read-only File Preview").value(),
                Some("CURRENT")
            );
        })
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(second.join("current.txt")).unwrap(),
            "CURRENT"
        );
    }
    #[test]
    fn source_is_readonly_and_symlinks_cannot_escape_project() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = root.path().join("index.html");
        std::fs::write(&file, "<script>alert(1)</script>").unwrap();
        assert!(
            matches!(read_preview(root.path(), &file).unwrap(), Preview::Text(_, extension) if extension=="html")
        );
        let secret = outside.path().join("outside");
        std::fs::write(&secret, "outside").unwrap();
        std::os::unix::fs::symlink(secret, root.path().join("link")).unwrap();
        assert!(read_preview(root.path(), &root.path().join("link")).is_err());
    }
}
