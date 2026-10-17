//! Native grid painting and IME integration adapted from Ghostex terminal_element.rs.
use super::{
    ghostty_vt::*,
    model::{Frame, TerminalModel},
};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{prelude::*, *};
use std::ops::Range;

gpui_kit::actions!(terminal, [Tab, BackTab]);

pub(crate) struct TerminalView {
    model: TerminalModel,
    frame: Frame,
    focus: FocusHandle,
    marked: String,
    marked_selection: Range<usize>,
    bounds: Bounds<Pixels>,
    cell: Size<Pixels>,
    anchor: Option<(u64, usize)>,
    selection: Option<((u64, usize), (u64, usize))>,
    colors: Option<(ffi::GhosttyColorRgb, ffi::GhosttyColorRgb)>,
}
impl TerminalView {
    pub(crate) fn new(
        model: TerminalModel,
        receive: async_channel::Receiver<()>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.spawn(async move |this, cx| {
            while cx
                .background_spawn({
                    let receive = receive.clone();
                    async move { receive.recv().await }
                })
                .await
                .is_ok()
            {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            model,
            frame: Frame::default(),
            focus: cx.focus_handle().tab_stop(true),
            marked: String::new(),
            marked_selection: 0..0,
            bounds: Bounds::default(),
            cell: size(px(8.), px(18.)),
            anchor: None,
            selection: None,
            colors: None,
        }
    }
    pub(crate) fn is_running(&self) -> bool {
        self.model.exit().is_none()
    }
    pub(crate) fn appearance_changed(&mut self, cx: &mut Context<Self>) {
        self.colors = None;
        cx.notify();
    }
    fn report_mouse(
        &mut self,
        position: Point<Pixels>,
        action: VtMouseAction,
        button: Option<VtMouseButton>,
        modifiers: &Modifiers,
    ) -> bool {
        if modifiers.shift || !self.model.mouse_tracking() {
            return false;
        }
        let p = position - self.bounds.origin;
        let cols = (self.bounds.size.width / self.cell.width).max(1.) as u32;
        let rows = (self.bounds.size.height / self.cell.height).max(1.) as u32;
        self.model.mouse(
            VtMouseInput {
                action,
                button,
                mods: vt_mods(modifiers),
                x: f32::from(p.x),
                y: f32::from(p.y),
            },
            cols,
            rows,
            f32::from(self.cell.width).round().max(1.) as u32,
            f32::from(self.cell.height).round().max(1.) as u32,
        )
    }
    fn cell_at(&self, position: Point<Pixels>) -> (u64, usize) {
        let p = position - self.bounds.origin;
        let row = ((p.y / self.cell.height).max(0.) as usize)
            .min(self.frame.rows.len().saturating_sub(1));
        let col = ((p.x / self.cell.width).max(0.) as usize)
            .min(self.frame.rows.get(row).map_or(0, Vec::len));
        (self.frame.offset + row as u64, col)
    }
    fn copy(&self, cx: &mut App) {
        if let Some((start, end)) = self.selection {
            let (a, b) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            let mut lines = vec![];
            for (row, cells) in self.frame.rows.iter().enumerate() {
                let absolute = self.frame.offset + row as u64;
                if absolute < a.0 || absolute > b.0 {
                    continue;
                }
                let start = if absolute == a.0 { a.1 } else { 0 };
                let end = if absolute == b.0 { b.1 } else { cells.len() };
                let text = cells
                    .iter()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .filter(|cell| {
                        !matches!(cell.wide, VtCellWide::SpacerHead | VtCellWide::SpacerTail)
                    })
                    .map(|cell| cell.text.as_str())
                    .collect::<String>();
                lines.push(text.trim_end().to_owned());
            }
            cx.write_to_clipboard(ClipboardItem::new_string(lines.join("\n")));
        }
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let mods = &key.modifiers;
        if mods.platform && key.key == "c" {
            self.copy(cx);
            cx.stop_propagation();
            return;
        }
        if mods.platform && key.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.model.paste(&text);
            }
            cx.stop_propagation();
            return;
        }
        if mods.platform && !mods.alt && !mods.control {
            let bytes = match key.key.as_str() {
                "backspace" => Some(&b"\x15"[..]),
                "left" => Some(&b"\x01"[..]),
                "right" => Some(&b"\x05"[..]),
                _ => None,
            };
            if let Some(bytes) = bytes {
                self.model.write(bytes);
                cx.stop_propagation();
                return;
            }
        }
        if !self.marked.is_empty() {
            return;
        }
        // Unbound Command chords belong to the desktop, never to a shell's key encoder.
        if mods.platform {
            return;
        }
        if mods.alt && !mods.control && !mods.shift {
            let bytes = match key.key.as_str() {
                "left" => Some(&b"\x1bb"[..]),
                "right" => Some(&b"\x1bf"[..]),
                _ => None,
            };
            if let Some(bytes) = bytes {
                self.model.write(bytes);
                cx.stop_propagation();
                return;
            }
        }
        // Printable text is committed by the platform input handler, preserving dead keys and IME.
        if !mods.control && !mods.platform && (key.key.chars().count() == 1 || key.key == "space") {
            return;
        }
        let (physical, codepoint) = keystroke_vt_key(key);
        if self.model.key(&VtKeyInput {
            action: VtKeyAction::Press,
            key: physical,
            mods: vt_mods(mods),
            consumed_mods: 0,
            utf8: None,
            unshifted_codepoint: codepoint,
            composing: false,
        }) {
            self.selection = None;
            cx.stop_propagation();
            cx.notify();
        }
        let _ = window;
    }
    fn prepare(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PaintFrame {
        self.bounds = bounds;
        let font_size = window.rem_size() * 0.875;
        let font = font("Menlo");
        let font_id = window.text_system().resolve_font(&font);
        let width = window
            .text_system()
            .advance(font_id, font_size, 'm')
            .map_or(font_size * 0.6, |size| size.width);
        let height = font_size * 1.45;
        self.cell = size(width, height);
        let cols = (bounds.size.width / width).floor().max(1.) as u16;
        let rows = (bounds.size.height / height).floor().max(1.) as u16;
        let _ = self.model.resize(
            cols,
            rows,
            (f32::from(width) * window.scale_factor()) as u32,
            (f32::from(height) * window.scale_factor()) as u32,
        );
        let fg = to_rgb(cx.theme().foreground);
        let bg = to_rgb(cx.theme().background);
        if self.colors != Some((fg, bg)) {
            self.model.colors(fg, bg);
            self.colors = Some((fg, bg));
        }
        if let Ok(frame) = self.model.snapshot() {
            self.frame = frame;
        }
        let mut runs = vec![];
        let mut backgrounds = vec![];
        for (row, cells) in self.frame.rows.iter().enumerate() {
            for (col, cell) in cells.iter().enumerate() {
                if let Some(bg) = cell.bg {
                    backgrounds.push((
                        Bounds::new(
                            bounds.origin + point(width * col as f32, height * row as f32),
                            self.cell,
                        ),
                        color(bg),
                    ));
                }
            }
            let mut col = 0;
            while col < cells.len() {
                let start = col;
                let first = &cells[col];
                if matches!(first.wide, VtCellWide::SpacerTail | VtCellWide::SpacerHead) {
                    col += 1;
                    continue;
                }
                let mut text = String::new();
                let wide = first.wide == VtCellWide::Wide;
                loop {
                    let cell = &cells[col];
                    text.push_str(if cell.text.is_empty() {
                        " "
                    } else {
                        &cell.text
                    });
                    col += 1;
                    if wide || col >= cells.len() {
                        break;
                    }
                    let next = &cells[col];
                    if next.wide != VtCellWide::Narrow
                        || next.fg != first.fg
                        || next.bold != first.bold
                        || next.italic != first.italic
                    {
                        break;
                    }
                }
                let mut font = font.clone();
                if first.bold {
                    font.weight = FontWeight::BOLD;
                }
                if first.italic {
                    font.style = FontStyle::Italic;
                }
                let run = TextRun {
                    len: text.len(),
                    font,
                    color: color(first.fg),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(
                    text.into(),
                    font_size,
                    &[run],
                    if wide { None } else { Some(width) },
                );
                runs.push((
                    bounds.origin + point(width * start as f32, height * row as f32),
                    shaped,
                ));
            }
        }
        let cursor = if self.frame.cursor_visible {
            self.frame.cursor.map(|(col, row)| {
                Bounds::new(
                    bounds.origin + point(width * f32::from(col), height * f32::from(row)),
                    self.cell,
                )
            })
        } else {
            None
        };
        let marked = if self.marked.is_empty() {
            None
        } else {
            let run = TextRun {
                len: self.marked.len(),
                font,
                color: cx.theme().foreground,
                background_color: Some(cx.theme().background),
                underline: Some(UnderlineStyle {
                    thickness: px(1.),
                    color: None,
                    wavy: false,
                }),
                strikethrough: None,
            };
            Some(window.text_system().shape_line(
                self.marked.clone().into(),
                font_size,
                &[run],
                None,
            ))
        };
        PaintFrame {
            runs,
            backgrounds,
            cursor,
            marked,
            height,
            focus: self.focus.clone(),
            view: cx.entity(),
        }
    }
}

struct PaintFrame {
    runs: Vec<(Point<Pixels>, ShapedLine)>,
    backgrounds: Vec<(Bounds<Pixels>, Hsla)>,
    cursor: Option<Bounds<Pixels>>,
    marked: Option<ShapedLine>,
    height: Pixels,
    focus: FocusHandle,
    view: Entity<TerminalView>,
}

impl Render for TerminalView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prepare = cx.entity();
        let paint = cx.entity();
        div()
            .id("terminal-surface")
            .role(Role::Terminal)
            .aria_label("Terminal")
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|this, _: &Tab, _, cx| {
                this.model.write(b"\t");
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &BackTab, _, cx| {
                this.model.write(b"\x1b[Z");
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    if this.report_mouse(
                        event.position,
                        VtMouseAction::Press,
                        Some(VtMouseButton::Left),
                        &event.modifiers,
                    ) {
                        cx.stop_propagation();
                        return;
                    }
                    let cell = this.cell_at(event.position);
                    this.anchor = Some(cell);
                    this.selection = None;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.report_mouse(
                    event.position,
                    VtMouseAction::Motion,
                    if event.pressed_button == Some(MouseButton::Left) {
                        Some(VtMouseButton::Left)
                    } else {
                        None
                    },
                    &event.modifiers,
                ) {
                    cx.stop_propagation();
                    return;
                }
                if event.pressed_button == Some(MouseButton::Left)
                    && let Some(anchor) = this.anchor
                {
                    let mut end = this.cell_at(event.position);
                    end.1 += 1;
                    this.selection = Some((anchor, end));
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.report_mouse(
                        event.position,
                        VtMouseAction::Release,
                        Some(VtMouseButton::Left),
                        &event.modifiers,
                    );
                    this.anchor = None;
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta = event.delta.pixel_delta(this.cell.height);
                let lines = (delta.y / this.cell.height).round() as isize;
                if lines == 0 {
                    return;
                }
                if this.model.mouse_tracking() {
                    for _ in 0..lines.unsigned_abs().min(50) {
                        this.report_mouse(
                            event.position,
                            VtMouseAction::Press,
                            Some(if lines > 0 {
                                VtMouseButton::WheelUp
                            } else {
                                VtMouseButton::WheelDown
                            }),
                            &event.modifiers,
                        );
                    }
                } else if this.model.alternate_scroll() {
                    let bytes = if lines > 0 {
                        &b"\x1b[A"[..]
                    } else {
                        &b"\x1b[B"[..]
                    };
                    for _ in 0..lines.unsigned_abs().min(50) {
                        this.model.write(bytes);
                    }
                } else {
                    this.model.scroll(-lines);
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        prepare.update(cx, |this, cx| this.prepare(bounds, window, cx))
                    },
                    move |bounds, frame, window, cx| {
                        window.handle_input(
                            &frame.focus,
                            ElementInputHandler::new(bounds, frame.view),
                            cx,
                        );
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            for (bounds, color) in frame.backgrounds {
                                window.paint_quad(fill(bounds, color));
                            }
                            let selection = paint.read(cx).selection;
                            let terminal = paint.read(cx);
                            if let Some((a, b)) = selection {
                                let (a, b) = if a <= b { (a, b) } else { (b, a) };
                                for (row, cells) in terminal.frame.rows.iter().enumerate() {
                                    let absolute = terminal.frame.offset + row as u64;
                                    if absolute >= a.0 && absolute <= b.0 {
                                        let start = if absolute == a.0 { a.1 } else { 0 };
                                        let end = if absolute == b.0 { b.1 } else { cells.len() };
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                bounds.origin
                                                    + point(
                                                        terminal.cell.width * start as f32,
                                                        terminal.cell.height * row as f32,
                                                    ),
                                                size(
                                                    terminal.cell.width
                                                        * end.saturating_sub(start) as f32,
                                                    terminal.cell.height,
                                                ),
                                            ),
                                            cx.theme().selection,
                                        ));
                                    }
                                }
                            }
                            for (origin, line) in frame.runs {
                                let _ = line.paint(
                                    origin,
                                    frame.height,
                                    TextAlign::Left,
                                    None,
                                    window,
                                    cx,
                                );
                            }
                            if let Some(cursor) = frame.cursor {
                                window.paint_quad(fill(
                                    Bounds::new(cursor.origin, size(px(1.5), cursor.size.height)),
                                    cx.theme().foreground,
                                ));
                                if let Some(marked) = frame.marked {
                                    let _ = marked.paint(
                                        cursor.origin,
                                        frame.height,
                                        TextAlign::Left,
                                        None,
                                        window,
                                        cx,
                                    );
                                }
                            }
                        });
                    },
                )
                .size_full(),
            )
            .when_some(self.model.exit(), |view, code| {
                view.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .right_0()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .bg(cx.theme().muted)
                        .child(format!("Exited · {code}")),
                )
            })
    }
}
impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let chars: Vec<u16> = self.marked.encode_utf16().collect();
        let start = range.start.min(chars.len());
        let end = range.end.clamp(start, chars.len());
        *adjusted = Some(start..end);
        Some(String::from_utf16_lossy(&chars[start..end]))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.marked_selection.clone(),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        if self.marked.is_empty() {
            None
        } else {
            Some(0..self.marked.encode_utf16().count())
        }
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked.clear();
        self.marked_selection = 0..0;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.model.write(text.as_bytes());
        self.marked.clear();
        self.marked_selection = 0..0;
        self.selection = None;
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = text.into();
        let len = text.encode_utf16().count();
        self.marked_selection = selection.unwrap_or(len..len);
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (col, row) = self.frame.cursor?;
        Some(Bounds::new(
            bounds.origin
                + point(
                    self.cell.width * f32::from(col),
                    self.cell.height * f32::from(row),
                ),
            self.cell,
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

// Raw color conversion is confined to terminal raster data, never application styling.
fn color(rgb: ffi::GhosttyColorRgb) -> Hsla {
    gpui_kit::rgb((u32::from(rgb.r) << 16) | (u32::from(rgb.g) << 8) | u32::from(rgb.b)).into()
}
fn to_rgb(color: Hsla) -> ffi::GhosttyColorRgb {
    let rgb = color.to_rgb();
    ffi::GhosttyColorRgb {
        r: (rgb.r * 255.) as u8,
        g: (rgb.g * 255.) as u8,
        b: (rgb.b * 255.) as u8,
    }
}

fn vt_mods(modifiers: &Modifiers) -> VtMods {
    let mut mods: VtMods = 0;
    if modifiers.shift {
        mods |= ffi::GHOSTTY_MODS_SHIFT;
    }
    if modifiers.control {
        mods |= ffi::GHOSTTY_MODS_CTRL;
    }
    if modifiers.alt {
        mods |= ffi::GHOSTTY_MODS_ALT;
    }
    if modifiers.platform {
        mods |= ffi::GHOSTTY_MODS_SUPER;
    }
    mods
}

fn keystroke_vt_key(keystroke: &Keystroke) -> (VtKey, u32) {
    let key = keystroke.key.as_str();
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let vt = match c.to_ascii_lowercase() {
            'a' => VtKey::A,
            'b' => VtKey::B,
            'c' => VtKey::C,
            'd' => VtKey::D,
            'e' => VtKey::E,
            'f' => VtKey::F,
            'g' => VtKey::G,
            'h' => VtKey::H,
            'i' => VtKey::I,
            'j' => VtKey::J,
            'k' => VtKey::K,
            'l' => VtKey::L,
            'm' => VtKey::M,
            'n' => VtKey::N,
            'o' => VtKey::O,
            'p' => VtKey::P,
            'q' => VtKey::Q,
            'r' => VtKey::R,
            's' => VtKey::S,
            't' => VtKey::T,
            'u' => VtKey::U,
            'v' => VtKey::V,
            'w' => VtKey::W,
            'x' => VtKey::X,
            'y' => VtKey::Y,
            'z' => VtKey::Z,
            '0' => VtKey::Digit0,
            '1' => VtKey::Digit1,
            '2' => VtKey::Digit2,
            '3' => VtKey::Digit3,
            '4' => VtKey::Digit4,
            '5' => VtKey::Digit5,
            '6' => VtKey::Digit6,
            '7' => VtKey::Digit7,
            '8' => VtKey::Digit8,
            '9' => VtKey::Digit9,
            '`' => VtKey::Backquote,
            '\\' => VtKey::Backslash,
            '[' => VtKey::BracketLeft,
            ']' => VtKey::BracketRight,
            ',' => VtKey::Comma,
            '=' => VtKey::Equal,
            '-' => VtKey::Minus,
            '.' => VtKey::Period,
            '\'' => VtKey::Quote,
            ';' => VtKey::Semicolon,
            '/' => VtKey::Slash,
            _ => VtKey::Unidentified,
        };
        return (vt, c as u32);
    }
    let vt = match key {
        "space" => VtKey::Space,
        "tab" => VtKey::Tab,
        "enter" => VtKey::Enter,
        "backspace" => VtKey::Backspace,
        "escape" => VtKey::Escape,
        "up" => VtKey::ArrowUp,
        "down" => VtKey::ArrowDown,
        "left" => VtKey::ArrowLeft,
        "right" => VtKey::ArrowRight,
        "pageup" => VtKey::PageUp,
        "pagedown" => VtKey::PageDown,
        "home" => VtKey::Home,
        "end" => VtKey::End,
        "delete" => VtKey::Delete,
        "insert" => VtKey::Insert,
        "f1" => VtKey::F1,
        "f2" => VtKey::F2,
        "f3" => VtKey::F3,
        "f4" => VtKey::F4,
        "f5" => VtKey::F5,
        "f6" => VtKey::F6,
        "f7" => VtKey::F7,
        "f8" => VtKey::F8,
        "f9" => VtKey::F9,
        "f10" => VtKey::F10,
        "f11" => VtKey::F11,
        "f12" => VtKey::F12,
        "f13" => VtKey::F13,
        "f14" => VtKey::F14,
        "f15" => VtKey::F15,
        "f16" => VtKey::F16,
        "f17" => VtKey::F17,
        "f18" => VtKey::F18,
        "f19" => VtKey::F19,
        "f20" => VtKey::F20,
        "f21" => VtKey::F21,
        "f22" => VtKey::F22,
        "f23" => VtKey::F23,
        "f24" => VtKey::F24,
        "f25" => VtKey::F25,
        _ => VtKey::Unidentified,
    };
    let codepoint = if vt == VtKey::Space {
        u32::from(' ')
    } else {
        0
    };
    (vt, codepoint)
}
