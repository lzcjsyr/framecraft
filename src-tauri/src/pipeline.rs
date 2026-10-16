use crate::{generation, media, model::*, store, tools::*};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Scene {
    source_id: String,
    source_start: f64,
    duration: f64,
    image_prompt: String,
    narration: String,
    caption: String,
    #[serde(default)]
    audio_source_id: String,
    #[serde(default)]
    audio_start: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Storyboard {
    title: String,
    summary: String,
    warnings: Vec<String>,
    audio_source_id: String,
    scenes: Vec<Scene>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentTurn {
    action: String,
    reply: String,
    storyboard: Storyboard,
    #[serde(default)]
    aspect_ratio: String,
}
fn storyboard_schema() -> Value {
    let text = json!({"type":"string"});
    json!({"type":"object","additionalProperties":false,
        "properties":{"title":text,"summary":text,"warnings":{"type":"array","items":text},"audioSourceId":text,
        "scenes":{"type":"array","minItems":0,"maxItems":12,"items":{"type":"object","additionalProperties":false,
            "properties":{"sourceId":text,"sourceStart":{"type":"number","minimum":0},"duration":{"type":"number","minimum":0.5,"maximum":600},
                "imagePrompt":text,"narration":text,"caption":text,"audioSourceId":text,"audioStart":{"type":"number","minimum":0}},
            "required":["sourceId","sourceStart","duration","imagePrompt","narration","caption","audioSourceId","audioStart"]}}},
        "required":["title","summary","warnings","audioSourceId","scenes"]})
}
fn schema(w: &Workspace) -> Value {
    let mut storyboard = storyboard_schema();
    let visuals: Vec<_> = std::iter::once("")
        .chain(
            w.media
                .iter()
                .filter(|m| ["video", "image"].contains(&m.kind.as_str()))
                .map(|m| m.id.as_str()),
        )
        .collect();
    let audio: Vec<_> = std::iter::once("")
        .chain(
            w.media
                .iter()
                .filter(|m| m.kind == "audio")
                .map(|m| m.id.as_str()),
        )
        .collect();
    storyboard["properties"]["audioSourceId"]["enum"] = json!(audio);
    storyboard["properties"]["scenes"]["items"]["properties"]["sourceId"]["enum"] = json!(visuals);
    storyboard["properties"]["scenes"]["items"]["properties"]["audioSourceId"]["enum"] =
        json!(audio);
    json!({"type":"object","additionalProperties":false,
        "properties":{"action":{"type":"string","enum":["reply","render"]},"reply":{"type":"string"},"aspectRatio":{"type":"string","enum":["","16:9","9:16","1:1"]},"storyboard":storyboard},
        "required":["action","reply","aspectRatio","storyboard"]})
}
fn validate(story: &Storyboard, w: &Workspace) -> Result<(), String> {
    if story.title.trim().is_empty() || story.scenes.is_empty() || story.scenes.len() > 12 {
        return Err("Agent returned an empty storyboard".into());
    }
    let mut generated = 0;
    let mut duration = 0.;
    for scene in &story.scenes {
        if !scene.duration.is_finite()
            || !(0.5..=600.).contains(&scene.duration)
            || !scene.source_start.is_finite()
            || scene.source_start < 0.
            || scene.caption.chars().count() > 300
            || scene.narration.chars().count() > 3000
            || !scene.audio_start.is_finite()
            || scene.audio_start < 0.
        {
            return Err("Agent returned invalid scene timing or text".into());
        }
        duration += scene.duration;
        if !scene.audio_source_id.is_empty()
            && !w
                .media
                .iter()
                .any(|m| m.id == scene.audio_source_id && m.kind == "audio")
        {
            return Err("Agent selected an unknown scene audio asset".into());
        }
        if scene.source_id.is_empty() {
            generated += 1;
            if scene.image_prompt.trim().is_empty() || scene.image_prompt.len() > 8000 {
                return Err("Missing image prompt for a generated scene".into());
            }
        } else {
            let source = w
                .media
                .iter()
                .find(|m| m.id == scene.source_id && ["video", "image"].contains(&m.kind.as_str()))
                .ok_or("Agent selected an unknown visual asset")?;
            if source.kind == "video"
                && scene.source_start + scene.duration > source.duration + 0.025
            {
                return Err("Agent selected a scene outside the source video".into());
            }
        }
    }
    if generated > 8 || duration > 600. {
        return Err("Use up to 8 generated images and 10 minutes per video".into());
    }
    if !story.audio_source_id.is_empty()
        && !w
            .media
            .iter()
            .any(|m| m.id == story.audio_source_id && m.kind == "audio")
    {
        return Err("Agent selected an unknown soundtrack".into());
    }
    Ok(())
}
fn agent_prompt(w: &Workspace) -> Result<String, String> {
    let folder = w
        .assets_folder
        .as_deref()
        .unwrap_or("No assets folder selected yet");
    Ok(include_str!("../../templates/video-agent.md")
        .replace("{{assets_folder}}", &json!(folder).to_string())
        .replace("{{goal}}", &json!(w.brief.topic).to_string()))
}
fn agent(
    w: &mut Workspace,
    run: &Run,
    s: &Settings,
    job: &Job,
    mut log: impl FnMut(String),
) -> Result<AgentTurn, String> {
    let dir = Path::new(&w.root).join(".framecraft/runs").join(&run.id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let context = dir.join("context.md");
    let skills: Vec<_> = w
        .skills
        .iter()
        .filter(|sk| w.selected_skills.contains(&sk.id))
        .collect();
    let task = agent_prompt(w)?;
    let current_video = run
        .base_artifact_id
        .as_ref()
        .and_then(|id| {
            w.artifacts
                .iter()
                .find(|a| &a.id == id && a.kind == "export")
        })
        .or_else(|| w.artifacts.iter().rev().find(|a| a.kind == "export"));
    let conversation = &w.conversation[w.conversation.len().saturating_sub(40)..];
    let inputs = json!({"goal":w.brief.topic,"assetsFolder":w.assets_folder,"brief":w.brief,"reference":w.reference,"assets":w.media,"skills":skills,
        "conversation":conversation,"requestKind":run.stage,"currentVideo":current_video,
        "request":w.conversation.iter().rev().find(|m| m.run_id == run.id && m.role == "user").map(|m|m.text.as_str()).unwrap_or(&w.brief.topic)});
    store::write(&dir.join("inputs.json"), &inputs)?;
    fs::write(
        &context,
        format!(
            "{task}\n\n{}",
            serde_json::to_string_pretty(&inputs).map_err(|e| e.to_string())?
        ),
    )
    .map_err(|e| e.to_string())?;
    let secret = if s.mode == "mimo" {
        Some(crate::credentials::key("MIMO_API_KEY")?)
    } else if s.mode == "api" {
        Some(key()?.ok_or("Save the agent API key first")?)
    } else {
        None
    };
    let mut cmd = agent_command(&required("claude", &s.claude_path)?, s, secret.as_deref())?;
    let session_dir = Path::new(&w.root).join(".framecraft/agent");
    fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;
    let connection = format!("{}\n{}\n{}", s.mode, s.base_url, s.model);
    if let Some(session) = w
        .agent_session
        .as_ref()
        .filter(|session| session.connection == connection)
    {
        cmd.arg("--resume").arg(&session.id);
        log("Continuing the Claude Code conversation".into());
    }
    cmd.current_dir(&session_dir).args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--max-turns",
        "30",
        "--tools",
        "Read,Glob",
        "--allowedTools",
        "Read,Glob",
        "--permission-mode",
        "default",
        "--setting-sources",
        "",
        "--safe-mode",
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--disable-slash-commands",
        "--no-chrome",
        "--append-system-prompt",
        "You are Framecraft's conversational video editor. Each turn supplies the complete current context through stdin. Its currentVideo is the version being edited, its conversation records the user's requests, and its current asset IDs supersede earlier tool results. Reply to questions without rendering; return a complete revised storyboard for edit requests. Follow the response schema and renderer constraints in the supplied context.",
        "--model",
        &s.model,
        "--json-schema",
        &schema(w).to_string(),
        "--add-dir",
        &w.root,
    ]);
    if let Some(folder) = &w.assets_folder {
        cmd.arg("--add-dir").arg(folder);
    }
    cmd.arg("--").arg("Use the complete current context supplied through stdin below. For requestKind=create, you MUST use Glob with pattern **/* and the actual assetsFolder path to inspect the folder, even though metadata is supplied. Continue the user's editing conversation and return the response schema. For subsequent edits and questions, inspect additional files when needed.");
    log(format!(
        "Claude Code · {} · composing the storyboard",
        s.model
    ));
    let mut result = None;
    let mut session_id = None;
    let input = fs::File::open(&context).map_err(|e| e.to_string())?;
    let (ok, _, err) = execute_with_input(cmd, job, 900, Some(input), |line| {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if let Some(id) = value["session_id"].as_str() {
                session_id = Some(id.to_owned());
            }
            match value["type"].as_str() {
                Some("assistant") => {
                    if let Some(blocks) = value["message"]["content"].as_array() {
                        for block in blocks {
                            if block["type"] == "tool_use" && block["name"] == "Glob" {
                                log("Inspecting the assets folder".into());
                            }
                            if block["type"] == "tool_use" && block["name"] == "Read" {
                                let file = Path::new(
                                    block["input"]["file_path"].as_str().unwrap_or("evidence"),
                                )
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                                log(format!("Reading {file}"));
                            }
                            if block["type"] == "text" {
                                if let Some(text) = block["text"]
                                    .as_str()
                                    .filter(|text| !text.trim_start().starts_with('{'))
                                {
                                    log(format!(
                                        "Claude Code: {}",
                                        redact(text, secret.as_deref())
                                    ));
                                }
                            }
                        }
                    }
                }
                Some("result") => result = Some(value),
                _ => {}
            }
        }
    })?;
    if let Some(id) = session_id {
        w.agent_session = Some(AgentSession { id, connection });
    }
    let result = result.ok_or_else(|| {
        format!(
            "Claude Code returned no result: {}",
            redact(&err, secret.as_deref())
        )
    })?;
    store::write(&dir.join("agent-result.json"), &result)?;
    if !ok || result["is_error"] == true {
        let detail = result["result"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| {
                result["errors"].as_array().map(|errors| {
                    errors
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("; ")
                })
            })
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| {
                format!(
                    "{}: {}",
                    result["subtype"].as_str().unwrap_or("Request failed"),
                    err
                )
            });
        return Err(format!(
            "Claude Code: {}",
            redact(&detail, secret.as_deref())
        ));
    }
    let value = if result["structured_output"].is_object() {
        result["structured_output"].clone()
    } else {
        let text = result["result"]
            .as_str()
            .ok_or("Agent returned no storyboard")?;
        serde_json::from_str(
            text.trim()
                .trim_start_matches("```json")
                .trim_end_matches("```")
                .trim(),
        )
        .map_err(|_| "Agent did not return valid JSON")?
    };
    let turn: AgentTurn =
        serde_json::from_value(value).map_err(|e| format!("Invalid storyboard: {e}"))?;
    if turn.reply.trim().is_empty() || turn.reply.chars().count() > 12000 {
        return Err("Claude Code returned an empty or oversized reply".into());
    }
    if !["reply", "render"].contains(&turn.action.as_str()) {
        return Err("Claude Code returned an unknown action".into());
    }
    if turn.action == "render" {
        validate(&turn.storyboard, w)?;
        store::write(&dir.join("storyboard.json"), &turn.storyboard)?;
    }
    Ok(turn)
}
pub fn create_video(
    w: &mut Workspace,
    run: &Run,
    s: &Settings,
    job: &Job,
    mut log: impl FnMut(String),
) -> Result<Value, String> {
    let root = std::path::PathBuf::from(&w.root);
    log("Reading the current assets folder".into());
    if w.assets_folder.is_some() {
        media::refresh_assets(w)?;
    }
    if let Some(reference) = &mut w.reference {
        log("Studying the reference video".into());
        media::prepare(reference, &root, s, job, &mut log)?;
    }
    if w.media.iter().filter(|m| m.role != "generated").count() > 100 {
        return Err("Use up to 100 assets per video".into());
    }
    for asset in &mut w.media {
        media::prepare(asset, &root, s, job, &mut log)?;
    }
    let turn = agent(w, run, s, job, &mut log)?;
    if turn.action == "reply" {
        return Ok(json!({"reply":turn.reply}));
    }
    let story = turn.storyboard;
    if w.assets_folder.is_none() {
        return Err("Choose an assets folder before rendering a video".into());
    }
    if !turn.aspect_ratio.is_empty() {
        if !["16:9", "9:16", "1:1"].contains(&turn.aspect_ratio.as_str()) {
            return Err("Claude Code returned an unsupported aspect ratio".into());
        }
        if w.brief.aspect != turn.aspect_ratio {
            w.brief.aspect = turn.aspect_ratio;
            w.revision += 1;
        }
    }
    log(format!("Storyboard ready · {} scenes", story.scenes.len()));
    let storyboard_id = id();
    w.artifacts.push(Artifact {
        id: storyboard_id,
        kind: "storyboard".into(),
        title: story.title.clone(),
        path: root
            .join(".framecraft/runs")
            .join(&run.id)
            .join("storyboard.json")
            .to_string_lossy()
            .into(),
        created: now(),
        run_id: run.id.clone(),
        data: serde_json::to_value(&story).map_err(|e| e.to_string())?,
    });
    let mut settings = s.clone();
    settings.generation.image_size = match w.brief.aspect.as_str() {
        "16:9" => "2560x1536",
        "9:16" => "1536x2560",
        _ => "2048x2048",
    }
    .into();
    let mut clips = Vec::new();
    let mut offset = 0.;
    let mut warnings = story.warnings.clone();
    for (index, scene) in story.scenes.iter().enumerate() {
        job.check()?;
        let source_id = if scene.source_id.is_empty() {
            log(format!(
                "Generating image {}/{}",
                index + 1,
                story.scenes.len()
            ));
            generation::create(w, &settings, "image", &scene.image_prompt, &run.id, job)?
        } else {
            scene.source_id.clone()
        };
        let mut duration = scene.duration;
        let audio_source_id = if !scene.audio_source_id.is_empty() {
            Some(scene.audio_source_id.clone())
        } else if !story.audio_source_id.is_empty() {
            let audio = w
                .media
                .iter()
                .find(|m| m.id == story.audio_source_id)
                .unwrap();
            if offset < audio.duration {
                Some(audio.id.clone())
            } else {
                warnings.push("The supplied soundtrack ends before the video.".into());
                None
            }
        } else if w.brief.narration && !scene.narration.trim().is_empty() {
            log(format!(
                "Generating narration {}/{} · MiMo",
                index + 1,
                story.scenes.len()
            ));
            let id = generation::create(w, &settings, "voice", &scene.narration, &run.id, job)?;
            let voice = w.media.iter().find(|m| m.id == id).unwrap();
            let source = w.media.iter().find(|m| m.id == source_id).unwrap();
            duration = duration.max(voice.duration + 0.1);
            if source.kind == "video" {
                duration = duration.min(source.duration - scene.source_start);
            }
            Some(id)
        } else {
            None
        };
        let source = w.media.iter().find(|m| m.id == source_id).unwrap();
        let start = if source.kind == "image" {
            0.
        } else {
            scene.source_start
        };
        clips.push(Clip {
            source_id,
            start,
            end: start + duration,
            beat: format!("Scene {}", index + 1),
            reason: scene.image_prompt.clone(),
            caption: if !scene.caption.is_empty() {
                scene.caption.clone()
            } else {
                String::new()
            },
            audio_source_id,
            audio_start: if !scene.audio_source_id.is_empty() {
                scene.audio_start
            } else if !story.audio_source_id.is_empty() {
                offset
            } else {
                0.
            },
            fit_audio: story.audio_source_id.is_empty() && scene.audio_source_id.is_empty(),
        });
        offset += (duration * 30.).round() / 30.;
    }
    let plan = EditPlan {
        title: story.title.clone(),
        summary: story.summary.clone(),
        warnings,
        clips,
    };
    store::write(&root.join("edits").join(format!("{}.json", run.id)), &plan)?;
    let mut result = media::render(w, &plan, &run.id, s, job, &mut log)?;
    result["title"] = json!(story.title);
    result["summary"] = json!(story.summary);
    result["warnings"] = json!(plan.warnings);
    result["reply"] = json!(turn.reply);
    Ok(result)
}
pub(crate) fn record_result(
    w: &mut Workspace,
    run: &Run,
    data: Value,
) -> Result<Option<String>, String> {
    let reply = data["reply"]
        .as_str()
        .unwrap_or("Your video is ready.")
        .to_owned();
    let artifact_id = if data["video"].is_string() {
        let path = Path::new(&w.root)
            .join("exports")
            .join(&run.id)
            .join("result.json");
        store::write(&path, &data)?;
        let aid = id();
        w.artifacts.push(Artifact {
            id: aid.clone(),
            kind: "export".into(),
            title: data["title"].as_str().unwrap_or("Your video").into(),
            path: path.to_string_lossy().into(),
            created: now(),
            run_id: run.id.clone(),
            data,
        });
        Some(aid)
    } else {
        None
    };
    w.conversation.push(ChatMessage {
        id: id(),
        role: "assistant".into(),
        text: reply,
        created: now(),
        run_id: run.id.clone(),
        artifact_id: artifact_id.clone(),
    });
    Ok(artifact_id)
}
pub fn launch(app: tauri::AppHandle, mut w: Workspace, run: Run, s: Settings, job: Arc<Job>) {
    std::thread::spawn(move || {
        let logs = Mutex::new(Vec::new());
        let dir = Path::new(&w.root).join(".framecraft/runs").join(&run.id);
        let workspace_id = w.id.clone();
        let log = |line: String| {
            logs.lock().unwrap().push(line.clone());
            let _ = app.emit(
                "run-log",
                json!({"workspaceId":workspace_id,"runId":run.id,"line":line}),
            );
        };
        let outcome = create_video(&mut w, &run, &s, &job, log);
        let (status, message, artifact_id) = match outcome {
            Ok(data) if job.check().is_ok() => match record_result(&mut w, &run, data) {
                Ok(aid) => (
                    "succeeded",
                    if aid.is_some() {
                        "Video ready"
                    } else {
                        "Reply ready"
                    }
                    .into(),
                    aid,
                ),
                Err(e) => ("failed", e, None),
            },
            Err(e) if job.check().is_ok() => ("failed", e, None),
            _ => ("cancelled", "Run cancelled".into(), None),
        };
        if status != "succeeded" {
            w.conversation.push(ChatMessage {
                id: id(), role: "assistant".into(),
                text: format!("{message}. Your previous video is still available. You can continue editing here."),
                created: now(), run_id: run.id.clone(), artifact_id: None,
            });
        }
        let lines = logs.into_inner().unwrap();
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("events.log"), lines.join("\n"));
        if let Some(r) = w.runs.iter_mut().find(|r| r.id == run.id) {
            r.status = status.into();
            r.message = message;
            r.finished = Some(now());
            r.logs = lines;
            r.artifact_id = artifact_id;
        }
        let _ = fs::remove_dir_all(dir.join("render"));
        if status != "succeeded" {
            let output = media::output_path(&w, &run.id);
            let _ = fs::remove_file(output.with_extension("partial.mp4"));
            let _ = fs::remove_dir_all(Path::new(&w.root).join("exports").join(&run.id));
        }
        w.updated = now();
        let studio = app.state::<store::Studio>();
        let _guard = studio.gate.lock().unwrap();
        if let Err(error) = studio.save(&w) {
            let _ = app.emit(
                "run-log",
                json!({"workspaceId":w.id,"runId":run.id,"line":error}),
            );
        }
        studio.jobs.lock().unwrap().remove(&w.id);
        let _ = app.emit("workspace-updated", json!({"workspaceId":w.id}));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversational_replies_preserve_video_versions() {
        let root = std::env::temp_dir().join(format!("framecraft-chat-{}", id()));
        fs::create_dir_all(&root).unwrap();
        let mut w = store::create("Chat".into(), root.to_string_lossy().into()).unwrap();
        let run = Run {
            id: id(),
            stage: "chat".into(),
            ..Default::default()
        };
        let artifact = record_result(
            &mut w,
            &run,
            json!({"reply":"The first video.","video":"preview.mp4","title":"One"}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            w.conversation[0].artifact_id.as_deref(),
            Some(artifact.as_str())
        );
        assert!(
            record_result(&mut w, &run, json!({"reply":"It is six seconds long."}))
                .unwrap()
                .is_none()
        );
        assert_eq!(w.artifacts.len(), 1);
        assert_eq!(w.conversation.len(), 2);
        assert!(w.conversation[1].artifact_id.is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Small real Claude Code request using the existing local account login; no media requests"]
    fn real_local_conversation() {
        let root = std::env::temp_dir().join(format!("framecraft-local-{}", id()));
        fs::create_dir_all(&root).unwrap();
        let mut w = store::create("Local login".into(), root.to_string_lossy().into()).unwrap();
        let run = Run {
            id: id(),
            stage: "chat".into(),
            ..Default::default()
        };
        w.conversation.push(ChatMessage { id: id(), role: "user".into(), text: "Reply exactly: Local Claude Code is connected. This is a connection question, do not render.".into(), run_id:run.id.clone(), created:now(), artifact_id:None });
        let settings = Settings {
            mode: "local".into(),
            model: "sonnet".into(),
            ..Default::default()
        };
        require_local_login(&settings.claude_path).unwrap();
        let reply = create_video(&mut w, &run, &settings, &Job::default(), |line| {
            println!("{line}")
        })
        .unwrap();
        assert!(reply.get("video").is_none());
        assert!(reply["reply"]
            .as_str()
            .unwrap()
            .contains("Local Claude Code is connected"));
        assert!(w.agent_session.is_some());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn storyboard_does_not_require_reference_and_validates_generated_scenes() {
        let root = std::env::temp_dir().join(format!("framecraft-story-{}", id()));
        fs::create_dir_all(&root).unwrap();
        let w = store::create("Test".into(), root.to_string_lossy().into()).unwrap();
        let mut story = Storyboard {
            title: "Test".into(),
            summary: "".into(),
            warnings: vec![],
            audio_source_id: "".into(),
            scenes: vec![Scene {
                source_id: "".into(),
                source_start: 0.,
                duration: 5.,
                image_prompt: "Blue sea".into(),
                narration: "Hello".into(),
                caption: "Hello".into(),
                audio_source_id: "".into(),
                audio_start: 0.,
            }],
        };
        assert!(validate(&story, &w).is_ok());
        story.scenes[0].source_id = "invented".into();
        assert!(validate(&story, &w).is_err());
        story.scenes[0].source_id = "".into();
        story.scenes[0].duration = f64::NAN;
        assert!(validate(&story, &w).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Real Claude Code + MiMo + Google ADC; creates a billable short video"]
    fn real_creation_workflow() {
        let root = std::env::var("FRAMECRAFT_TEST_OUTPUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join(format!("framecraft-live-{}", id())));
        fs::create_dir_all(&root).unwrap();
        let mut w = store::create("A quiet morning".into(), root.to_string_lossy().into()).unwrap();
        let assets = root.join("input-assets");
        fs::create_dir_all(&assets).unwrap();
        w.assets_folder = Some(assets.to_string_lossy().into());
        w.brief.topic="Create a 6-second video about a quiet morning by the sea. Use exactly one generated image: a cinematic sunrise over calm water. A single very short Chinese narration sentence, no more than 10 Chinese characters. One scene only. Caption the spoken line. No additional effects.".into();
        w.brief.duration = 6;
        w.brief.aspect = "16:9".into();
        w.brief.language = "简体中文".into();
        let mut s = Settings::default();
        s.generation.image_provider = "google_adc".into();
        s.generation.image_model = "gemini-3.1-flash-image".into();
        let run = Run {
            id: id(),
            stage: "create".into(),
            status: "running".into(),
            started: now(),
            finished: None,
            message: "".into(),
            logs: vec![],
            artifact_id: None,
            input_revision: 0,
            base_artifact_id: None,
        };
        let data =
            create_video(&mut w, &run, &s, &Job::default(), |line| println!("{line}")).unwrap();
        assert!(Path::new(data["video"].as_str().unwrap()).is_file());
        assert!(data["duration"].as_f64().unwrap() >= 5.);
        store::write(&root.join("workspace.json"), &w).unwrap();
        println!("Real video: {}", data["video"].as_str().unwrap());
    }
}

#[cfg(test)]
mod folder_integration {
    use super::*;
    #[test]
    fn folder_and_goal_are_explicit_inputs_to_the_agent_template() {
        let root = std::env::temp_dir().join(format!("framecraft-template-{}", id()));
        fs::create_dir_all(&root).unwrap();
        let mut w = store::create("Template".into(), root.to_string_lossy().into()).unwrap();
        assert!(agent_prompt(&w).unwrap().contains("No assets folder"));
        w.assets_folder = Some(root.join("My related content").to_string_lossy().into());
        w.brief.topic = "Use this folder to explain a quiet morning.".into();
        let prompt = agent_prompt(&w).unwrap();
        assert!(prompt.contains(&json!(w.assets_folder).to_string()));
        assert!(prompt.contains(&w.brief.topic));
        assert!(prompt.contains("Glob"));
        assert!(!prompt.contains("{{assets_folder}}"));
        let mut legacy = serde_json::to_value(&w).unwrap();
        let folder = legacy
            .as_object_mut()
            .unwrap()
            .remove("assetsFolder")
            .unwrap();
        legacy["rawFolder"] = folder;
        legacy.as_object_mut().unwrap().remove("conversation");
        legacy.as_object_mut().unwrap().remove("agent_session");
        legacy.as_object_mut().unwrap().remove("agentSession");
        let migrated: Workspace = serde_json::from_value(legacy).unwrap();
        assert_eq!(migrated.assets_folder, w.assets_folder);
        assert!(migrated.conversation.is_empty() && migrated.agent_session.is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Real Claude Code + MiMo reads a folder of image, audio and text, then renders a video"]
    fn real_folder_creation_workflow() {
        folder_workflow(false);
    }
    #[test]
    #[ignore = "Real Claude Code conversation: create, edit, answer, and branch from an earlier version"]
    fn real_conversation_editing() {
        folder_workflow(true);
    }
    fn folder_workflow(with_edits: bool) {
        let root = std::env::var("FRAMECRAFT_TEST_OUTPUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::temp_dir().join(format!("framecraft-folder-live-{}", id()))
            });
        let assets = root.join("related-content");
        fs::create_dir_all(assets.join("images")).unwrap();
        let job = Job::default();
        let supplied = std::env::var("FRAMECRAFT_TEST_ASSETS").ok();
        if let Some(supplied) = supplied {
            let supplied = Path::new(&supplied);
            fs::copy(
                supplied.join("images/sunrise.jpg"),
                assets.join("images/sunrise.jpg"),
            )
            .unwrap();
            fs::copy(supplied.join("narration.wav"), assets.join("narration.wav")).unwrap();
        } else {
            let mut image = command(&required("ffmpeg", "").unwrap());
            image
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "gradients=s=1280x720:c0=0xfed5a5:c1=0x649fc1:x0=640:y0=0:x1=640:y1=720",
                    "-frames:v",
                    "1",
                ])
                .arg(assets.join("images/sunrise.jpg"));
            assert!(execute(image, &job, 30, |_| {}).unwrap().0);
            let (voice, _) = tauri::async_runtime::block_on(generation::request(
                &Settings::default().generation,
                "voice",
                "清晨的颜色，从温暖的橙色，慢慢变成宁静的蓝色。",
                &job,
            ))
            .unwrap();
            let speech = root.join("speech.wav");
            fs::write(&speech, voice).unwrap();
            let mut cmd = command(&required("ffmpeg", "").unwrap());
            cmd.args(["-v", "error", "-y", "-i"])
                .arg(&speech)
                .args(["-vn", "-af", "apad", "-t", "6"])
                .arg(assets.join("narration.wav"));
            assert!(execute(cmd, &job, 30, |_| {}).unwrap().0);
            fs::remove_file(speech).unwrap();
        }
        fs::write(assets.join("story.md"),"Make a quiet video using images/sunrise.jpg and the complete supplied narration.wav soundtrack. Do not generate new images or speech. Use one scene, 6 seconds, landscape. The scene caption is 清晨的颜色。 All files belong to this video.").unwrap();
        let work = root.join("workspace");
        fs::create_dir_all(&work).unwrap();
        let mut w =
            store::create("Folder-driven video".into(), work.to_string_lossy().into()).unwrap();
        w.assets_folder = Some(assets.to_string_lossy().into());
        w.brief.topic="Read the assets folder and follow story.md. Combine the supplied photo, soundtrack, and text into a single six-second video. Use every supplied file. No new images or speech.".into();
        w.brief.duration = 6;
        w.brief.aspect = "16:9".into();
        w.brief.narration = false;
        let run = Run {
            id: id(),
            stage: "create".into(),
            status: "running".into(),
            started: now(),
            finished: None,
            message: "".into(),
            logs: vec![],
            artifact_id: None,
            input_revision: 0,
            base_artifact_id: None,
        };
        let mut logs = Vec::new();
        let result = create_video(&mut w, &run, &Settings::default(), &job, |line| {
            println!("{line}");
            logs.push(line);
        })
        .unwrap();
        assert!(logs
            .iter()
            .any(|line| line == "Inspecting the assets folder"));
        assert!(Path::new(result["video"].as_str().unwrap()).is_file());
        let inputs: Value = store::read(
            &work
                .join(".framecraft/runs")
                .join(&run.id)
                .join("inputs.json"),
        )
        .unwrap();
        assert_eq!(inputs["assetsFolder"], json!(assets.to_string_lossy()));
        assert_eq!(inputs["assets"].as_array().unwrap().len(), 3);
        assert!(w.media.iter().all(|m| m.role == "folder"));
        store::write(&root.join("result.json"), &result).unwrap();
        println!("Folder-driven video: {}", result["video"].as_str().unwrap());
        if with_edits {
            w.conversation.push(ChatMessage {
                id: id(),
                role: "user".into(),
                text: w.brief.topic.clone(),
                run_id: run.id.clone(),
                created: now(),
                artifact_id: None,
            });
            let first_video = result["video"].as_str().unwrap().to_owned();
            let first_source = result["plan"]["clips"][0]["sourceId"].clone();
            let first = record_result(&mut w, &run, result).unwrap();
            let session = w.agent_session.as_ref().unwrap().id.clone();
            store::write(&work.join("workspace.json"), &w).unwrap();
            let mut w: Workspace = store::read(&work.join("workspace.json")).unwrap();
            let mut edit = Run {
                id: id(),
                stage: "chat".into(),
                base_artifact_id: first.clone(),
                ..run.clone()
            };
            w.conversation.push(ChatMessage {
                id: id(), role: "user".into(),
                text: "Edit the selected video: shorten it to exactly 4 seconds, and change the caption to 清晨，重新出发。 Keep the same image and supplied audio starting at zero. Do not generate new media. Render the revision now.".into(),
                run_id: edit.id.clone(), created: now(), artifact_id: None,
            });
            let mut continued = false;
            let revision = create_video(&mut w, &edit, &Settings::default(), &job, |line| {
                println!("{line}");
                continued |= line == "Continuing the Claude Code conversation";
            })
            .unwrap();
            assert!(continued);
            assert_eq!(w.agent_session.as_ref().unwrap().id, session);
            assert_eq!(revision["plan"]["clips"][0]["sourceId"], first_source);
            assert!((revision["duration"].as_f64().unwrap() - 4.).abs() < 0.04);
            assert!(fs::read_to_string(revision["captions"].as_str().unwrap())
                .unwrap()
                .contains("重新出发"));
            assert!(Path::new(&first_video).is_file());
            store::write(&root.join("revision.json"), &revision).unwrap();
            edit.base_artifact_id = record_result(&mut w, &edit, revision).unwrap();
            store::write(&work.join("workspace.json"), &w).unwrap();
            edit.id = id();
            w.conversation.push(ChatMessage {
                id: id(), role: "user".into(), text: "What is the duration of the selected version? Answer with the number of seconds. This is a question only: do not render or change the storyboard.".into(),
                run_id: edit.id.clone(), created: now(), artifact_id: None,
            });
            let answer = create_video(&mut w, &edit, &Settings::default(), &job, |line| {
                println!("{line}")
            })
            .unwrap();
            assert!(answer.get("video").is_none());
            assert!(answer["reply"].as_str().unwrap().contains('4'));
            assert!(record_result(&mut w, &edit, answer).unwrap().is_none());
            assert_eq!(w.artifacts.iter().filter(|a| a.kind == "export").count(), 2);
            assert_eq!(w.agent_session.as_ref().unwrap().id, session);
            assert_eq!(w.conversation.len(), 6);
            store::write(&work.join("workspace.json"), &w).unwrap();
            edit.id = id();
            edit.base_artifact_id = first;
            w.conversation.push(ChatMessage {
                id: id(), role: "user".into(), text: "Edit the SELECTED original version shown in currentVideo, not the latest revision: change only the caption to 原版，新的开始。 and convert to portrait 9:16. Keep the selected original's full duration, image, and supplied audio. Render this branch now; do not generate new media.".into(),
                run_id: edit.id.clone(), created: now(), artifact_id: None,
            });
            let branch = create_video(&mut w, &edit, &Settings::default(), &job, |line| {
                println!("{line}")
            })
            .unwrap();
            assert!((branch["duration"].as_f64().unwrap() - 6.).abs() < 0.04);
            assert_eq!(branch["width"], 1080);
            assert_eq!(branch["height"], 1920);
            assert_eq!(branch["plan"]["clips"][0]["sourceId"], first_source);
            store::write(&root.join("branch.json"), &branch).unwrap();
            assert!(record_result(&mut w, &edit, branch).unwrap().is_some());
            assert_eq!(w.conversation.len(), 8);
            assert_eq!(w.artifacts.iter().filter(|a| a.kind == "export").count(), 3);
            assert_eq!(w.agent_session.as_ref().unwrap().id, session);
            store::write(&work.join("workspace.json"), &w).unwrap();
            println!("Conversation passed: three video versions, eight persisted messages, one resumed Claude Code session, question without render, portrait branch from the selected original.");
        }
    }
}
