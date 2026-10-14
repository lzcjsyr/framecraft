use crate::{media, model::*, store::Studio, tools::Job};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tauri::{Emitter, Manager};

const LIMIT: usize = 96 * 1024 * 1024;
pub fn validate(s: &GenerationSettings) -> Result<(), String> {
    if s.voice.len() > 200
        || !["seed-tts-2.0-standard", "seed-tts-2.0-expressive"].contains(&s.voice_model.as_str())
        || !(-50..=100).contains(&s.speech_rate)
        || !["neutral", "happy", "sad"].contains(&s.emotion.as_str())
        || !["doubao", "google", "google_adc"].contains(&s.image_provider.as_str())
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
        return Err("Check the voice, model, speed, and image settings".into());
    }
    Ok(())
}
fn resource(voice: &str) -> &'static str {
    if voice.starts_with("S_") {
        "seed-icl-2.0"
    } else if voice.contains("_uranus_") || voice.starts_with("saturn_") {
        "seed-tts-2.0"
    } else if voice.ends_with("_moon_bigtts") || voice.ends_with("_mars_bigtts") {
        "seed-tts-1.0"
    } else {
        "seed-tts-2.0"
    }
}
fn voice_payload(s: &GenerationSettings, text: &str) -> Value {
    json!({"user":{"uid":"framecraft"},"req_params":{
        "text":text,"speaker":s.voice,"model":s.voice_model,
        "audio_params":{"format":"pcm","sample_rate":48000,"speech_rate":s.speech_rate,
            "emotion":s.emotion,"emotion_scale":4},"additions":"{\"silence_duration\":0}"}})
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
            "generationConfig":{"responseModalities":["TEXT","IMAGE"],
                "imageConfig":{"aspectRatio":aspect,"imageSize":"2K"}}})
    }
}
fn decode(value: &str) -> Result<Vec<u8>, String> {
    STANDARD
        .decode(value)
        .map_err(|_| "Provider returned invalid media data".into())
}
fn wav(pcm: &[u8]) -> Result<Vec<u8>, String> {
    if pcm.is_empty() || !pcm.len().is_multiple_of(2) {
        return Err("Provider returned incomplete PCM audio".into());
    }
    let n = u32::try_from(pcm.len()).map_err(|_| "Audio too large")?;
    let mut out = Vec::with_capacity(pcm.len() + 44);
    out.extend(b"RIFF");
    out.extend((n + 36).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(48000u32.to_le_bytes());
    out.extend(96000u32.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(n.to_le_bytes());
    out.extend(pcm);
    Ok(out)
}
fn voice_response(body: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(body).map_err(|_| "Invalid voice response")?;
    let mut pcm = Vec::new();
    let mut complete = false;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let line = line.strip_prefix("data:").unwrap_or(line).trim();
        let v: Value = serde_json::from_str(line).map_err(|_| "Invalid voice stream")?;
        let code = v["code"]
            .as_i64()
            .ok_or("Voice response has no status code")?;
        if code == 20000000 {
            complete = true;
            break;
        }
        if code != 0 {
            return Err(format!(
                "ByteDance voice error {code}. Check key, voice permissions, and resource quota."
            ));
        }
        if let Some(data) = v["data"].as_str().filter(|s| !s.is_empty()) {
            pcm.extend(decode(data)?);
        }
        if pcm.len() > LIMIT {
            return Err("Voice response exceeds size limit".into());
        }
    }
    if !complete {
        return Err("Voice stream ended before completion; retry generation".into());
    }
    wav(&pcm)
}
fn image_response(body: &[u8], provider: &str) -> Result<(Vec<u8>, &'static str), String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "Invalid image response")?;
    let encoded = if provider == "doubao" {
        v["data"][0]["b64_json"].as_str()
    } else {
        v["candidates"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|c| c["content"]["parts"].as_array().into_iter().flatten())
            .filter(|p| {
                p["inlineData"]["mimeType"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("image/"))
            })
            .find_map(|p| p["inlineData"]["data"].as_str())
    };
    let bytes = decode(
        encoded.ok_or("Provider returned no image. Check the model or revise the prompt.")?,
    )?;
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
async fn request(
    s: &GenerationSettings,
    kind: &str,
    prompt: &str,
    secret: &str,
) -> Result<(Vec<u8>, &'static str), String> {
    if kind == "voice" && s.voice.trim().is_empty() {
        return Err("Configure your voice ID in Connections before generating narration".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(240))
        .connect_timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot initialize generation client")?;
    let mut adc_token = String::new();
    let mut project = String::new();
    if kind == "image" && s.image_provider == "google_adc" {
        let adc = crate::credentials::adc()?;
        project = if s.google_project.is_empty() {
            adc["quota_project_id"].as_str().unwrap_or_default().into()
        } else {
            s.google_project.clone()
        };
        if project.is_empty()
            || !project
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err("Enter your Google Cloud project ID in Settings".into());
        }
        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", adc["client_id"].as_str().unwrap()),
                ("client_secret", adc["client_secret"].as_str().unwrap()),
                ("refresh_token", adc["refresh_token"].as_str().unwrap()),
            ])
            .send()
            .await
            .map_err(|_| "Cannot refresh Google Cloud login; check your network")?;
        if !response.status().is_success() {
            return Err("Google Cloud login expired or was rejected. Run gcloud auth application-default login.".into());
        }
        let v: Value = response
            .json()
            .await
            .map_err(|_| "Invalid Google Cloud authentication response")?;
        adc_token = v["access_token"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("Google returned no access token")?
            .into();
    }
    let req = if kind == "voice" {
        client
            .post("https://openspeech.bytedance.com/api/v3/tts/unidirectional")
            .header("X-Api-Key", secret)
            .header("X-Api-Resource-Id", resource(&s.voice))
            .json(&voice_payload(s, prompt))
    } else if s.image_provider == "doubao" {
        client
            .post("https://ark.cn-beijing.volces.com/api/v3/images/generations")
            .bearer_auth(secret)
            .json(&image_payload(s, prompt))
    } else if s.image_provider == "google_adc" {
        client.post(format!("https://aiplatform.googleapis.com/v1/projects/{project}/locations/global/publishers/google/models/{}:generateContent", s.image_model))
            .bearer_auth(&adc_token).header("x-goog-user-project", &project).json(&image_payload(s, prompt))
    } else {
        client
            .post(format!(
                "https://aiplatform.googleapis.com/v1/publishers/google/models/{}:generateContent",
                s.image_model
            ))
            .header("x-goog-api-key", secret)
            .json(&image_payload(s, prompt))
    };
    let mut response = req
        .send()
        .await
        .map_err(|_| "Generation connection failed or timed out; check your network and retry")?;
    let status = response.status();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Generation stream interrupted")?
    {
        if body.len() + chunk.len() > LIMIT {
            return Err("Provider response exceeds size limit".into());
        }
        body.extend(chunk);
    }
    if !status.is_success() {
        let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let detail = parsed["error"]["message"]
            .as_str()
            .or_else(|| parsed["message"].as_str())
            .unwrap_or("Check credentials, model access, and quota in Settings.");
        let detail = crate::tools::redact(
            &crate::tools::redact(detail, Some(secret)),
            Some(&adc_token),
        )
        .chars()
        .take(400)
        .collect::<String>();
        return Err(format!("Provider HTTP {}: {detail}", status.as_u16()));
    }
    if kind == "voice" {
        Ok((voice_response(&body)?, "wav"))
    } else {
        image_response(&body, &s.image_provider)
    }
}
pub fn key_name(s: &GenerationSettings, kind: &str) -> &'static str {
    if kind == "voice" {
        "BYTEDANCE_TTS_API_KEY"
    } else if s.image_provider == "doubao" {
        "VOLCENGINE_API_KEY"
    } else if s.image_provider == "google_adc" {
        "GOOGLE_ADC"
    } else {
        "GOOGLE_CLOUD_API_KEY"
    }
}
pub fn secret(s: &GenerationSettings, kind: &str) -> Result<String, String> {
    if key_name(s, kind) == "GOOGLE_ADC" {
        crate::credentials::adc()?;
        Ok(String::new())
    } else {
        crate::credentials::key(key_name(s, kind))
    }
}
pub fn launch(
    app: tauri::AppHandle,
    mut w: Workspace,
    run: Run,
    s: GenerationSettings,
    job: Arc<Job>,
    prompt: String,
    secret: String,
) {
    tauri::async_runtime::spawn(async move {
        let result = tokio::select! {
            r = request(&s, &run.stage, &prompt, &secret) => r,
            _ = async { loop { tokio::time::sleep(Duration::from_millis(100)).await; if job.check().is_err() { break; } } } => Err("Run cancelled".into()),
        };
        // File writes and manifest changes stay on the blocking pool.
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let studio = app.state::<Studio>();
            let dir = PathBuf::from(&w.root).join("assets").join(&run.id);
            let result = result.and_then(|(bytes, ext)| {
                job.check()?;
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let path = dir.join(format!("{}.{}", run.stage, ext));
                std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
                let mut m = media::inventory(&path, "generated")?;
                if run.stage == "voice" { m.has_audio = true; m.duration = (bytes.len() - 44) as f64 / 96000.; m.transcript = Some(prompt.clone()); }
                job.check()?;
                let artifact = Artifact {
                    id: id(), kind: run.stage.clone(), title: prompt.chars().take(60).collect(),
                    path: path.to_string_lossy().into(), created: now(), run_id: run.id.clone(),
                    data: json!({"prompt":prompt,"provider":if run.stage=="voice"{"bytedance"}else{&s.image_provider},
                        "model":if run.stage=="voice"{&s.voice_model}else{&s.image_model},"duration":m.duration,
                        "voice":if run.stage=="voice"{Some(&s.voice)}else{None},"mediaId":m.id}),
                };
                Ok((artifact, m))
            });
            let (status, message, artifact_id) = match result {
                Ok((a, m)) => { let aid = a.id.clone(); w.artifacts.push(a); w.media.push(m); w.revision += 1; ("succeeded", "Complete".into(), Some(aid)) },
                Err(e) => { let _ = std::fs::remove_dir_all(&dir); (if job.check().is_err(){"cancelled"}else{"failed"}, e, None) }
            };
            if let Some(r) = w.runs.iter_mut().find(|r| r.id == run.id) {
                r.status = status.into(); r.message = message; r.finished = Some(now());
                r.artifact_id = artifact_id; r.logs = vec![format!("{} · {}", r.stage, r.message)];
            }
            w.updated = now();
            let _guard = studio.gate.lock().unwrap();
            if let Err(e) = studio.save(&w) { let _ = app.emit("run-log", json!({"workspaceId":w.id,"runId":run.id,"line":format!("Could not save generation: {e}")})); }
            studio.jobs.lock().unwrap().remove(&w.id);
            let _ = app.emit("workspace-updated", json!({"workspaceId":w.id}));
        }).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voice_stream_requires_success_and_valid_audio() {
        let body = format!(
            "{{\"code\":0,\"data\":\"{}\"}}\n{{\"code\":20000000}}",
            STANDARD.encode([0, 1, 2, 3])
        );
        let audio = voice_response(body.as_bytes()).unwrap();
        assert_eq!(&audio[..4], b"RIFF");
        assert_eq!(audio.len(), 48);
        assert!(voice_response(b"{\"code\":0,\"data\":\"AAE=\"}").is_err());
        assert!(
            voice_response(b"{\"code\":55000000,\"message\":\"secret\"}")
                .unwrap_err()
                .contains("55000000")
        );
        assert!(
            !voice_response(b"{\"code\":55000000,\"message\":\"secret\"}")
                .unwrap_err()
                .contains("secret")
        );
        assert_eq!(resource("S_fixture"), "seed-icl-2.0");
        assert_eq!(resource("zh_male_x_moon_bigtts"), "seed-tts-1.0");
    }
    #[test]
    fn image_parsing_and_provider_payloads() {
        let data = STANDARD.encode(b"\x89PNG\r\n\x1a\nfixture");
        let seed = json!({"data":[{"b64_json":data}]});
        assert_eq!(
            image_response(&serde_json::to_vec(&seed).unwrap(), "doubao")
                .unwrap()
                .1,
            "png"
        );
        let google = json!({"candidates":[{"content":{"parts":[{"text":"Description"},{"inlineData":{"mimeType":"image/png","data":data}}]}}]});
        assert!(image_response(&serde_json::to_vec(&google).unwrap(), "google").is_ok());
        assert!(image_response(b"{\"candidates\":[]}", "google").is_err());
        let mut s = GenerationSettings::default();
        assert_eq!(image_payload(&s, "test")["response_format"], "b64_json");
        s.image_provider = "google".into();
        s.image_size = "1536x2560".into();
        assert_eq!(
            image_payload(&s, "test")["generationConfig"]["imageConfig"]["aspectRatio"],
            "9:16"
        );
        s.image_model = "bad/model?key=secret".into();
        assert!(validate(&s).is_err());
    }
    #[test]
    fn old_settings_get_generation_defaults() {
        let mut v = serde_json::to_value(Settings::default()).unwrap();
        v.as_object_mut().unwrap().remove("generation");
        assert_eq!(
            serde_json::from_value::<Settings>(v)
                .unwrap()
                .generation
                .voice,
            ""
        );
    }
    #[test]
    #[ignore = "Requires provider key environment variables; makes small billable generation requests"]
    fn live_media_providers() {
        let mut s = GenerationSettings::default();
        s.voice = std::env::var("FRAMECRAFT_TEST_VOICE").unwrap_or_default();
        if let Ok(provider) = std::env::var("FRAMECRAFT_TEST_IMAGE_PROVIDER") {
            s.image_provider = provider;
            s.image_model = "gemini-3.1-flash-image".into();
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for (kind, prompt) in [
            ("voice", "你好，这是配音接口测试。"),
            (
                "image",
                "A single blue circle on a plain white background, no text",
            ),
        ] {
            if std::env::var("FRAMECRAFT_TEST_KIND").is_ok_and(|v| v != kind) {
                continue;
            }
            let secret = if s.image_provider == "google_adc" && kind == "image" {
                String::new()
            } else {
                std::env::var(key_name(&s, kind)).expect("Set the provider key in the test process environment; tests never read Keychain")
            };
            let (bytes, ext) = runtime
                .block_on(request(&s, kind, prompt, &secret))
                .unwrap();
            let path = std::env::temp_dir().join(format!("framecraft-provider-test.{ext}"));
            std::fs::write(&path, bytes).unwrap();
            println!("{kind} saved to {}", path.display());
        }
    }
}
