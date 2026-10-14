use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub topic: String,
    pub direction: String,
    pub audience: String,
    pub platform: String,
    pub language: String,
    pub duration: u32,
    pub aspect: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub role: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    pub bytes: u64,
    #[serde(default)]
    pub modified: u64,
    pub frames: Vec<Frame>,
    pub transcript: Option<String>,
    pub warning: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Frame {
    pub time: f64,
    pub path: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub origin: String,
}
pub fn builtins() -> Vec<Skill> {
    [
        (
            "reference-study",
            "Reference study",
            "Visual language, story beats, and evidence",
            include_str!("../../skills/reference-study.md"),
        ),
        (
            "story-adaptation",
            "Story adaptation",
            "Original scripts shaped by your topic",
            include_str!("../../skills/story-adaptation.md"),
        ),
        (
            "footage-editing",
            "Footage editing",
            "Source-aware cuts and a reviewable timeline",
            include_str!("../../skills/footage-editing.md"),
        ),
        (
            "social-pacing",
            "Social pacing",
            "Hooks, rhythm, and purposeful captions",
            include_str!("../../skills/social-pacing.md"),
        ),
    ]
    .into_iter()
    .map(|(id, name, description, instructions)| Skill {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        instructions: instructions.into(),
        origin: "Built-in".into(),
    })
    .collect()
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub path: String,
    pub created: u64,
    pub run_id: String,
    pub data: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub stage: String,
    pub status: String,
    pub started: u64,
    pub finished: Option<u64>,
    pub message: String,
    pub logs: Vec<String>,
    pub artifact_id: Option<String>,
    pub input_revision: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub root: String,
    pub created: u64,
    pub updated: u64,
    pub revision: u64,
    pub brief: Brief,
    pub reference: Option<Media>,
    pub raw_folder: Option<String>,
    pub media: Vec<Media>,
    pub skills: Vec<Skill>,
    pub selected_skills: Vec<String>,
    pub artifacts: Vec<Artifact>,
    pub runs: Vec<Run>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub id: String,
    pub name: String,
    pub root: String,
    pub updated: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub mode: String,
    pub base_url: String,
    pub model: String,
    pub claude_path: String,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub whisper_path: String,
    #[serde(default)]
    pub generation: GenerationSettings,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: "local".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "sonnet".into(),
            claude_path: String::new(),
            ffmpeg_path: String::new(),
            ffprobe_path: String::new(),
            whisper_path: String::new(),
            generation: GenerationSettings::default(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppData {
    pub settings: Settings,
    pub recents: Vec<Recent>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub source_id: String,
    pub start: f64,
    pub end: f64,
    pub beat: String,
    pub reason: String,
    pub caption: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPlan {
    pub title: String,
    pub summary: String,
    pub warnings: Vec<String>,
    pub clips: Vec<Clip>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub claude: Option<String>,
    pub ffmpeg: Option<String>,
    pub ffprobe: Option<String>,
    pub whisper: Option<String>,
    pub key_saved: bool,
    pub errors: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    pub path: Option<String>,
    pub version: Option<String>,
    pub logged_in: bool,
    pub auth_method: Option<String>,
    pub account: Option<String>,
    pub checked: u64,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GenerationSettings {
    pub voice: String,
    pub voice_model: String,
    pub speech_rate: i32,
    pub emotion: String,
    pub image_provider: String,
    pub image_model: String,
    pub image_size: String,
    pub google_project: String,
}
impl Default for GenerationSettings {
    fn default() -> Self {
        Self {
            voice: String::new(),
            voice_model: "seed-tts-2.0-standard".into(),
            speech_rate: 0,
            emotion: "neutral".into(),
            image_provider: "doubao".into(),
            image_model: "doubao-seedream-5-0-pro-260628".into(),
            image_size: "2048x2048".into(),
            google_project: String::new(),
        }
    }
}
