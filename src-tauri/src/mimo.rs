//! MiMo's text, speech synthesis, and recognition share the chat-completions API.
use crate::{credentials, tools::Job};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::time::Duration;

pub async fn send(
    request: reqwest::RequestBuilder,
    secret: &str,
    job: &Job,
) -> Result<Vec<u8>, String> {
    let fetch = async {
        let mut response = request
            .send()
            .await
            .map_err(|_| "Provider connection failed or timed out")?;
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Provider response interrupted")?
        {
            if body.len() + chunk.len() > 96 * 1024 * 1024 {
                return Err("Provider response exceeds 96 MB".into());
            }
            body.extend(chunk);
        }
        if !status.is_success() {
            let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let detail = value["error"]["message"]
                .as_str()
                .or_else(|| value["message"].as_str())
                .unwrap_or("Check the API key, model access, and quota in Settings.");
            return Err(format!(
                "Provider HTTP {}: {}",
                status.as_u16(),
                crate::tools::redact(detail, Some(secret))
                    .chars()
                    .take(500)
                    .collect::<String>()
            ));
        }
        Ok(body)
    };
    tokio::select! {
        result = fetch => result,
        _ = async { loop { tokio::time::sleep(Duration::from_millis(100)).await; if job.check().is_err() { break; } } } => Err("Run cancelled".into()),
    }
}
pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(240))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot initialize provider client".into())
}
pub async fn chat(payload: Value, job: &Job) -> Result<Value, String> {
    let secret = credentials::key("MIMO_API_KEY")?;
    let request = client()?
        .post("https://api.xiaomimimo.com/v1/chat/completions")
        .header("api-key", &secret)
        .json(&payload);
    serde_json::from_slice(&send(request, &secret, job).await?)
        .map_err(|_| "MiMo returned invalid JSON".into())
}
pub fn speech_payload(voice: &str, style: &str, text: &str) -> Value {
    json!({"model":"mimo-v2.5-tts","messages":[{"role":"user","content":style},
        {"role":"assistant","content":text}],"audio":{"format":"wav","voice":voice}})
}
pub fn speech_response(value: &Value) -> Result<Vec<u8>, String> {
    let encoded = value["choices"][0]["message"]["audio"]["data"]
        .as_str()
        .ok_or("MiMo returned no speech audio")?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "MiMo returned invalid audio data")?;
    if bytes.len() < 44 || !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("MiMo returned an invalid WAV file".into());
    }
    Ok(bytes)
}
pub async fn transcribe(audio: &[u8], job: &Job) -> Result<String, String> {
    if audio.len() > 12 * 1024 * 1024 {
        return Err("Audio segment exceeds 12 MB".into());
    }
    let value = chat(json!({"model":"mimo-v2.5-asr","messages":[{"role":"user","content":[
        {"type":"input_audio","input_audio":{"data":format!("data:audio/mpeg;base64,{}",STANDARD.encode(audio))}}
    ]}],"asr_options":{"language":"auto"}}),job).await?;
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or("MiMo returned no transcript".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn speech_uses_assistant_text_and_rejects_invalid_audio() {
        let payload = speech_payload("Mia", "Calm", "Hello");
        assert_eq!(payload["messages"][1]["role"], "assistant");
        assert_eq!(payload["audio"]["format"], "wav");
        assert!(
            speech_response(&json!({"choices":[{"message":{"audio":{"data":"YWJj"}}}]})).is_err()
        );
        assert!(speech_response(&json!({"choices":[]})).is_err());
    }
}

#[cfg(test)]
mod live {
    use super::*;
    #[test]
    #[ignore = "Real MiMo TTS to ASR roundtrip; makes provider requests"]
    fn real_speech_roundtrip() {
        let job = Job::default();
        let value = tauri::async_runtime::block_on(chat(
            speech_payload(
                "mimo_default",
                "Clear natural Mandarin",
                "你好，欢迎使用视频创作助手。",
            ),
            &job,
        ))
        .unwrap();
        let dir = std::env::temp_dir().join(format!("framecraft-speech-{}", crate::model::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("voice.wav"), speech_response(&value).unwrap()).unwrap();
        let mut cmd = crate::tools::command(&crate::tools::required("ffmpeg", "").unwrap());
        cmd.args(["-v", "error", "-y", "-i"])
            .arg(dir.join("voice.wav"))
            .args(["-ar", "24000", "-ac", "1", "-b:a", "64k"])
            .arg(dir.join("voice.mp3"));
        assert!(crate::tools::execute(cmd, &job, 30, |_| {}).unwrap().0);
        let text = tauri::async_runtime::block_on(transcribe(
            &std::fs::read(dir.join("voice.mp3")).unwrap(),
            &job,
        ))
        .unwrap();
        assert!(text.contains("视频创作助手"), "{text}");
        println!("MiMo recognized: {text}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
