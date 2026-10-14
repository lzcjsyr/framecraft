#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod credentials;
mod generation;
mod media;
mod model;
mod pipeline;
mod store;
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
    if let Some(raw) = &w.raw_folder {
        scope
            .allow_directory(raw, true)
            .map_err(|e| e.to_string())?;
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
    folder: String,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
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
            if let Some(raw) = w.raw_folder.as_mut() {
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
async fn import_raw_folder(
    app: tauri::AppHandle,
    id: String,
    folder: String,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        s.idle(&id)?;
        let mut w = s.load(&id)?;
        let folder = PathBuf::from(folder)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !folder.is_dir() {
            return Err("Choose a raw-material folder".into());
        }
        let mut items = media::scan(&folder)?;
        if items.is_empty() {
            return Err("The selected folder contains no supported media".into());
        }
        for m in &mut items {
            if let Some(old) = w.media.iter().find(|old| {
                old.path == m.path && old.bytes == m.bytes && old.modified == m.modified
            }) {
                m.id = old.id.clone();
                if m.transcript.is_none() {
                    m.transcript = old.transcript.clone()
                }
            }
        }
        items.extend(w.media.iter().filter(|m| m.role == "generated").cloned());
        w.media = items;
        w.raw_folder = Some(folder.to_string_lossy().into());
        w.revision += 1;
        w.updated = now();
        s.save(&w)?;
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
        if !["local", "api"].contains(&settings.mode.as_str())
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
async fn unlock_provider_keys() -> Result<std::collections::BTreeMap<String, bool>, String> {
    work(credentials::retry).await
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
            return Err("Choose voice or image and enter 1–8,000 characters".into());
        }
        let studio = app.state::<Studio>();
        let _guard = studio.gate.lock().unwrap();
        studio.idle(&id)?;
        let mut w = studio.load(&id)?;
        let settings = studio.data()?.settings.generation;
        generation::validate(&settings)?;
        if kind == "voice" && settings.voice.trim().is_empty() {
            return Err(
                "Configure your voice ID in Connections before generating narration".into(),
            );
        }
        let secret = generation::secret(&settings, &kind)?;
        let run = Run {
            id: model::id(),
            stage: kind,
            status: "running".into(),
            started: now(),
            finished: None,
            message: "Generating media".into(),
            logs: vec![],
            artifact_id: None,
            input_revision: w.revision,
        };
        store::write(
            &PathBuf::from(&w.root)
                .join(".framecraft/runs")
                .join(&run.id)
                .join("generation.json"),
            &serde_json::json!({"kind":run.stage,"prompt":prompt.trim(),"settings":settings}),
        )?;
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
            secret,
        );
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
    stage: String,
    artifact_id: Option<String>,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        s.idle(&id)?;
        let mut w = s.load(&id)?;
        let settings = s.data()?.settings;
        let mut plan = None;
        match stage.as_str() {
            "analyze" => {
                if w.reference.is_none() { return Err("Choose a reference video first".into()); }
            }
            "adapt" | "plan" => {
                pipeline::analysis(&w)?;
                if w.brief.topic.trim().is_empty() { return Err("Enter your topic first".into()); }
                if stage == "plan" && w.media.iter().all(|m| m.kind != "video") {
                    return Err("Choose a raw-material folder containing videos".into());
                }
            }
            "render" => {
                let a = w.artifacts.iter().find(|a| Some(&a.id) == artifact_id.as_ref() && a.kind == "plan")
                    .ok_or("Select an edit plan to render")?;
                if a.data["_inputRevision"].as_u64() != Some(w.revision) {
                    return Err("Inputs changed since this plan was made. Generate a new plan before rendering.".into());
                }
                let p: EditPlan = serde_json::from_value(a.data.clone()).map_err(|e| e.to_string())?;
                media::validate_plan(&p, &w)?;
                plan = Some(a.clone());
            }
            _ => return Err("Unknown stage".into()),
        }
        if stage != "render" {
            tools::required("claude", &settings.claude_path)?;
            if settings.mode == "local" { tools::require_local_login(&settings.claude_path)?; }
            if settings.mode == "api" && tools::key()?.is_none() {
                return Err("Save your provider key in Settings first".into());
            }
        }
        if stage != "adapt" {
            tools::required("ffmpeg", &settings.ffmpeg_path)?;
            tools::required("ffprobe", &settings.ffprobe_path)?;
        }
        let run = Run {
            id: model::id(), stage, status: "running".into(), started: now(),
            finished: None, message: "Starting".into(), logs: vec![],
            artifact_id: None, input_revision: w.revision,
        };
        w.runs.push(run.clone());
        w.updated = now();
        s.save(&w)?;
        let job = Arc::new(tools::Job::default());
        s.jobs.lock().unwrap().insert(id, job.clone());
        pipeline::launch(app.clone(), w.clone(), run, settings, job, plan);
        Ok(w)
    }).await
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
async fn save_plan(
    app: tauri::AppHandle,
    id: String,
    artifact_id: String,
    plan: EditPlan,
) -> Result<Workspace, String> {
    work(move || {
        let s = app.state::<Studio>();
        let _g = s.gate.lock().unwrap();
        s.idle(&id)?;
        let mut w = s.load(&id)?;
        media::validate_plan(&plan, &w)?;
        let old = w
            .artifacts
            .iter()
            .find(|a| a.id == artifact_id && a.kind == "plan")
            .ok_or("Edit plan not found")?;
        if old.data["_inputRevision"].as_u64() != Some(w.revision) {
            return Err("This plan is stale. Generate a new plan first.".into());
        }
        let a_id = model::id();
        let mut data = serde_json::to_value(&plan).map_err(|e| e.to_string())?;
        data["_inputRevision"] = serde_json::json!(w.revision);
        data["_referenceId"] = serde_json::json!(w.reference.as_ref().map(|r| &r.id));
        let path = PathBuf::from(&w.root)
            .join("edits")
            .join(format!("{a_id}.json"));
        write(&path, &data)?;
        w.artifacts.push(Artifact {
            id: a_id,
            kind: "plan".into(),
            title: format!("{} · revised", plan.title),
            path: path.to_string_lossy().into(),
            created: now(),
            run_id: old.run_id.clone(),
            data,
        });
        w.updated = now();
        s.save(&w)?;
        Ok(w)
    })
    .await
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
            || w.raw_folder.as_ref().is_some_and(|raw| {
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_data,
            create_workspace,
            open_workspace,
            get_workspace,
            save_brief,
            import_reference,
            import_raw_folder,
            attach_transcript,
            import_skill,
            save_settings,
            diagnostics,
            cli_status,
            adc_status,
            provider_status,
            unlock_provider_keys,
            import_provider_keys,
            generate_asset,
            start_run,
            cancel_run,
            save_plan,
            reveal_path
        ])
        .build(tauri::generate_context!())
        .expect("Cannot start Framecraft")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let s = app.state::<Studio>();
                for job in s.jobs.lock().unwrap().values() {
                    job.cancelled.store(true, Ordering::SeqCst);
                    job.cancel();
                }
            }
        });
}
