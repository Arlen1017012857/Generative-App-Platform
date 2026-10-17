//! Local PTY ownership and VT snapshots, adapted from Ghostex terminal_model.rs.
use super::ghostty_vt::*;
use anyhow::Result;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
};

#[derive(Clone)]
pub(crate) struct Cell {
    pub(crate) text: String,
    pub(crate) wide: VtCellWide,
    pub(crate) fg: ffi::GhosttyColorRgb,
    pub(crate) bg: Option<ffi::GhosttyColorRgb>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
}
#[derive(Clone, Default)]
pub(crate) struct Frame {
    pub(crate) rows: Vec<Vec<Cell>>,
    pub(crate) cursor: Option<(u16, u16)>,
    pub(crate) cursor_visible: bool,
    pub(crate) offset: u64,
}

pub(crate) struct TerminalModel {
    terminal: Arc<Mutex<VtTerminal>>,
    render: VtRenderState,
    master: Box<dyn MasterPty + Send>,
    write: mpsc::Sender<Vec<u8>>,
    child: Arc<Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>>,
    exit: Arc<Mutex<Option<u32>>>,
    key: VtKeyEncoder,
    mouse: VtMouseEncoder,
    dimensions: (u16, u16),
}

impl TerminalModel {
    pub(crate) fn spawn(
        shell: &Path,
        directory: &Path,
        notify: async_channel::Sender<()>,
    ) -> Result<Self> {
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new(shell);
        command.arg("-l");
        command.cwd(directory);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env_remove("NO_COLOR");
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let child = Arc::new(Mutex::new(Some(child)));
        let owner = child.clone();
        let exit = Arc::new(Mutex::new(None));
        let exit_status = exit.clone();
        let exit_notify = notify.clone();
        thread::Builder::new()
            .name("terminal-child".into())
            .spawn(move || {
                loop {
                    let status = {
                        let mut guard = owner.lock().unwrap();
                        let Some(child) = guard.as_mut() else {
                            return;
                        };
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                guard.take();
                                Some(status.exit_code())
                            }
                            _ => None,
                        }
                    };
                    if let Some(code) = status {
                        *exit_status.lock().unwrap() = Some(code);
                        let _ = exit_notify.try_send(());
                        return;
                    }
                    thread::sleep(std::time::Duration::from_millis(40));
                }
            })?;
        let mut writer = pair.master.take_writer()?;
        let (write, receive) = mpsc::channel::<Vec<u8>>();
        thread::Builder::new()
            .name("terminal-write".into())
            .spawn(move || {
                for bytes in receive {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                }
            })?;
        let mut terminal = VtTerminal::new(80, 24, 16 * 1024 * 1024)?;
        let reply = write.clone();
        terminal.set_host_callbacks(VtHostCallbacks {
            write_pty: Some(Box::new(move |bytes| {
                let _ = reply.send(bytes.to_vec());
            })),
            ..Default::default()
        })?;
        let terminal = Arc::new(Mutex::new(terminal));
        let reader_terminal = terminal.clone();
        let mut reader = pair.master.try_clone_reader()?;
        thread::Builder::new()
            .name("terminal-read".into())
            .spawn(move || {
                let mut buffer = [0; 16384];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => {
                            reader_terminal.lock().unwrap().feed(&buffer[..count]);
                            let _ = notify.try_send(());
                        }
                    }
                }
            })?;
        Ok(Self {
            terminal,
            render: VtRenderState::new()?,
            master: pair.master,
            write,
            child,
            exit,
            key: VtKeyEncoder::new()?,
            mouse: VtMouseEncoder::new()?,
            dimensions: (80, 24),
        })
    }
    pub(crate) fn write(&self, bytes: &[u8]) {
        let _ = self.write.send(bytes.to_vec());
    }
    pub(crate) fn paste(&mut self, text: &str) {
        let bracketed = self
            .terminal
            .lock()
            .unwrap()
            .mode(ffi::GHOSTTY_MODE_BRACKETED_PASTE)
            .unwrap_or(false);
        if let Ok(bytes) = encode_paste(text, bracketed) {
            self.write(&bytes);
        }
    }
    pub(crate) fn key(&mut self, input: &VtKeyInput<'_>) -> bool {
        self.key
            .sync_from_terminal(&mut self.terminal.lock().unwrap(), VtOptionAsAlt::False);
        let mut bytes = vec![];
        if self.key.encode(input, &mut bytes).is_ok() && !bytes.is_empty() {
            self.write(&bytes);
            true
        } else {
            false
        }
    }
    pub(crate) fn mouse(
        &mut self,
        input: VtMouseInput,
        cols: u32,
        rows: u32,
        cw: u32,
        ch: u32,
    ) -> bool {
        self.mouse
            .sync_from_terminal(&mut self.terminal.lock().unwrap());
        self.mouse.set_size(cols * cw, rows * ch, cw, ch);
        let mut bytes = vec![];
        if self.mouse.encode(&input, &mut bytes).is_ok() && !bytes.is_empty() {
            self.write(&bytes);
            true
        } else {
            false
        }
    }
    pub(crate) fn scroll(&mut self, lines: isize) {
        self.terminal
            .lock()
            .unwrap()
            .scroll_viewport(VtScrollViewport::Delta(lines));
    }
    pub(crate) fn mouse_tracking(&mut self) -> bool {
        self.terminal
            .lock()
            .unwrap()
            .mouse_tracking()
            .unwrap_or(false)
    }
    pub(crate) fn alternate_scroll(&mut self) -> bool {
        let mut terminal = self.terminal.lock().unwrap();
        terminal.alternate_screen_active().unwrap_or(false)
            && terminal.mode(ffi::GHOSTTY_MODE_ALT_SCROLL).unwrap_or(false)
    }
    pub(crate) fn resize(&mut self, cols: u16, rows: u16, cw: u32, ch: u32) -> Result<()> {
        if self.dimensions == (cols, rows) {
            return Ok(());
        }
        self.terminal.lock().unwrap().resize(cols, rows, cw, ch)?;
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: (u32::from(cols) * cw).min(65535) as u16,
            pixel_height: (u32::from(rows) * ch).min(65535) as u16,
        })?;
        self.dimensions = (cols, rows);
        Ok(())
    }
    pub(crate) fn colors(&mut self, fg: ffi::GhosttyColorRgb, bg: ffi::GhosttyColorRgb) {
        let _ = self.terminal.lock().unwrap().set_default_colors(
            fg,
            bg,
            None,
            &default_color_palette(),
        );
    }
    pub(crate) fn snapshot(&mut self) -> Result<Frame> {
        let offset = {
            let mut terminal = self.terminal.lock().unwrap();
            self.render.update(&mut terminal)?;
            terminal.scrollbar()?.offset
        };
        let colors = self.render.colors()?;
        let cursor = self.render.cursor_viewport()?;
        let cursor_visible = self.render.cursor_visible()?;
        let mut rows = vec![];
        let mut iterator = self.render.rows()?;
        while let Some(mut row) = iterator.next_row() {
            let mut cells = vec![];
            let mut iter = row.cells()?;
            while let Some(cell) = iter.next_cell() {
                let mut codepoints = vec![];
                cell.append_codepoints(&mut codepoints)?;
                let style = cell.style()?;
                let mut fg = cell.fg_color()?.unwrap_or(colors.foreground);
                let mut bg = cell.bg_color()?;
                if style.inverse {
                    let old = fg;
                    fg = bg.unwrap_or(colors.background);
                    bg = Some(old);
                }
                cells.push(Cell {
                    text: if style.invisible {
                        String::new()
                    } else {
                        codepoints.into_iter().filter_map(char::from_u32).collect()
                    },
                    wide: cell.wide()?,
                    fg,
                    bg,
                    bold: style.bold,
                    italic: style.italic,
                });
            }
            row.clear_dirty()?;
            rows.push(cells);
        }
        self.render.clear_dirty()?;
        Ok(Frame {
            rows,
            cursor,
            cursor_visible,
            offset,
        })
    }
    pub(crate) fn exit(&self) -> Option<u32> {
        *self.exit.lock().unwrap()
    }
    pub(crate) fn terminate(&mut self) {
        let mut guard = self.child.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            if child.try_wait().ok().flatten().is_some() {
                guard.take();
                return;
            }
            if let Some(group) = self.master.process_group_leader() {
                unsafe {
                    libc::kill(-group, libc::SIGTERM);
                }
            }
            if let Some(pid) = child.process_id() {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGTERM);
                }
            }
            let _ = child.kill();
        }
    }
}
impl Drop for TerminalModel {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_live_terminal_terminates_and_reaps_shell() {
        let dir = tempfile::tempdir().unwrap();
        let (send, _) = async_channel::bounded(1);
        let model = TerminalModel::spawn(Path::new("/bin/sh"), dir.path(), send).unwrap();
        let owner = model.child.clone();
        let pid = owner
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .process_id()
            .unwrap();
        drop(model);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while owner.lock().unwrap().is_some() && std::time::Instant::now() < until {
            thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            owner.lock().unwrap().is_none(),
            "Child watcher must reap the process"
        );
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }
    #[test]
    fn real_pty_output_resize_and_exit() {
        let dir = tempfile::tempdir().unwrap();
        let (send, _) = async_channel::bounded(1);
        let mut model = TerminalModel::spawn(Path::new("/bin/sh"), dir.path(), send).unwrap();
        model.resize(100, 30, 8, 16).unwrap();
        model.write(b"printf 'GAP_PTY_OK\\n'; exit 0\n");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while model.exit().is_none() && std::time::Instant::now() < until {
            thread::sleep(std::time::Duration::from_millis(20));
        }
        let frame = model.snapshot().unwrap();
        let text: String = frame
            .rows
            .iter()
            .flatten()
            .map(|c| c.text.as_str())
            .collect();
        assert!(text.contains("GAP_PTY_OK"));
        assert_eq!(model.exit(), Some(0));
        assert_eq!(frame.rows.len(), 30);
    }
}
