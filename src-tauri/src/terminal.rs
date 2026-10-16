use crate::{bridge, credentials, model::*, store::Studio, tools};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{ipc::Channel, Manager};

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    Data { bytes: Vec<u8> },
    Exit { code: u32 },
}
#[derive(Default)]
struct Output {
    chunks: VecDeque<Vec<u8>>,
    size: usize,
    channel: Option<Channel<TerminalEvent>>,
}
struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: u32,
    running: Arc<AtomicBool>,
    output: Arc<Mutex<Output>>,
}
#[derive(Default)]
pub struct Terminals {
    sessions: Mutex<HashMap<String, Session>>,
    updating: AtomicBool,
}
impl Session {
    fn stop(&mut self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        tools::kill_tree(self.pid);
        let _ = self.killer.kill();
        self.running.store(false, Ordering::SeqCst);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}
impl Terminals {
    pub fn stop_all(&self) {
        for session in self.sessions.lock().unwrap().values_mut() {
            session.stop();
        }
    }
    pub fn running(&self) -> bool {
        self.sessions
            .lock()
            .unwrap()
            .values()
            .any(|s| s.running.load(Ordering::SeqCst))
    }
}
fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols: cols.clamp(20, 500),
        rows: rows.clamp(5, 300),
        pixel_width: 0,
        pixel_height: 0,
    }
}
fn attach(session: &Session, channel: Channel<TerminalEvent>) {
    let mut out = session.output.lock().unwrap();
    for bytes in &out.chunks {
        let _ = channel.send(TerminalEvent::Data {
            bytes: bytes.clone(),
        });
    }
    out.channel = Some(channel);
}
fn session_exists(session: &str) -> bool {
    let base = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")));
    base.and_then(|b| std::fs::read_dir(b.join("projects")).ok())
        .is_some_and(|dirs| {
            dirs.flatten()
                .any(|d| d.path().join(format!("{session}.jsonl")).is_file())
        })
}
fn spawn(
    cmd: CommandBuilder,
    cols: u16,
    rows: u16,
    channel: Channel<TerminalEvent>,
) -> Result<Session, String> {
    let pair = native_pty_system()
        .openpty(size(cols, rows))
        .map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Cannot start Claude Code: {e}"))?;
    drop(pair.slave);
    let running = Arc::new(AtomicBool::new(true));
    let output = Arc::new(Mutex::new(Output {
        channel: Some(channel),
        ..Default::default()
    }));
    let session = Session {
        master: pair.master,
        writer,
        killer: child.clone_killer(),
        pid: child.process_id().unwrap_or(0),
        running: running.clone(),
        output: output.clone(),
    };
    let read_output = output.clone();
    let read_thread = std::thread::spawn(move || {
        let mut buf = [0; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            let bytes = buf[..n].to_vec();
            let mut out = read_output.lock().unwrap();
            if let Some(channel) = &out.channel {
                let _ = channel.send(TerminalEvent::Data {
                    bytes: bytes.clone(),
                });
            }
            out.size += n;
            out.chunks.push_back(bytes);
            while out.size > 2_000_000 {
                if let Some(old) = out.chunks.pop_front() {
                    out.size -= old.len();
                }
            }
        }
    });
    std::thread::spawn(move || {
        let code = child.wait().map(|s| s.exit_code()).unwrap_or(1);
        let _ = read_thread.join();
        running.store(false, Ordering::SeqCst);
        if let Some(channel) = &output.lock().unwrap().channel {
            let _ = channel.send(TerminalEvent::Exit { code });
        }
    });
    Ok(session)
}
#[tauri::command]
pub async fn terminal_attach(
    app: tauri::AppHandle,
    id: String,
    channel: Channel<TerminalEvent>,
) -> Result<bool, String> {
    let hub = app.state::<Terminals>();
    let sessions = hub.sessions.lock().unwrap();
    if let Some(session) = sessions.get(&id) {
        attach(session, channel);
        return Ok(session.running.load(Ordering::SeqCst));
    }
    Ok(false)
}
#[tauri::command]
pub async fn terminal_open(
    app: tauri::AppHandle,
    id: String,
    cols: u16,
    rows: u16,
    channel: Channel<TerminalEvent>,
) -> Result<bool, String> {
    crate::work(move || {
        let hub = app.state::<Terminals>();
        let mut sessions = hub.sessions.lock().unwrap();
        if hub.updating.load(Ordering::SeqCst) {
            return Err("Claude Code is updating. Try again when it finishes.".into());
        }
        if let Some(session) = sessions
            .get(&id)
            .filter(|s| s.running.load(Ordering::SeqCst))
        {
            attach(session, channel);
            return Ok(false);
        }
        let studio = app.state::<Studio>();
        let _guard = studio.gate.lock().unwrap();
        studio.idle(&id)?;
        let mut w = studio.load(&id)?;
        let settings = studio.data()?.settings;
        let program = tools::required("claude", &settings.claude_path)?;
        let secret = match settings.mode.as_str() {
            "mimo" => credentials::optional("MIMO_API_KEY")?,
            "api" => tools::key()?,
            _ => None,
        };
        let env = tools::agent_command(&program, &settings, secret.as_deref())?;
        let dir = Path::new(&w.root).join(".framecraft/terminal");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("guidance.md"),
            include_str!("../../templates/terminal-agent.md"),
        )
        .map_err(|e| e.to_string())?;
        let config = bridge::config(&app, &id)?;
        let cwd = w.assets_folder.as_deref().unwrap_or(&w.root);
        let signature = format!(
            "{}\n{}\n{}\n{}",
            settings.mode, settings.base_url, settings.model, cwd
        );
        let saved = w
            .terminal_session
            .as_ref()
            .filter(|s| s.connection == signature && session_exists(&s.id));
        let (sid, resume) = saved
            .map(|s| (s.id.clone(), true))
            .unwrap_or_else(|| (crate::model::id(), false));
        let mut cmd = CommandBuilder::new(&program);
        cmd.cwd(cwd);
        for name in tools::PROVIDER_ENV {
            cmd.env_remove(name);
        }
        for name in [
            "MIMO_API_KEY",
            "AGENT_API_KEY",
            "VOLCENGINE_API_KEY",
            "GOOGLE_CLOUD_API_KEY",
            "GOOGLE_APPLICATION_CREDENTIALS",
        ] {
            cmd.env_remove(name);
        }
        for name in tools::inherited_claude_markers() {
            cmd.env_remove(name);
        }
        for (key, value) in env.get_envs() {
            if let Some(value) = value {
                cmd.env(key, value);
            } else {
                cmd.env_remove(key);
            }
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("MCP_CONNECTION_NONBLOCKING", "0");
        // Compatible providers need concrete tool schemas instead of tool_reference blocks.
        if settings.mode != "local" {
            cmd.env("ENABLE_TOOL_SEARCH", "false");
        }
        let mut paths = vec![
            program.parent().unwrap().to_path_buf(),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ];
        if let Some(path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path));
        }
        cmd.env(
            "PATH",
            std::env::join_paths(paths).map_err(|e| e.to_string())?,
        );
        cmd.args([
            "--model",
            &settings.model,
            "--permission-mode",
            "default",
            "--verbose",
            "--append-system-prompt-file",
        ]);
        cmd.arg(dir.join("guidance.md"));
        cmd.arg("--mcp-config");
        cmd.arg(config);
        cmd.arg("--add-dir");
        cmd.arg(&w.root);
        if let Some(folder) = &w.assets_folder {
            cmd.arg("--add-dir");
            cmd.arg(folder);
        }
        cmd.arg(if resume { "--resume" } else { "--session-id" });
        cmd.arg(&sid);
        let session = spawn(cmd, cols, rows, channel)?;
        w.terminal_session = Some(AgentSession {
            id: sid,
            connection: signature,
        });
        studio.save(&w)?;
        sessions.insert(id, session);
        Ok(true)
    })
    .await
}
#[tauri::command]
pub fn terminal_write(app: tauri::AppHandle, id: String, data: String) -> Result<(), String> {
    if data.len() > 1_000_000 {
        return Err("Paste less than 1 MB at once".into());
    }
    let hub = app.state::<Terminals>();
    let mut sessions = hub.sessions.lock().unwrap();
    let session = sessions
        .get_mut(&id)
        .filter(|s| s.running.load(Ordering::SeqCst))
        .ok_or("Start Claude Code first")?;
    session
        .writer
        .write_all(data.as_bytes())
        .and_then(|_| session.writer.flush())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn terminal_resize(
    app: tauri::AppHandle,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let hub = app.state::<Terminals>();
    let sessions = hub.sessions.lock().unwrap();
    if let Some(s) = sessions.get(&id) {
        s.master
            .resize(size(cols, rows))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub fn terminal_close(app: tauri::AppHandle, id: String) {
    if let Some(session) = app
        .state::<Terminals>()
        .sessions
        .lock()
        .unwrap()
        .get_mut(&id)
    {
        session.stop();
    }
    if let Some(job) = app.state::<Studio>().jobs.lock().unwrap().get(&id) {
        job.cancel();
    }
}
#[tauri::command]
pub fn claude_installation(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let settings = app.state::<Studio>().data()?.settings;
    Ok(tools::resolve("claude", &settings.claude_path).map(|p| p.to_string_lossy().into_owned()))
}

fn install_command() -> Result<std::process::Command, String> {
    #[cfg(unix)]
    let mut cmd = {
        let mut cmd = tools::command(&tools::required("bash", "")?);
        // The vendor's native installer verifies its downloaded binary's checksum.
        cmd.args(["-o", "pipefail", "-c", "curl -fsSL --connect-timeout 20 --max-time 120 https://claude.ai/install.sh | bash -s -- latest"]);
        cmd
    };
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = tools::command(Path::new("powershell.exe"));
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$ErrorActionPreference = 'Stop'; irm https://claude.ai/install.ps1 | iex",
        ]);
        cmd
    };
    tools::clean_provider(&mut cmd);
    for name in [
        "MIMO_API_KEY",
        "AGENT_API_KEY",
        "VOLCENGINE_API_KEY",
        "GOOGLE_CLOUD_API_KEY",
    ] {
        cmd.env_remove(name);
    }
    Ok(cmd)
}

#[tauri::command]
pub async fn install_claude(
    app: tauri::AppHandle,
    progress: Channel<String>,
) -> Result<String, String> {
    crate::work(move || {
        let hub = app.state::<Terminals>();
        if hub.updating.swap(true, Ordering::SeqCst) {
            return Err("Claude Code installation or update is already running.".into());
        }
        let result = (|| {
            if hub.running() || !app.state::<Studio>().jobs.lock().unwrap().is_empty() {
                return Err("Stop Claude Code and wait for media tasks before installing.".into());
            }
            let studio = app.state::<Studio>();
            let settings = studio.data()?.settings;
            // Reuse an existing installation, including when a configured path is stale.
            let existing = tools::resolve("claude", &settings.claude_path)
                .or_else(|| tools::resolve("claude", ""));
            let program = if let Some(program) = existing {
                let _ = progress.send("Found an existing Claude Code installation. Verifying…".into());
                program
            } else {
                let _ = progress.send("Downloading Claude Code from the official installer. This may take a few minutes…".into());
                let (ok, out, err) = tools::execute(install_command()?, &tools::Job::default(), 600, |line| {
                    let _ = progress.send(tools::redact(line, None));
                })?;
                if !ok {
                    return Err(tools::redact(&format!("Installation failed. Check your connection and try again.\n{out}\n{err}"), None));
                }
                tools::required("claude", "")?
            };
            let mut cmd = tools::command(&program);
            tools::clean_provider(&mut cmd);
            cmd.arg("--version");
            let (ok, version, err) = tools::execute(cmd, &tools::Job::default(), 15, |_| {})?;
            if !ok || !version.contains("Claude Code") {
                return Err(tools::redact(&format!("Installation could not be verified. Check the executable path in Settings.\n{err}"), None));
            }
            let _guard = studio.gate.lock().unwrap();
            let mut data = studio.data()?;
            data.settings.claude_path = program.to_string_lossy().into_owned();
            studio.save_data(&data)?;
            Ok(format!("{} is ready. Choose your connection and start the terminal.", version.trim()))
        })();
        hub.updating.store(false, Ordering::SeqCst);
        result
    }).await
}

#[tauri::command]
pub async fn update_claude(app: tauri::AppHandle) -> Result<String, String> {
    crate::work(move || {
        let hub = app.state::<Terminals>();
        if hub.updating.swap(true, Ordering::SeqCst) {
            return Err("An update is already running".into());
        }
        let result = (|| {
            if hub.running() || !app.state::<Studio>().jobs.lock().unwrap().is_empty() {
                return Err("Stop Claude Code and wait for media tasks before updating.".into());
            }
            let settings = app.state::<Studio>().data()?.settings;
            let program = tools::required("claude", &settings.claude_path)?;
            let cmd = tools::cli_update_command(&program)?;
            let (ok, out, err) = tools::execute(cmd, &tools::Job::default(), 300, |_| {})?;
            if !ok {
                return Err(tools::redact(&format!("{out}\n{err}"), None));
            }
            let mut version = tools::command(&program);
            tools::clean_provider(&mut version);
            version.arg("--version");
            let (ok, current, _) = tools::execute(version, &tools::Job::default(), 10, |_| {})?;
            if !ok {
                return Err(
                    "Update finished, but this CLI could not start. Check its path in Settings."
                        .into(),
                );
            }
            Ok(
                format!("{}\n{}\n{}", current.trim(), out.trim(), err.trim())
                    .trim()
                    .to_owned(),
            )
        })();
        hub.updating.store(false, Ordering::SeqCst);
        result
    })
    .await
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    use tauri::ipc::InvokeResponseBody;

    fn channel(tx: mpsc::Sender<serde_json::Value>) -> Channel<TerminalEvent> {
        Channel::new(move |body| {
            if let InvokeResponseBody::Json(s) = body {
                let _ = tx.send(serde_json::from_str(&s).unwrap());
            }
            Ok(())
        })
    }
    fn until(rx: &mpsc::Receiver<serde_json::Value>, bytes: &mut Vec<u8>, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !String::from_utf8_lossy(bytes).contains(expected) {
            let event = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("PTY output timed out");
            bytes.extend(
                event["bytes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|b| b.as_u64().unwrap() as u8),
            );
        }
    }
    #[test]
    fn interactive_pty_preserves_unicode_resizes_replays_and_stops() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", "printf '\\033[31mREADY\\033[0m\\n'; stty size; IFS= read -r line; printf 'ACK:%s\\n' \"$line\"; IFS= read -r line; stty size; sleep 60"]);
        let (tx, rx) = mpsc::channel();
        let mut session = spawn(cmd, 80, 22, channel(tx)).unwrap();
        let mut bytes = vec![];
        until(&rx, &mut bytes, "22 80");
        session.writer.write_all("你好，CLI\n".as_bytes()).unwrap();
        until(&rx, &mut bytes, "ACK:你好，CLI");
        session.master.resize(size(72, 14)).unwrap();
        session.writer.write_all(b"\n").unwrap();
        until(&rx, &mut bytes, "14 72");
        let (tx, rx2) = mpsc::channel();
        attach(&session, channel(tx));
        let mut replay = vec![];
        until(&rx2, &mut replay, "ACK:你好，CLI");
        assert!(String::from_utf8_lossy(&replay).contains("\x1b[31mREADY"));
        session.stop();
        loop {
            let event = rx2
                .recv_timeout(Duration::from_secs(5))
                .expect("Stopped PTY must exit");
            if event["type"] == "exit" {
                break;
            }
        }
        assert!(!session.running.load(Ordering::SeqCst));
    }
}
