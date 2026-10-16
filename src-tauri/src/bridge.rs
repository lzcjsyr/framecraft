//! The CLI speaks standard stdio MCP to a tiny proxy of this executable.
//! Work stays in the app process, sharing its workspace lock, credentials and preview events.
use crate::{
    generation, media,
    model::*,
    pipeline,
    store::{self, Studio},
    tools::{self, Job},
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};

pub struct Bridge {
    address: String,
    token: String,
    selected: Mutex<HashMap<String, String>>,
}
pub fn start(app: &tauri::AppHandle) -> Result<Bridge, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let address = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .to_string();
    let token = id();
    let secret = token.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        for socket in listener.incoming().flatten() {
            let app = app.clone();
            let secret = secret.clone();
            std::thread::spawn(move || {
                let _ = serve(app, socket, &secret);
            });
        }
    });
    Ok(Bridge {
        address,
        token,
        selected: Mutex::new(HashMap::new()),
    })
}
pub fn config(app: &tauri::AppHandle, workspace: &str) -> Result<PathBuf, String> {
    let b = app.state::<Bridge>();
    let path = app
        .state::<Studio>()
        .root(workspace)?
        .join(".framecraft/terminal/mcp.json");
    store::write(
        &path,
        &json!({"mcpServers":{"framecraft":{
        "type":"stdio", "alwaysLoad":true, "command":std::env::current_exe().map_err(|e|e.to_string())?,
            "args":["--mcp-proxy",b.address,workspace], "env":{"FRAMECRAFT_BRIDGE_TOKEN":b.token}
        }}}),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(path)
}
// This path runs before Tauri is initialized; stdout contains JSON-RPC only.
pub fn proxy(address: &str, workspace: &str) -> Result<(), String> {
    let mut socket = TcpStream::connect(address).map_err(|e| e.to_string())?;
    writeln!(socket, "{}", json!({"token":std::env::var("FRAMECRAFT_BRIDGE_TOKEN").unwrap_or_default(),"workspaceId":workspace})).map_err(|e|e.to_string())?;
    let mut read = BufReader::new(socket.try_clone().map_err(|e| e.to_string())?);
    std::thread::spawn(move || {
        let mut line = String::new();
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        while read.read_line(&mut line).unwrap_or(0) > 0 {
            if out
                .write_all(line.as_bytes())
                .and_then(|_| out.flush())
                .is_err()
            {
                break;
            }
            line.clear();
        }
    });
    for line in std::io::stdin().lock().lines() {
        writeln!(socket, "{}", line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        socket.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn send(socket: &Mutex<TcpStream>, value: Value) {
    if let Ok(mut s) = socket.lock() {
        let _ = writeln!(s, "{value}");
    }
}
fn serve(app: tauri::AppHandle, socket: TcpStream, token: &str) -> Result<(), String> {
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(socket.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let hello: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if hello["token"].as_str() != Some(token) {
        return Err("Unauthorized bridge".into());
    }
    let workspace = hello["workspaceId"]
        .as_str()
        .ok_or("Missing workspace")?
        .to_owned();
    app.state::<Studio>().root(&workspace)?;
    socket.set_read_timeout(None).map_err(|e| e.to_string())?;
    let writer = Arc::new(Mutex::new(socket));
    let pending: Arc<Mutex<HashMap<String, Arc<Job>>>> = Arc::default();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.len() > 8_000_000 {
            return Err("Request too large".into());
        }
        let Ok(r) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = r["method"].as_str().unwrap_or_default();
        if method == "notifications/cancelled" {
            if let Some(job) = pending
                .lock()
                .unwrap()
                .get(&r["params"]["requestId"].to_string())
            {
                job.cancel();
            }
            continue;
        }
        let Some(rid) = r.get("id").cloned() else {
            continue;
        };
        if method == "tools/call" {
            let app = app.clone();
            let workspace = workspace.clone();
            let writer = writer.clone();
            let pending = pending.clone();
            let job = Arc::new(Job::default());
            pending.lock().unwrap().insert(rid.to_string(), job.clone());
            std::thread::spawn(move || {
                let result = call_tool(&app, &workspace, &r["params"], job);
                let (is_error, text) = match result {
                    Ok(v) => (false, v.to_string()),
                    Err(e) => (true, e),
                };
                send(
                    &writer,
                    json!({"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":text}],"isError":is_error}}),
                );
                pending.lock().unwrap().remove(&rid.to_string());
            });
            continue;
        }
        let result = match method {
            "initialize" => {
                json!({"protocolVersion":r["params"]["protocolVersion"].as_str().unwrap_or("2024-11-05"),"capabilities":{"tools":{}},"serverInfo":{"name":"Framecraft","version":env!("CARGO_PKG_VERSION")}})
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools":tool_list()}),
            _ => {
                send(
                    &writer,
                    json!({"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":"Method not found"}}),
                );
                continue;
            }
        };
        send(&writer, json!({"jsonrpc":"2.0","id":rid,"result":result}));
    }
    for job in pending.lock().unwrap().values() {
        job.cancel();
    }
    Ok(())
}
#[tauri::command]
pub fn terminal_context(app: tauri::AppHandle, id: String, selected: String) {
    app.state::<Bridge>()
        .selected
        .lock()
        .unwrap()
        .insert(id, selected);
}
fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required}})
}
fn tool_list() -> Vec<Value> {
    vec![
        tool("project_context","Read the live goal, assets folder, skills, reference, indexed media (with ASR when available), and selected preview version. Call before each request. No credentials are returned.",json!({}),&[]),
        tool("generate_asset","Generate an image using configured Seedream/Google ADC, or voice using MiMo TTS. Returns a media ID usable in render_video. Call sequentially.",json!({"kind":{"type":"string","enum":["image","voice"]},"prompt":{"type":"string","minLength":1,"maxLength":8000}}),&["kind","prompt"]),
        tool("transcribe_asset","Prepare one source and return its MiMo ASR transcript. Only use a media ID from project_context.",json!({"mediaId":{"type":"string"}}),&["mediaId"]),
        tool("render_video","Render straight cuts/held images with source audio or narration and SRT sidecar captions. Saves a new MP4 directly in the assets folder and updates the preview. sourceId/audioSourceId are media IDs. Image start=0/end=hold seconds; video end is source time. Call sequentially.",json!({
            "aspect":{"type":"string","enum":["9:16","16:9","1:1"]},
            "plan":{"type":"object","properties":{
                "title":{"type":"string"},"summary":{"type":"string"},"warnings":{"type":"array","items":{"type":"string"}},
                "clips":{"type":"array","minItems":1,"maxItems":80,"items":{"type":"object","properties":{
                    "sourceId":{"type":"string"},"start":{"type":"number"},"end":{"type":"number"},"caption":{"type":"string"},"beat":{"type":"string"},"reason":{"type":"string"},"audioSourceId":{"type":["string","null"]},"audioStart":{"type":"number"},"fitAudio":{"type":"boolean"}
                },"required":["sourceId","start","end","caption","beat","reason"]}}
            },"required":["title","summary","warnings","clips"]}
        }),&["plan"]),
        tool("publish_video","Register a finished MP4 created with CLI tools. Copies it to a unique filename in the selected assets folder and updates preview/history. Never overwrites the source. Input must be inside assets or the app workspace.",json!({"path":{"type":"string"},"title":{"type":"string"},"captions":{"type":"string"},"summary":{"type":"string"}}),&["path","title"]),
    ]
}
fn prepared(w: &mut Workspace, s: &Settings, job: &Job) -> Result<(), String> {
    media::refresh_assets(w)?;
    for m in &mut w.media {
        media::prepare(m, Path::new(&w.root), s, job, |_| {})?;
    }
    if let Some(m) = &mut w.reference {
        media::prepare(m, Path::new(&w.root), s, job, |_| {})?;
    }
    Ok(())
}
fn context(w: &Workspace, selected: &str) -> Value {
    let exports: Vec<_> = w.artifacts.iter().filter(|a| a.kind == "export").collect();
    let current = exports
        .iter()
        .find(|a| a.id == selected)
        .copied()
        .or_else(|| exports.last().copied());
    json!({"goal":w.brief,"assetsFolder":w.assets_folder,"workspaceRoot":w.root,"media":w.media,"reference":w.reference,
        "skills":w.skills.iter().filter(|s|w.selected_skills.contains(&s.id)).collect::<Vec<_>>(),
        "currentVideo":current,"versions":exports,"outputDirectory":w.assets_folder})
}
fn allowed_file(w: &Workspace, input: &str) -> Result<PathBuf, String> {
    let path = Path::new(input).canonicalize().map_err(|e| e.to_string())?;
    let allowed = path.starts_with(
        Path::new(&w.root)
            .canonicalize()
            .map_err(|e| e.to_string())?,
    ) || w.assets_folder.as_ref().is_some_and(|p| {
        Path::new(p)
            .canonicalize()
            .is_ok_and(|p| path.starts_with(p))
    });
    if !allowed || !path.is_file() {
        return Err("Choose a file inside the assets folder or workspace".into());
    }
    Ok(path)
}
fn publish(
    w: &Workspace,
    s: &Settings,
    args: &Value,
    run: &Run,
    job: &Job,
) -> Result<Value, String> {
    let input = allowed_file(w, args["path"].as_str().ok_or("Missing video path")?)?;
    if input
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some("mp4")
    {
        return Err("Publish an MP4 for preview".into());
    }
    let mut cmd = tools::command(&tools::required("ffprobe", &s.ffprobe_path)?);
    cmd.args([
        "-v",
        "error",
        "-show_format",
        "-show_streams",
        "-of",
        "json",
    ])
    .arg(&input);
    let (ok, out, err) = tools::execute(cmd, job, 60, |_| {})?;
    if !ok {
        return Err(err);
    }
    let probe: Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    let video = probe["streams"]
        .as_array()
        .and_then(|ss| ss.iter().find(|s| s["codec_type"] == "video"))
        .ok_or("No video stream found")?;
    let duration: f64 = probe["format"]["duration"]
        .as_str()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0.);
    if duration <= 0. {
        return Err("Video duration is invalid".into());
    }
    let sub = args["captions"]
        .as_str()
        .filter(|p| !p.is_empty())
        .map(|p| allowed_file(w, p))
        .transpose()?;
    let dest = media::output_path(w, &run.id);
    std::fs::copy(&input, &dest).map_err(|e| e.to_string())?;
    let captions = if let Some(sub) = sub {
        let p = dest.with_extension("srt");
        std::fs::copy(sub, &p).map_err(|e| e.to_string())?;
        Some(p)
    } else {
        None
    };
    Ok(
        json!({"video":dest,"captions":captions,"title":args["title"],"summary":args["summary"].as_str().unwrap_or(""),"duration":duration,"width":video["width"],"height":video["height"]}),
    )
}
pub(crate) fn call_tool(
    app: &tauri::AppHandle,
    workspace: &str,
    params: &Value,
    job: Arc<Job>,
) -> Result<Value, String> {
    let selected = app
        .state::<Bridge>()
        .selected
        .lock()
        .unwrap()
        .get(workspace)
        .cloned()
        .unwrap_or_default();
    execute_tool(
        &app.state::<Studio>(),
        workspace,
        &selected,
        params,
        job,
        || {
            let _ = app.emit("workspace-updated", json!({"workspaceId":workspace}));
        },
    )
}
fn execute_tool(
    studio: &Studio,
    workspace: &str,
    selected: &str,
    params: &Value,
    job: Arc<Job>,
    notify: impl Fn(),
) -> Result<Value, String> {
    let name = params["name"].as_str().ok_or("Missing tool name")?;
    if !tool_list().iter().any(|t| t["name"] == name) {
        return Err("Unknown Framecraft tool".into());
    }
    let (mut w, settings, run) = {
        let _guard = studio.gate.lock().unwrap();
        studio.idle(workspace)?;
        let mut w = studio.load(workspace)?;
        if w.assets_folder.is_none() {
            return Err("Choose an assets folder in Framecraft first".into());
        }
        let run = Run {
            id: id(),
            stage: name.into(),
            status: "running".into(),
            started: now(),
            input_revision: w.revision,
            ..Default::default()
        };
        w.runs.push(run.clone());
        studio.save(&w)?;
        studio
            .jobs
            .lock()
            .unwrap()
            .insert(workspace.into(), job.clone());
        (w, studio.data()?.settings, run)
    };
    notify();
    let outcome = (|| {
        job.check()?;
        let a = &params["arguments"];
        let result = match name {
            "project_context" => {
                prepared(&mut w, &settings, &job)?;
                context(&w, selected)
            }
            "generate_asset" => {
                let kind = a["kind"]
                    .as_str()
                    .filter(|k| ["image", "voice"].contains(k))
                    .ok_or("Choose image or voice")?;
                let prompt = a["prompt"]
                    .as_str()
                    .filter(|p| !p.trim().is_empty() && p.chars().count() <= 8000)
                    .ok_or("Enter 1–8,000 characters")?;
                let mid = generation::create(&mut w, &settings, kind, prompt, &run.id, &job)?;
                json!({"media":w.media.iter().find(|m|m.id==mid)})
            }
            "transcribe_asset" => {
                media::refresh_assets(&mut w)?;
                let m = w
                    .media
                    .iter_mut()
                    .find(|m| Some(m.id.as_str()) == a["mediaId"].as_str())
                    .ok_or("Unknown media ID")?;
                media::prepare(m, Path::new(&w.root), &settings, &job, |_| {})?;
                if m.transcript.is_none() {
                    return Err(m
                        .warning
                        .clone()
                        .unwrap_or("No speech found in this asset".into()));
                }
                json!({"mediaId":m.id,"transcript":m.transcript})
            }
            "render_video" => {
                prepared(&mut w, &settings, &job)?;
                if let Some(aspect) = a["aspect"].as_str() {
                    if !["9:16", "16:9", "1:1"].contains(&aspect) {
                        return Err("Invalid aspect ratio".into());
                    }
                    w.brief.aspect = aspect.into();
                    w.revision += 1;
                }
                let plan: EditPlan = serde_json::from_value(a["plan"].clone())
                    .map_err(|e| format!("Invalid cut plan: {e}"))?;
                let mut result = media::render(&w, &plan, &run.id, &settings, &job, |_| {})?;
                result["title"] = json!(plan.title);
                result["summary"] = json!(plan.summary);
                result["warnings"] = json!(plan.warnings);
                result
            }
            "publish_video" => publish(&w, &settings, a, &run, &job)?,
            _ => unreachable!(),
        };
        if result["video"].is_string() {
            pipeline::record_result(&mut w, &run, result.clone())?;
        }
        Ok(result)
    })();
    let r = w.runs.iter_mut().find(|r| r.id == run.id).unwrap();
    r.finished = Some(now());
    r.status = if job.check().is_err() {
        "cancelled"
    } else if outcome.is_ok() {
        "succeeded"
    } else {
        "failed"
    }
    .into();
    r.message = outcome
        .as_ref()
        .err()
        .cloned()
        .unwrap_or_else(|| "Complete".into());
    let _ = std::fs::remove_dir_all(
        Path::new(&w.root)
            .join(".framecraft/runs")
            .join(&run.id)
            .join("render"),
    );
    let output = media::output_path(&w, &run.id);
    let _ = std::fs::remove_file(output.with_extension("partial.mp4"));
    if outcome.is_err() {
        let _ = std::fs::remove_file(&output);
        let _ = std::fs::remove_file(output.with_extension("srt"));
    }
    w.updated = now();
    let _guard = studio.gate.lock().unwrap();
    let saved = studio.save(&w);
    studio.jobs.lock().unwrap().remove(workspace);
    notify();
    saved?;
    outcome
}

/// Standalone stdio transport for local CLI use and headless integration checks.
/// Use a separate storage directory from a running GUI instance.
pub fn stdio(storage: &str, workspace: &str) -> Result<(), String> {
    let studio = Studio {
        storage: PathBuf::from(storage),
        gate: Mutex::new(()),
        jobs: Mutex::new(HashMap::new()),
    };
    studio.load(workspace)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let r: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        let Some(rid) = r.get("id") else {
            continue;
        };
        let result = match r["method"].as_str().unwrap_or_default() {
            "initialize" => {
                json!({"protocolVersion":r["params"]["protocolVersion"],"capabilities":{"tools":{}},"serverInfo":{"name":"Framecraft","version":env!("CARGO_PKG_VERSION")}})
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools":tool_list()}),
            "tools/call" => {
                let result = execute_tool(
                    &studio,
                    workspace,
                    "",
                    &r["params"],
                    Arc::new(Job::default()),
                    || {},
                );
                let (error, text) = match result {
                    Ok(v) => (false, v.to_string()),
                    Err(e) => (true, e),
                };
                json!({"content":[{"type":"text","text":text}],"isError":error})
            }
            _ => {
                writeln!(out,"{}",json!({"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":"Method not found"}})).map_err(|e|e.to_string())?;
                out.flush().map_err(|e| e.to_string())?;
                continue;
            }
        };
        writeln!(out, "{}", json!({"jsonrpc":"2.0","id":rid,"result":result}))
            .map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
