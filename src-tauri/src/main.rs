#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod bridge;
mod credentials;
mod generation;
mod local_env;
mod media;
mod mimo;
mod model;
mod pipeline;
mod store;
mod terminal;
mod tools;
use model::*;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
};
use store::*;
use tauri::Manager;
async fn work<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}
fn grant(app: &tauri::AppHandle, w: &Workspace) -> Result<(), String> {
    let scope = app.asset_protocol_scope();
    scope
        .allow_directory(&w.root, true)
        .map_err(|e| e.to_string())?;
    if let Some(r) = &w.reference {
        scope.allow_file(&r.path).map_err(|e| e.to_string())?;
    }
    if let Some(raw) = &w.assets_folder {
        scope
            .allow_directory(raw, true)
            .map_err(|e| e.to_string())?;
    }
    for media in &w.media {
        scope.allow_file(&media.path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
async fn app_data(app: tauri::AppHandle) -> Result<AppData, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        s.data()
    })
    .await
}
#[tauri::command]
async fn create_workspace(
    app: tauri::AppHandle,
    name: String,
    folder: Option<String>,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        let folder = match folder {
            Some(folder) => folder,
            None => {
                let root = s.storage.join("workspaces").join(model::id());
                std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                root.to_string_lossy().into()
            }
        };
        let w = create(name, folder)?;
        let root = PathBuf::from(&w.root);
        let w = s.register(w, &root)?;
        grant(&app, &w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn open_workspace(app: tauri::AppHandle, folder: String) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        let root = PathBuf::from(folder)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !root.join("workspace.json").is_file() {
            return Err("This folder is not a Framecraft workspace. Choose Create new to start a workspace here.".into());
        }
        let w: Workspace = read(&root.join("workspace.json"))?;
        if w.schema_version != 1 {
            return Err("Unsupported workspace version".into());
        }
        s.idle(&w.id)?;
        let mut w = w; // Rebase generated paths when a workspace folder is moved.
        let old = PathBuf::from(&w.root);
        if old != root {
            let rebase = |p: &mut String| {
                if let Ok(relative) = Path::new(p).strip_prefix(&old) {
                    *p = root.join(relative).to_string_lossy().into();
                }
            };
            if let Some(m) = w.reference.as_mut() {
                rebase(&mut m.path);
                for f in &mut m.frames {
                    rebase(&mut f.path)
                }
            }
            if let Some(raw) = w.assets_folder.as_mut() {
                rebase(raw)
            }
            for m in &mut w.media {
                rebase(&mut m.path);
                for f in &mut m.frames {
                    rebase(&mut f.path)
                }
            }
            for a in &mut w.artifacts {
                rebase(&mut a.path);
                if a.kind == "export" {
                    for key in ["video", "captions"] {
                        if let Some(p) = a.data[key].as_str() {
                            let mut p = p.to_owned();
                            rebase(&mut p);
                            a.data[key] = Value::String(p);
                        }
                    }
                }
            }
        }
        let w = s.register(w, &root)?;
        grant(&app, &w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn get_workspace(app: tauri::AppHandle, id: String) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        let mut w = s.load(&id)?;
        if !s.jobs.lock().unwrap().contains_key(&id) {
            let mut changed = false;
            for run in &mut w.runs {
                if run.status == "running" || run.status == "queued" {
                    run.status = "interrupted".into();
                    run.message = "App closed during this run. Start the stage again.".into();
                    run.finished = Some(now());
                    changed = true;
                }
            }
            if changed {
                s.save(&w)?;
            }
        }
        grant(&app, &w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn save_brief(
    app: tauri::AppHandle,
    id: String,
    brief: Brief,
    selected_skills: Vec<String>,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        s.idle(&id)?;
        let mut w = s.load(&id)?;
        if brief.topic.len() > 4000 || brief.direction.len() > 8000
            || brief.audience.len() > 1000 || brief.language.len() > 100
            || brief.platform.len() > 200 || !(5..=600).contains(&brief.duration)
            || !["9:16", "16:9", "1:1"].contains(&brief.aspect.as_str()) {
            return Err("Check the brief: target duration is 5–600 seconds and aspect is 9:16, 16:9, or 1:1.".into());
        }
        if selected_skills.iter().any(|id| !w.skills.iter().any(|sk| &sk.id == id)) {
            return Err("Unknown skill".into());
        }
        w.brief = brief;
        w.selected_skills = selected_skills;
        w.revision += 1;
        w.updated = now();
        s.save(&w)?;
        Ok(w)
    }).await
}
#[tauri::command]
async fn import_reference(
    app: tauri::AppHandle,
    id: String,
    path: String,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        s.idle(&id)?;
        let mut w = s.load(&id)?;
        let m = media::inventory(Path::new(&path), "reference")?;
        if m.kind != "video" {
            return Err("The reference must be a video".into());
        }
        w.reference = Some(m);
        w.revision += 1;
        w.updated = now();
        s.save(&w)?;
        grant(&app, &w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn set_assets_folder(
    app: tauri::AppHandle,
    id: Option<String>,
    folder: String,
) -> Result<Workspace, String> {
    work(move || {
        let folder = media::folder_path(Path::new(&folder))?;
        let studio = app.state::<Studio>();
        let _guard = studio.gate.lock().unwrap();
        let mut w = if let Some(id) = id {
            studio.idle(&id)?;
            studio.load(&id)?
        } else {
            let root = studio.storage.join("workspaces").join(model::id());
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let name = folder
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .take(30)
                .collect::<String>();
            let w = create(
                if name.is_empty() {
                    "My video".into()
                } else {
                    name
                },
                root.to_string_lossy().into(),
            )?;
            studio.register(w, &root)?
        };
        w.assets_folder = Some(folder.to_string_lossy().into());
        w.media
            .retain(|m| m.role == "generated" || Path::new(&m.path).starts_with(&folder));
        w.revision += 1;
        w.updated = now();
        studio.save(&w)?;
        grant(&app, &w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn attach_transcript(
    app: tauri::AppHandle,
    id: String,
    media_id: String,
    path: String,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        s.idle(&id)?;
        let mut w = s.load(&id)?;
        let text = tools::small_text(Path::new(&path))?;
        if text.trim().is_empty() {
            return Err("Transcript is empty".into());
        }
        if let Some(m) = w.reference.as_mut().filter(|m| m.id == media_id) {
            m.transcript = Some(text);
            m.warning = None;
            m.id = model::id();
        } else if let Some(m) = w.media.iter_mut().find(|m| m.id == media_id) {
            m.transcript = Some(text);
            m.warning = None;
        } else {
            return Err("Unknown media".into());
        }
        w.revision += 1;
        w.updated = now();
        s.save(&w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn import_skill(
    app: tauri::AppHandle,
    id: String,
    path: String,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        s.idle(&id)?;
        let mut w = s.load(&id)?;
        let contents = tools::small_text(Path::new(&path))?;
        if contents.trim().is_empty() || contents.len() > 32768 {
            return Err("A skill must contain 1–32,768 bytes of instructions".into());
        }
        let sk_id = model::id();
        let name = contents
            .lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches("# ").to_owned())
            .unwrap_or_else(|| {
                Path::new(&path)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            });
        let dest = PathBuf::from(&w.root)
            .join(".framecraft/skills")
            .join(&sk_id);
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        std::fs::write(dest.join("SKILL.md"), &contents).map_err(|e| e.to_string())?;
        w.skills.push(Skill {
            id: sk_id.clone(),
            name,
            description: "Imported workspace skill".into(),
            instructions: contents,
            origin: "Workspace".into(),
        });
        w.selected_skills.push(sk_id);
        w.revision += 1;
        w.updated = now();
        s.save(&w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    settings: Settings,
    api_key: Option<String>,
    provider_keys: Option<std::collections::BTreeMap<String, String>>,
) -> Result<Settings, String> {
    work(move || {
        if !["local", "api", "mimo"].contains(&settings.mode.as_str())
            || settings.model.trim().is_empty()
            || settings.model.len() > 200
        {
            return Err("Choose an authentication mode and model".into());
        }
        let u =
            url::Url::parse(&settings.base_url).map_err(|_| "Enter a valid provider base URL")?;
        if u.scheme() != "https"
            && !(u.scheme() == "http"
                && ["localhost", "127.0.0.1", "[::1]"].contains(&u.host_str().unwrap_or_default()))
        {
            return Err("Use HTTPS for a remote provider or HTTP for localhost".into());
        }
        if !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
        {
            return Err(
                "The provider URL must not contain credentials, query parameters, or fragments"
                    .into(),
            );
        }
        generation::validate(&settings.generation)?;
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        if !s.jobs.lock().unwrap().is_empty() {
            return Err(
                "Wait for active workspace runs to finish before changing agent settings.".into(),
            );
        }
        if let Some(k) = api_key.filter(|k| !k.is_empty()) {
            tools::save_key(k.trim())?;
        }
        if let Some(keys) = provider_keys.filter(|k| !k.is_empty()) {
            credentials::save(keys)?;
        }
        let mut d = s.data()?;
        d.settings = settings.clone();
        s.save_data(&d)?;
        Ok(settings)
    })
    .await
}
#[tauri::command]
async fn provider_status() -> Result<std::collections::BTreeMap<String, bool>, String> {
    work(credentials::status).await
}
#[tauri::command]
async fn check_provider_keys(
    image_provider: String,
) -> Result<std::collections::BTreeMap<String, bool>, String> {
    work(move || credentials::retry(&image_provider)).await
}
#[tauri::command]
async fn import_provider_keys() -> Result<std::collections::BTreeMap<String, bool>, String> {
    work(credentials::import_reference).await
}
#[tauri::command]
async fn generate_asset(
    app: tauri::AppHandle,
    id: String,
    kind: String,
    prompt: String,
) -> Result<Workspace, String> {
    work(move || {
        if !["voice", "image"].contains(&kind.as_str())
            || prompt.trim().is_empty()
            || prompt.chars().count() > 8000
        {
            return Err("Choose an image or voice and enter 1–8,000 characters".into());
        }
        let studio = app.state::<Studio>();
        let _guard = studio.gate.lock().unwrap();
        studio.idle(&id)?;
        let mut w = studio.load(&id)?;
        let settings = studio.data()?.settings;
        generation::validate(&settings.generation)?;
        let run = Run {
            id: model::id(),
            stage: kind,
            status: "running".into(),
            started: now(),
            finished: None,
            message: "Generating asset".into(),
            logs: vec![],
            artifact_id: None,
            input_revision: w.revision,
            base_artifact_id: None,
        };
        w.runs.push(run.clone());
        w.updated = now();
        studio.save(&w)?;
        let job = Arc::new(tools::Job::default());
        studio.jobs.lock().unwrap().insert(id, job.clone());
        generation::launch(
            app.clone(),
            w.clone(),
            run,
            settings,
            job,
            prompt.trim().into(),
        );
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn import_assets(
    app: tauri::AppHandle,
    id: String,
    paths: Vec<String>,
) -> Result<Workspace, String> {
    work(move || {
        let studio = app.state::<Studio>();
        let _guard = studio.gate.lock().unwrap();
        studio.idle(&id)?;
        let mut w = studio.load(&id)?;
        if paths.len() + w.media.len() > 100 {
            return Err("Use up to 100 assets per workspace".into());
        }
        let items = paths
            .iter()
            .map(|path| media::inventory(Path::new(path), "imported"))
            .collect::<Result<Vec<_>, _>>()?;
        for item in items {
            if !w.media.iter().any(|m| m.path == item.path) {
                w.media.push(item);
            }
        }
        w.revision += 1;
        w.updated = now();
        studio.save(&w)?;
        grant(&app, &w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn remove_input(
    app: tauri::AppHandle,
    id: String,
    media_id: String,
) -> Result<Workspace, String> {
    work(move || {
        let studio = app.state::<Studio>();
        let _guard = studio.gate.lock().unwrap();
        studio.idle(&id)?;
        let mut w = studio.load(&id)?;
        if w.reference.as_ref().is_some_and(|m| m.id == media_id) {
            w.reference = None;
        } else {
            w.media.retain(|m| m.id != media_id);
        }
        w.revision += 1;
        w.updated = now();
        studio.save(&w)?;
        Ok(w)
    })
    .await
}
#[tauri::command]
async fn adc_status(google_project: String) -> credentials::AdcStatus {
    credentials::inspect_adc(&google_project).await
}
#[tauri::command]
async fn cli_status(claude_path: String) -> Result<CliStatus, String> {
    work(move || Ok(tools::inspect_cli(&claude_path))).await
}
#[tauri::command]
async fn diagnostics(app: tauri::AppHandle) -> Result<Diagnostics, String> {
    work(move || {
        let s = app.state::<Studio>();
        let settings = {
            let _g = s.gate.lock().unwrap();
            s.data()?.settings
        };
        Ok(tools::diagnose(&settings))
    })
    .await
}
#[tauri::command]
async fn start_run(
    app: tauri::AppHandle,
    id: String,
    message: Option<String>,
    base_artifact_id: Option<String>,
) -> Result<Workspace, String> {
    work(move || {
        let studio = app.state::<Studio>();
        let _guard = studio.gate.lock().unwrap();
        studio.idle(&id)?;
        let mut w = studio.load(&id)?;
        let settings = studio.data()?.settings;
        let is_chat = message.is_some();
        let request = message.unwrap_or_else(|| w.brief.topic.clone());
        if request.trim().is_empty() || request.chars().count() > 8000 {
            return Err("Enter a message of 1–8,000 characters".into());
        }
        if !is_chat {
            media::folder_path(Path::new(
                w.assets_folder
                    .as_deref()
                    .ok_or("Choose your assets folder first")?,
            ))?;
        }
        if !is_chat && w.brief.topic.trim().is_empty() && w.brief.direction.trim().is_empty() {
            return Err("Describe the video you want to create".into());
        }
        if base_artifact_id.as_ref().is_some_and(|id| {
            !w.artifacts
                .iter()
                .any(|a| &a.id == id && a.kind == "export")
        }) {
            return Err("The selected video version is unavailable".into());
        }
        tools::required("claude", &settings.claude_path)?;
        tools::required("ffmpeg", &settings.ffmpeg_path)?;
        tools::required("ffprobe", &settings.ffprobe_path)?;
        if settings.mode == "mimo" {
            credentials::key("MIMO_API_KEY")?;
        } else if settings.mode == "local" {
            tools::require_local_login(&settings.claude_path)?;
        } else {
            tools::key()?.ok_or("Save your third-party API key first")?;
        }
        let run = Run {
            id: model::id(),
            stage: if is_chat { "chat" } else { "create" }.into(),
            status: "running".into(),
            started: now(),
            finished: None,
            message: "Starting Claude Code".into(),
            logs: vec![],
            artifact_id: None,
            input_revision: w.revision,
            base_artifact_id,
        };
        w.conversation.push(ChatMessage {
            id: model::id(),
            role: "user".into(),
            text: request.trim().into(),
            run_id: run.id.clone(),
            created: now(),
            artifact_id: None,
        });
        w.runs.push(run.clone());
        w.updated = now();
        studio.save(&w)?;
        let job = Arc::new(tools::Job::default());
        studio.jobs.lock().unwrap().insert(id, job.clone());
        pipeline::launch(app.clone(), w.clone(), run, settings, job);
        Ok(w)
    })
    .await
}
#[tauri::command]
fn cancel_run(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let s = app.state::<Studio>();
    let jobs = s.jobs.lock().unwrap();
    if let Some(job) = jobs.get(&id) {
        job.cancel();
    }
    Ok(())
}
#[tauri::command]
async fn reveal_path(app: tauri::AppHandle, id: String, path: String) -> Result<(), String> {
    work(move || {
        let s = app.state::<Studio>();
        let w = {
            let _g = s.gate.lock().unwrap();
            s.load(&id)?
        };
        let p = PathBuf::from(path)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let root = s.root(&id)?;
        let allowed = p.starts_with(&root)
            || w.reference
                .as_ref()
                .is_some_and(|m| Path::new(&m.path).canonicalize().ok().as_ref() == Some(&p))
            || w.media
                .iter()
                .any(|m| Path::new(&m.path).canonicalize().ok().as_ref() == Some(&p))
            || w.assets_folder.as_ref().is_some_and(|raw| {
                Path::new(raw)
                    .canonicalize()
                    .ok()
                    .is_some_and(|raw| p.starts_with(raw))
            });
        if !allowed {
            return Err("Path does not belong to this workspace".into());
        }
        #[cfg(target_os = "macos")]
        {
            let mut c = tools::command(Path::new("/usr/bin/open"));
            if p.is_file() {
                c.arg("-R");
            }
            c.arg(&p);
            c.spawn().map_err(|e| e.to_string())?;
        }
        #[cfg(windows)]
        {
            let mut c = tools::command(Path::new("explorer.exe"));
            if p.is_file() {
                c.arg(format!("/select,{}", p.display()));
            } else {
                c.arg(&p);
            }
            c.spawn().map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--mcp-stdio") {
        let result = args
            .get(2)
            .zip(args.get(3))
            .ok_or("Missing standalone storage or workspace")
            .and_then(|(s, w)| bridge::stdio(s, w).map_err(|_| "Standalone MCP server failed"));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--mcp-proxy") {
        let result = args
            .get(2)
            .zip(args.get(3))
            .ok_or("Missing MCP bridge address or workspace")
            .and_then(|(a, w)| bridge::proxy(a, w).map_err(|_| "MCP bridge disconnected"));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let storage = app.path().app_data_dir()?;
            std::fs::create_dir_all(&storage)?;
            app.manage(Studio {
                storage,
                gate: std::sync::Mutex::new(()),
                jobs: std::sync::Mutex::new(std::collections::HashMap::new()),
            });
            app.manage(terminal::Terminals::default());
            app.manage(bridge::start(app.handle())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_data,
            create_workspace,
            open_workspace,
            get_workspace,
            save_brief,
            import_reference,
            set_assets_folder,
            attach_transcript,
            import_skill,
            save_settings,
            diagnostics,
            cli_status,
            adc_status,
            provider_status,
            check_provider_keys,
            import_provider_keys,
            generate_asset,
            start_run,
            cancel_run,
            import_assets,
            remove_input,
            reveal_path,
            terminal::terminal_open,
            terminal::terminal_attach,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
            terminal::update_claude,
            terminal::claude_installation,
            terminal::install_claude,
            bridge::terminal_context
        ])
        .build(tauri::generate_context!())
        .expect("Cannot start Framecraft")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<terminal::Terminals>().stop_all();
                let s = app.state::<Studio>();
                for job in s.jobs.lock().unwrap().values() {
                    job.cancelled.store(true, Ordering::SeqCst);
                    job.cancel();
                }
            }
        });
}
