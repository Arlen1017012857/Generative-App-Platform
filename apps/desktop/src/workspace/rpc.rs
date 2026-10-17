use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// JSONL uses LF only: Unicode U+2028 and split UTF-8 sequences are valid payload.
#[derive(Default)]
pub(crate) struct JsonLines {
    buffer: Vec<u8>,
}

impl JsonLines {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>> {
        self.buffer.extend_from_slice(bytes);
        let mut values = Vec::new();
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.buffer.drain(..=end).collect();
            let line = line
                .strip_suffix(b"\n")
                .unwrap()
                .strip_suffix(b"\r")
                .unwrap_or(&line[..line.len() - 1]);
            if !line.is_empty() {
                values.push(serde_json::from_slice(line).context("Invalid Pi JSONL response")?);
            }
        }
        if self.buffer.len() > 16 * 1024 * 1024 {
            bail!("Pi response exceeded 16 MiB without a record delimiter");
        }
        Ok(values)
    }
}

#[derive(Debug)]
pub(crate) enum PiEvent {
    Json(String, Value),
    Error(String, String),
    Exited(String, Option<i32>),
}

/// One connection owns one session's process. It never replays prompts on reconnect.
pub(crate) struct PiClient {
    write: mpsc::Sender<Value>,
    child: Arc<Mutex<Option<Child>>>,
    pending: HashMap<String, (String, Instant)>,
}

impl PiClient {
    #[cfg(test)]
    pub(super) fn fixture_connection() -> (Self, mpsc::Receiver<Value>) {
        let (write, receive) = mpsc::channel();
        (
            Self {
                write,
                child: Arc::new(Mutex::new(None)),
                pending: HashMap::new(),
            },
            receive,
        )
    }
    pub(crate) fn spawn(
        executable: &Path,
        project: &Path,
        session: &Path,
        id: String,
        events: async_channel::Sender<PiEvent>,
    ) -> Result<Self> {
        use std::os::unix::process::CommandExt;
        let mut child = Command::new(executable)
            .args(["--mode", "rpc", "--session"])
            .arg(session)
            .current_dir(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .context("Could not launch Pi. Check the executable and provider configuration.")?;
        let mut input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        let mut error_output = child.stderr.take().unwrap();
        let (write, receive) = mpsc::channel::<Value>();
        thread::Builder::new()
            .name("pi-input".into())
            .spawn(move || {
                for value in receive {
                    if serde_json::to_writer(&mut input, &value).is_err()
                        || input.write_all(b"\n").is_err()
                        || input.flush().is_err()
                    {
                        break;
                    }
                }
            })?;
        let owner = Arc::new(Mutex::new(Some(child)));
        let reader_events = events.clone();
        let reader_id = id.clone();
        thread::Builder::new()
            .name("pi-output".into())
            .spawn(move || {
                let mut parser = JsonLines::default();
                let mut buffer = [0; 8192];
                loop {
                    match output.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => match parser.push(&buffer[..count]) {
                            Ok(values) => {
                                for value in values {
                                    if reader_events
                                        .send_blocking(PiEvent::Json(reader_id.clone(), value))
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                            Err(error) => {
                                let _ = reader_events.send_blocking(PiEvent::Error(
                                    reader_id.clone(),
                                    error.to_string(),
                                ));
                                break;
                            }
                        },
                        Err(error) => {
                            let _ = reader_events.send_blocking(PiEvent::Error(
                                reader_id.clone(),
                                error.to_string(),
                            ));
                            break;
                        }
                    }
                }
            })?;
        // Drain stderr independently, retaining only a bounded last diagnostic for failures.
        let diagnostic = Arc::new(Mutex::new(Vec::<u8>::new()));
        let retained = diagnostic.clone();
        thread::Builder::new()
            .name("pi-stderr".into())
            .spawn(move || {
                let mut buffer = [0; 2048];
                while let Ok(count) = error_output.read(&mut buffer) {
                    if count == 0 {
                        break;
                    }
                    let mut bytes = retained.lock().unwrap();
                    bytes.extend_from_slice(&buffer[..count]);
                    if bytes.len() > 8192 {
                        let excess = bytes.len() - 8192;
                        bytes.drain(..excess);
                    }
                }
            })?;
        let process = owner.clone();
        thread::Builder::new()
            .name("pi-lifecycle".into())
            .spawn(move || {
                loop {
                    let status = {
                        let mut guard = process.lock().unwrap();
                        let Some(child) = guard.as_mut() else {
                            return;
                        };
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                guard.take();
                                Some(status)
                            }
                            _ => None,
                        }
                    };
                    if let Some(status) = status {
                        if !status.success() {
                            let message = String::from_utf8_lossy(&diagnostic.lock().unwrap())
                                .trim()
                                .to_owned();
                            if !message.is_empty() {
                                let _ = events.send_blocking(PiEvent::Error(id.clone(), message));
                            }
                        }
                        let _ = events.send_blocking(PiEvent::Exited(id, status.code()));
                        return;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            })?;
        Ok(Self {
            write,
            child: owner,
            pending: HashMap::new(),
        })
    }

    pub(crate) fn request(&mut self, command: &str, mut body: Value) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        body["type"] = command.into();
        body["id"] = id.clone().into();
        self.write.send(body).context("Pi connection closed")?;
        self.pending
            .insert(id.clone(), (command.into(), Instant::now()));
        Ok(id)
    }

    pub(crate) fn response(&mut self, value: &Value) -> Option<String> {
        self.pending
            .remove(value.get("id")?.as_str()?)
            .map(|(command, _)| command)
    }

    pub(crate) fn timed_out(&mut self) -> Vec<String> {
        let mut result = Vec::new();
        self.pending.retain(|_, (command, start)| {
            let valid = start.elapsed() < Duration::from_secs(60);
            if !valid {
                result.push(command.clone());
            }
            valid
        });
        result
    }

    pub(crate) fn extension_response(&self, id: &str, mut value: Value) -> Result<()> {
        value["type"] = "extension_ui_response".into();
        value["id"] = id.into();
        self.write.send(value).context("Pi connection closed")
    }

    pub(crate) fn stop(&mut self) -> Result<()> {
        self.request("clear_queue", json!({}))?;
        self.request("abort", json!({}))?;
        Ok(())
    }

    pub(crate) fn terminate(&self) {
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            // The owner lock serializes reaping with signalling, preventing PID reuse races.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGTERM);
            }
            let _ = child.kill();
        }
    }

    /// Used off the UI thread before deleting the engine's transcript.
    pub(crate) fn terminate_and_wait(self) -> Result<()> {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            // Kill the process group too: tools must not outlive a deleted session.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            child
                .wait()
                .context("Could not reap the session's Pi process")?;
        }
        Ok(())
    }
}

impl Drop for PiClient {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correlates_out_of_order_responses_and_never_retries_unknown_requests() {
        let (write, receive) = mpsc::channel();
        let mut client = PiClient {
            write,
            child: Arc::new(Mutex::new(None)),
            pending: HashMap::new(),
        };
        let prompt = client
            .request("prompt", json!({"message":"hello"}))
            .unwrap();
        let state = client.request("get_state", json!({})).unwrap();
        assert_eq!(receive.recv().unwrap()["id"], prompt);
        assert_eq!(receive.recv().unwrap()["id"], state);
        assert_eq!(
            client.response(&json!({"id":state})).as_deref(),
            Some("get_state")
        );
        assert_eq!(client.response(&json!({"id":"unrelated"})), None);
        client.pending.get_mut(&prompt).unwrap().1 = Instant::now() - Duration::from_secs(61);
        assert_eq!(client.timed_out(), ["prompt"]);
        assert_eq!(client.response(&json!({"id":prompt})), None);
        assert!(
            receive.try_recv().is_err(),
            "An unknown result must not resend a prompt"
        );
    }
    #[test]
    fn frames_split_utf8_without_splitting_unicode_separators() {
        let wire = "{\"text\":\"中文\u{2028}emoji😀\"}\r\n{\"n\":2}\n".as_bytes();
        let mut parser = JsonLines::default();
        let mut values = vec![];
        for byte in wire {
            values.extend(parser.push(&[*byte]).unwrap());
        }
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["text"], "中文\u{2028}emoji😀");
    }
}
