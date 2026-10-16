use crate::{media, mimo, model::*, store, tools::Job};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tauri::{Emitter, Manager};

pub fn validate(s: &GenerationSettings) -> Result<(), String> {
    if s.voice.is_empty()
        || s.voice.len() > 100
        || s.voice_model != "mimo-v2.5-tts"
        || !["doubao", "google_adc", "google"].contains(&s.image_provider.as_str())
        || s.google_project.len() > 100
        || !s
            .google_project
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || s.image_model.is_empty()
        || s.image_model.len() > 200
        || !s
            .image_model
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        || !["2048x2048", "1536x2560", "2560x1536"].contains(&s.image_size.as_str())
    {
        return Err("Check the voice, image model, and project settings".into());
    }
    Ok(())
}
fn image_payload(s: &GenerationSettings, prompt: &str) -> Value {
    if s.image_provider == "doubao" {
        json!({"model":s.image_model,"prompt":prompt,"size":s.image_size,
            "response_format":"b64_json","watermark":false,"stream":false})
    } else {
        let aspect = match s.image_size.as_str() {
            "1536x2560" => "9:16",
            "2560x1536" => "16:9",
            _ => "1:1",
        };
        json!({"contents":[{"role":"user","parts":[{"text":prompt}]}],
            "generationConfig":{"responseModalities":["TEXT","IMAGE"],"imageConfig":{"aspectRatio":aspect,"imageSize":"2K"}}})
    }
}
fn image_response(value: &Value, provider: &str) -> Result<(Vec<u8>, &'static str), String> {
    let encoded = if provider == "doubao" {
        value["data"][0]["b64_json"].as_str()
    } else {
        value["candidates"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|c| c["content"]["parts"].as_array().into_iter().flatten())
            .find_map(|p| p["inlineData"]["data"].as_str())
    }
    .ok_or("Provider returned no image. Check the model or revise the prompt.")?;
    let bytes = STANDARD.decode(encoded).map_err(|_| "Invalid image data")?;
    let ext = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "jpg"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "webp"
    } else {
        return Err("Provider returned an unsupported image format".into());
    };
    Ok((bytes, ext))
}
pub async fn request(
    s: &GenerationSettings,
    kind: &str,
    prompt: &str,
    job: &Job,
) -> Result<(Vec<u8>, &'static str), String> {
    validate(s)?;
    if kind == "voice" {
        let style = format!(
            "Speak naturally with clear diction. Mood: {}. Speed adjustment: {} percent.",
            s.emotion, s.speech_rate
        );
        let value = mimo::chat(mimo::speech_payload(&s.voice, &style, prompt), job).await?;
        return Ok((mimo::speech_response(&value)?, "wav"));
    }
    let client = mimo::client()?;
    let secret;
    let request = if s.image_provider == "google_adc" {
        let adc = crate::credentials::adc()?;
        let project = crate::credentials::adc_project(&adc, &s.google_project)?
            .ok_or("Enter your Google Cloud project ID in Settings")?;
        secret = crate::credentials::refresh_adc(&client, &adc).await?.token;
        client.post(format!("https://aiplatform.googleapis.com/v1/projects/{project}/locations/global/publishers/google/models/{}:generateContent",s.image_model))
            .bearer_auth(&secret).header("x-goog-user-project",project)
    } else if s.image_provider == "doubao" {
        secret = crate::credentials::key("VOLCENGINE_API_KEY")?;
        client
            .post("https://ark.cn-beijing.volces.com/api/v3/images/generations")
            .bearer_auth(&secret)
    } else {
        secret = crate::credentials::key("GOOGLE_CLOUD_API_KEY")?;
        client
            .post(format!(
                "https://aiplatform.googleapis.com/v1/publishers/google/models/{}:generateContent",
                s.image_model
            ))
            .header("x-goog-api-key", &secret)
    };
    let bytes = mimo::send(request.json(&image_payload(s, prompt)), &secret, job).await?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid image response")?;
    image_response(&value, &s.image_provider)
}
pub fn create(
    w: &mut Workspace,
    s: &Settings,
    kind: &str,
    prompt: &str,
    run_id: &str,
    job: &Job,
) -> Result<String, String> {
    let (bytes, ext) = tauri::async_runtime::block_on(request(&s.generation, kind, prompt, job))?;
    job.check()?;
    let dir = Path::new(&w.root).join("assets").join(run_id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{kind}-{}.{ext}", id()));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let mut m = media::inventory(&path, "generated")?;
    if kind == "voice" {
        m.transcript = Some(prompt.into());
    }
    if let Err(error) = media::prepare(&mut m, Path::new(&w.root), s, job, |_| {}) {
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    let media_id = m.id.clone();
    w.media.push(m);
    w.revision += 1;
    w.artifacts.push(Artifact {
        id: id(),
        kind: kind.into(),
        title: prompt.chars().take(60).collect(),
        path: path.to_string_lossy().into(),
        created: now(),
        run_id: run_id.into(),
        data: json!({"prompt":prompt,"mediaId":media_id,
            "provider":if kind=="voice"{"mimo"}else{&s.generation.image_provider}}),
    });
    Ok(media_id)
}
pub fn launch(
    app: tauri::AppHandle,
    mut w: Workspace,
    run: Run,
    s: Settings,
    job: Arc<Job>,
    prompt: String,
) {
    std::thread::spawn(move || {
        let outcome = create(&mut w, &s, &run.stage, &prompt, &run.id, &job)
            .map(|_| w.artifacts.last().unwrap().id.clone());
        let studio = app.state::<store::Studio>();
        if let Some(r) = w.runs.iter_mut().find(|r| r.id == run.id) {
            r.finished = Some(now());
            match outcome {
                Ok(id) => {
                    r.status = "succeeded".into();
                    r.message = "Asset ready".into();
                    r.artifact_id = Some(id);
                }
                Err(e) => {
                    r.status = if job.check().is_err() {
                        "cancelled"
                    } else {
                        "failed"
                    }
                    .into();
                    r.message = e;
                }
            }
        }
        w.updated = now();
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
    fn images_require_media_and_valid_settings() {
        assert!(image_response(&json!({"candidates":[]}), "google_adc").is_err());
        let data = STANDARD.encode(b"\x89PNG\r\n\x1a\nfixture");
        assert_eq!(
            image_response(&json!({"data":[{"b64_json":data}]}), "doubao")
                .unwrap()
                .1,
            "png"
        );
        let mut settings = GenerationSettings::default();
        settings.image_model = "bad/model?key=secret".into();
        assert!(validate(&settings).is_err());
    }
}

#[cfg(test)]
mod live {
    use super::*;
    #[test]
    #[ignore = "Real Seedream call using the saved key; one billable image"]
    fn real_seedream_provider() {
        let settings = GenerationSettings {
            image_provider: "doubao".into(),
            image_model: "doubao-seedream-5-0-pro-260628".into(),
            ..Default::default()
        };
        let bytes = tauri::async_runtime::block_on(request(
            &settings,
            "image",
            "A single blue ceramic cup on a white table in natural light. No text.",
            &Job::default(),
        ))
        .unwrap()
        .0;
        let path = std::env::var("FRAMECRAFT_TEST_IMAGE").unwrap_or_else(|_| {
            std::env::temp_dir()
                .join("framecraft-seedream-test.png")
                .to_string_lossy()
                .into()
        });
        std::fs::write(&path, bytes).unwrap();
        println!("Seedream image: {path}");
    }
}
