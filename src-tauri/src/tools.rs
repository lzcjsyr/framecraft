use crate::model::*;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub const PROVIDER_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_OAUTH_TOKEN",
];
#[derive(Default)]
pub struct Job {
    pub cancelled: AtomicBool,
    pub pid: AtomicU32,
}
impl Job {
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err("Run cancelled".into())
        } else {
            Ok(())
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        kill_tree(self.pid.load(Ordering::SeqCst));
    }
}
pub fn kill_tree(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .status();
    }
}
#[cfg(windows)]
use std::os::windows::process::CommandExt;
pub fn command(p: &Path) -> Command {
    let mut c = Command::new(p);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    #[cfg(windows)]
    {
        c.creation_flags(0x08000000);
    }
    c
}
// Drains both pipes concurrently, so large tool output cannot deadlock before timeout/cancel.
pub fn execute(
    c: Command,
    job: &Job,
    timeout: u64,
    on_line: impl FnMut(&str),
) -> Result<(bool, String, String), String> {
    execute_with_input(c, job, timeout, None, on_line)
}
pub fn execute_with_input(
    mut c: Command,
    job: &Job,
    timeout: u64,
    input: Option<std::fs::File>,
    mut on_line: impl FnMut(&str),
) -> Result<(bool, String, String), String> {
    job.check()?;
    c.stdin(input.map(Stdio::from).unwrap_or_else(Stdio::null))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| format!("Cannot start tool: {e}"))?;
    job.pid.store(child.id(), Ordering::SeqCst);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let t = tx.clone();
    std::thread::spawn(move || {
        for l in BufReader::new(stdout).lines() {
            if let Ok(l) = l {
                if t.send((false, l)).is_err() {
                    break;
                }
            } else {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        for l in BufReader::new(stderr).lines() {
            if let Ok(l) = l {
                if tx.send((true, l)).is_err() {
                    break;
                }
            } else {
                break;
            }
        }
    });
    let mut out = String::new();
    let mut err = String::new();
    let start = Instant::now();
    let mut disconnected = false;
    loop {
        while let Ok((is_err, line)) = rx.try_recv() {
            if is_err {
                if err.len() < 262144 {
                    err.push_str(&line);
                    err.push('\n');
                }
            } else {
                on_line(&line);
                if out.len() < 16_000_000 {
                    out.push_str(&line);
                    out.push('\n');
                }
            }
        }
        if job.cancelled.load(Ordering::SeqCst) || start.elapsed().as_secs() > timeout {
            kill_tree(child.id());
            let _ = child.kill();
            let _ = child.wait();
            job.pid.store(0, Ordering::SeqCst);
            return Err(if job.cancelled.load(Ordering::SeqCst) {
                "Run cancelled".into()
            } else {
                "Tool timed out".into()
            });
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            while !disconnected {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok((is_err, line)) => {
                        if is_err {
                            if err.len() < 262144 {
                                err.push_str(&line);
                                err.push('\n')
                            }
                        } else {
                            on_line(&line);
                            if out.len() < 16_000_000 {
                                out.push_str(&line);
                                out.push('\n')
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => disconnected = true,
                    Err(_) => {
                        if start.elapsed().as_secs() > timeout {
                            disconnected = true
                        }
                    }
                }
            }
            job.pid.store(0, Ordering::SeqCst);
            return Ok((status.success(), out, err));
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}
pub fn resolve(name: &str, configured: &str) -> Option<PathBuf> {
    if !configured.trim().is_empty() {
        let p = PathBuf::from(configured.trim());
        return if p.is_file() { Some(p) } else { None };
    }
    let mut paths = vec![];
    if let Some(env) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&env) {
            paths.push(dir.join(name));
            #[cfg(windows)]
            {
                paths.push(dir.join(format!("{name}.exe")));
            }
        }
    }
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        let home = PathBuf::from(home);
        for dir in [
            ".local/bin",
            ".claude/local",
            ".npm-global/bin",
            ".bun/bin",
            ".volta/bin",
        ] {
            paths.push(home.join(dir).join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            }));
        }
    }
    #[cfg(unix)]
    {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
            paths.push(PathBuf::from(dir).join(name));
        }
    }
    paths.into_iter().find(|p| p.is_file())
}
pub fn required(name: &str, path: &str) -> Result<PathBuf, String> {
    resolve(name, path).ok_or_else(|| {
        format!("{name} was not found. Install it or set its executable path in Settings.")
    })
}
// Pin npm updates to the selected CLI's prefix, even when PATH has another Node.
pub fn cli_update_command(program: &Path) -> Result<Command, String> {
    let real = program.canonicalize().map_err(|e| e.to_string())?;
    for package in real.ancestors() {
        if package.file_name().is_some_and(|n| n == "claude-code")
            && package
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "@anthropic-ai")
        {
            let modules = package
                .parent()
                .and_then(Path::parent)
                .ok_or("Invalid npm installation")?;
            let mut prefix = modules.parent().ok_or("Invalid npm prefix")?;
            if prefix.file_name().is_some_and(|n| n == "lib") {
                prefix = prefix.parent().ok_or("Invalid npm prefix")?;
            }
            let mut cmd = command(&required("npm", "")?);
            cmd.args([
                "install",
                "--global",
                "@anthropic-ai/claude-code@latest",
                "--prefix",
            ])
            .arg(prefix);
            clean_provider(&mut cmd);
            return Ok(cmd);
        }
    }
    let parts: Vec<_> = real.iter().collect();
    if let Some(i) = parts.iter().position(|s| *s == "Caskroom") {
        if let Some(cask) = parts
            .get(i + 1)
            .filter(|n| **n == "claude-code" || **n == "claude-code@latest")
        {
            let mut cmd = command(&required("brew", "")?);
            cmd.args(["upgrade", "--cask"]).arg(cask);
            clean_provider(&mut cmd);
            return Ok(cmd);
        }
    }
    let mut cmd = command(program);
    clean_provider(&mut cmd);
    cmd.arg("update");
    Ok(cmd)
}
/// Session markers a parent Claude Code leaves in this app's environment, e.g. when
/// Framecraft is launched from a Claude Code terminal. A child CLI that inherits them
/// treats itself as a subagent session and disables transcript saving.
pub fn inherited_claude_markers() -> Vec<std::ffi::OsString> {
    std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| {
            name.to_str()
                .is_some_and(|n| n == "CLAUDECODE" || n.starts_with("CLAUDE_CODE_"))
        })
        .collect()
}
pub fn clean_provider(c: &mut Command) {
    for name in PROVIDER_ENV {
        c.env_remove(name);
    }
    for name in inherited_claude_markers() {
        c.env_remove(name);
    }
}
/// Builds a process-scoped provider environment; credentials never enter arguments.
pub fn agent_command(
    program: &Path,
    settings: &Settings,
    secret: Option<&str>,
) -> Result<Command, String> {
    let mut cmd = command(program);
    clean_provider(&mut cmd);
    if settings.mode == "api" || settings.mode == "mimo" {
        let key = secret.ok_or("Save your provider API key in Settings first")?;
        cmd.env("ANTHROPIC_BASE_URL", &settings.base_url);
        if settings.mode == "mimo" {
            cmd.env("ANTHROPIC_AUTH_TOKEN", key)
                .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
        } else {
            cmd.env("ANTHROPIC_API_KEY", key);
        }
        for name in [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
        ] {
            cmd.env(name, &settings.model);
        }
    }
    // Media credentials are consumed by the native app, never by agent tools.
    for name in [
        "MIMO_API_KEY",
        "AGENT_API_KEY",
        "VOLCENGINE_API_KEY",
        "GOOGLE_CLOUD_API_KEY",
        "GOOGLE_APPLICATION_CREDENTIALS",
    ] {
        cmd.env_remove(name);
    }
    Ok(cmd)
}

pub fn key() -> Result<Option<String>, String> {
    crate::credentials::optional("AGENT_API_KEY")
}
pub fn save_key(value: &str) -> Result<(), String> {
    if value.len() > 2400
        || value.is_empty()
        || !value.is_ascii()
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("Paste an API key without spaces (up to 2,400 characters).".into());
    }
    crate::credentials::save(std::collections::BTreeMap::from([(
        "AGENT_API_KEY".into(),
        value.into(),
    )]))?;
    Ok(())
}
pub fn redact(text: &str, secret: Option<&str>) -> String {
    let mut text = text.to_owned();
    if let Some(k) = secret.filter(|k| !k.is_empty()) {
        text = text.replace(k, "[redacted]");
    }
    text.chars().take(4000).collect()
}
fn apply_cli_auth(status: &mut CliStatus, success: bool, output: &str) {
    match serde_json::from_str::<Value>(output) {
        Ok(v) => {
            status.auth_method = v["authMethod"].as_str().map(str::to_owned);
            status.account = v["email"].as_str().map(str::to_owned);
            let account_auth = matches!(
                status.auth_method.as_deref(),
                Some("claude.ai" | "oauth_token")
            );
            status.logged_in = success && v["loggedIn"].as_bool() == Some(true) && account_auth;
            if !status.logged_in {
                status.error = Some(if v["loggedIn"].as_bool() == Some(true) && !account_auth {
                    "Local mode needs a Claude account login. Run `claude auth login`, or choose Third-party API mode.".into()
                } else {
                    "Claude Code is not signed in. Run `claude auth login` in Terminal, then verify again.".into()
                });
            }
        }
        Err(_) => {
            status.error =
                Some("CLI login could not be verified. Update Claude Code and try again.".into())
        }
    }
}
pub fn inspect_cli(path: &str) -> CliStatus {
    let program = resolve("claude", path);
    let mut status = CliStatus {
        path: program.as_ref().map(|p| p.to_string_lossy().into()),
        version: None,
        logged_in: false,
        auth_method: None,
        account: None,
        checked: now(),
        error: None,
    };
    let Some(program) = program else {
        status.error =
            Some("Claude Code was not found. Install it or set its executable path below.".into());
        return status;
    };
    let mut version = command(&program);
    clean_provider(&mut version);
    version.args(["--setting-sources", "", "--version"]);
    match execute(version, &Job::default(), 10, |_| {}) {
        Ok((true, output, _)) => status.version = Some(output.trim().to_owned()),
        _ => {
            status.error = Some("Claude Code could not start. Check its executable path.".into());
            return status;
        }
    }
    let mut auth = command(&program);
    clean_provider(&mut auth);
    auth.args(["--setting-sources", "", "auth", "status", "--json"]);
    match execute(auth, &Job::default(), 10, |_| {}) {
        Ok((success, output, _)) => apply_cli_auth(&mut status, success, &output),
        Err(error) => status.error = Some(format!("CLI login verification failed: {error}")),
    }
    status.checked = now();
    status
}
pub fn require_local_login(path: &str) -> Result<(), String> {
    let status = inspect_cli(path);
    if status.logged_in {
        Ok(())
    } else {
        Err(status
            .error
            .unwrap_or_else(|| "Verify your Claude Code login in Agent settings.".into()))
    }
}
pub fn diagnose(s: &Settings) -> Diagnostics {
    let path =
        |name, configured: &str| resolve(name, configured).map(|p| p.to_string_lossy().into());
    let mut d = Diagnostics {
        claude: path("claude", &s.claude_path),
        ffmpeg: path("ffmpeg", &s.ffmpeg_path),
        ffprobe: path("ffprobe", &s.ffprobe_path),
        whisper: path("whisper", &s.whisper_path),
        key_saved: false,
        errors: vec![],
    };
    match if s.mode == "mimo" {
        crate::credentials::optional("MIMO_API_KEY")
    } else {
        key()
    } {
        Ok(k) => d.key_saved = k.is_some(),
        Err(e) => d.errors.push(e),
    }
    d
}
pub fn small_text(p: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > 1_048_576 {
        return Err("Text file exceeds 1 MB".into());
    }
    let mut s = String::new();
    std::fs::File::open(p)
        .map_err(|e| e.to_string())?
        .read_to_string(&mut s)
        .map_err(|e| format!("Use a UTF-8 text file: {e}"))?;
    Ok(s)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(unix)]
    fn context_reaches_the_child_process_on_stdin_without_unicode_loss() {
        let path = std::env::temp_dir().join(format!("framecraft-stdin-{}", id()));
        let context = "Assets: /项目/素材\nGoal: 把结尾缩短。\n";
        std::fs::write(&path, context).unwrap();
        let (ok, output, _) = execute_with_input(
            command(Path::new("/bin/cat")),
            &Job::default(),
            5,
            Some(std::fs::File::open(&path).unwrap()),
            |_| {},
        )
        .unwrap();
        assert!(ok);
        assert_eq!(output, context);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn cli_login_fails_closed_for_signed_out_api_auth_and_invalid_responses() {
        let mut status = CliStatus {
            path: None,
            version: None,
            logged_in: false,
            auth_method: None,
            account: None,
            checked: 0,
            error: None,
        };
        for (success, output) in [
            (false, r#"{"loggedIn":false,"authMethod":"none"}"#),
            (true, r#"{"loggedIn":true,"authMethod":"api_key"}"#),
            (false, r#"{"loggedIn":true,"authMethod":"claude.ai"}"#),
            (true, "invalid JSON"),
        ] {
            status.logged_in = false;
            apply_cli_auth(&mut status, success, output);
            assert!(!status.logged_in);
            assert!(status.error.is_some());
        }
        status.error = None;
        apply_cli_auth(
            &mut status,
            true,
            r#"{"loggedIn":true,"authMethod":"claude.ai","email":"test@example.invalid"}"#,
        );
        assert!(status.logged_in);
        assert_eq!(status.account.as_deref(), Some("test@example.invalid"));
        assert!(status.error.is_none());
    }
    #[test]
    fn subscription_clears_every_provider_override() {
        let mut c = command(Path::new("claude"));
        for k in PROVIDER_ENV {
            c.env(k, "unexpected");
        }
        clean_provider(&mut c);
        for (_, v) in c.get_envs() {
            assert!(v.is_none())
        }
    }
    #[test]
    fn api_credentials_are_process_scoped_and_not_arguments() {
        let settings = Settings {
            mode: "api".into(),
            base_url: "https://provider.example".into(),
            ..Default::default()
        };
        let c = agent_command(Path::new("claude"), &settings, Some("test-key")).unwrap();
        let env: std::collections::HashMap<_, _> = c.get_envs().collect();
        assert_eq!(
            env[std::ffi::OsStr::new("ANTHROPIC_API_KEY")],
            Some(std::ffi::OsStr::new("test-key"))
        );
        assert_eq!(env[std::ffi::OsStr::new("ANTHROPIC_AUTH_TOKEN")], None);
        let mimo =
            agent_command(Path::new("claude"), &Settings::default(), Some("test-key")).unwrap();
        let mimo_env: std::collections::HashMap<_, _> = mimo.get_envs().collect();
        assert_eq!(
            mimo_env[std::ffi::OsStr::new("ANTHROPIC_AUTH_TOKEN")],
            Some(std::ffi::OsStr::new("test-key"))
        );
        assert_eq!(mimo_env[std::ffi::OsStr::new("ANTHROPIC_API_KEY")], None);
        assert_eq!(
            env[std::ffi::OsStr::new("ANTHROPIC_BASE_URL")],
            Some(std::ffi::OsStr::new("https://provider.example"))
        );
        assert!(c
            .get_args()
            .all(|arg| !arg.to_string_lossy().contains("test-key")));
        assert!(agent_command(Path::new("claude"), &settings, None).is_err());
        let local = agent_command(
            Path::new("claude"),
            &Settings {
                mode: "local".into(),
                ..Default::default()
            },
            Some("ignored-key"),
        )
        .unwrap();
        assert!(local.get_envs().all(|(_, value)| value.is_none()));
    }
    #[test]
    fn cancellation_stops_a_child_process() {
        #[cfg(unix)]
        {
            let job = std::sync::Arc::new(Job::default());
            let cancel = job.clone();
            let thread = std::thread::spawn(move || {
                let mut c = command(Path::new("/bin/sh"));
                c.args(["-c", "sleep 30"]);
                execute(c, &job, 60, |_| {})
            });
            for _ in 0..100 {
                if cancel.pid.load(Ordering::SeqCst) != 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            cancel.cancel();
            let start = Instant::now();
            assert!(thread.join().unwrap().is_err());
            assert!(start.elapsed().as_secs() < 3);
        }
    }
    #[test]
    fn redacts_credentials() {
        assert_eq!(
            redact("bad token very-secret", Some("very-secret")),
            "bad token [redacted]"
        );
    }
    #[test]
    fn drains_large_stderr() {
        #[cfg(unix)]
        {
            let mut c = command(Path::new("/bin/sh"));
            c.args([
                "-c",
                "i=0; while [ $i -lt 5000 ]; do echo stderr >&2; i=$((i+1)); done; echo done",
            ]);
            let (ok, out, err) = execute(c, &Job::default(), 5, |_| {}).unwrap();
            assert!(ok);
            assert!(out.contains("done"));
            assert!(err.len() > 30000);
        }
    }
}
