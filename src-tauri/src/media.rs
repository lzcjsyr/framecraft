use crate::{model::*, tools::*};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
pub fn kind(p: &Path) -> Option<&'static str> {
    let e = p.extension()?.to_str()?.to_lowercase();
    match e.as_str() {
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" => Some("video"),
        "wav" | "mp3" | "m4a" | "aac" | "flac" => Some("audio"),
        "png" | "jpg" | "jpeg" | "webp" => Some("image"),
        _ => None,
    }
}
pub fn inventory(p: &Path, role: &str) -> Result<Media, String> {
    let p = p.canonicalize().map_err(|e| e.to_string())?;
    let k = kind(&p).ok_or("Unsupported media. Use MP4, MOV, MKV, audio, or image files.")?;
    let meta = fs::metadata(&p).map_err(|e| e.to_string())?;
    let bytes = meta.len();
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut transcript = None;
    for ext in ["srt", "vtt", "txt"] {
        let sidecar = p.with_extension(ext);
        if sidecar.is_file() {
            transcript = Some(small_text(&sidecar)?);
            break;
        }
    }
    Ok(Media {
        id: id(),
        path: p.to_string_lossy().into(),
        name: p.file_name().unwrap().to_string_lossy().into(),
        kind: k.into(),
        role: role.into(),
        duration: 0.,
        width: 0,
        height: 0,
        has_audio: false,
        bytes,
        modified,
        frames: vec![],
        transcript,
        warning: None,
    })
}
pub fn scan(folder: &Path) -> Result<Vec<Media>, String> {
    fn walk(p: &Path, files: &mut Vec<PathBuf>, depth: usize) -> Result<(), String> {
        if depth > 12 {
            return Err("Folder nesting exceeds 12 levels".into());
        }
        for e in fs::read_dir(p).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            let ty = e.file_type().map_err(|e| e.to_string())?;
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.')
                || [
                    "exports",
                    "assets",
                    "analysis",
                    "scripts",
                    "edits",
                    "node_modules",
                ]
                .contains(&name.as_str())
            {
                continue;
            }
            if ty.is_dir() {
                walk(&e.path(), files, depth + 1)?
            } else if ty.is_file() && kind(&e.path()).is_some() {
                files.push(e.path());
                if files.len() > 500 {
                    return Err("This folder has more than 500 media files. Choose a smaller raw-material folder.".into());
                }
            }
        }
        Ok(())
    }
    let mut files = vec![];
    walk(folder, &mut files, 0)?;
    files.sort();
    files.iter().map(|p| inventory(p, "raw")).collect()
}
pub fn verify_source(m: &Media) -> Result<(), String> {
    let meta = fs::metadata(&m.path).map_err(|_| format!("Source is missing: {}", m.name))?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    if meta.len() != m.bytes || modified != m.modified {
        return Err(format!("{} changed on disk. Re-import the reference or refresh the raw folder before continuing.",m.name));
    }
    Ok(())
}
pub fn prepare(
    m: &mut Media,
    root: &Path,
    s: &Settings,
    job: &Job,
    mut log: impl FnMut(String),
) -> Result<(), String> {
    job.check()?;
    verify_source(m)?;
    if m.kind == "image" {
        m.frames = vec![Frame {
            time: 0.,
            path: m.path.clone(),
        }];
        return Ok(());
    }
    let ffprobe = required("ffprobe", &s.ffprobe_path)?;
    let mut c = command(&ffprobe);
    c.args([
        "-v",
        "error",
        "-show_format",
        "-show_streams",
        "-of",
        "json",
        &m.path,
    ]);
    let (ok, out, err) = execute(c, job, 60, |_| {})?;
    if !ok {
        return Err(format!("Cannot inspect {}: {}", m.name, err));
    }
    let v: Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    m.duration = v["format"]["duration"]
        .as_str()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.);
    if !m.duration.is_finite() || m.duration <= 0. {
        return Err(format!("{} has no usable duration", m.name));
    }
    let streams = v["streams"].as_array().ok_or("No media streams")?;
    m.has_audio = streams.iter().any(|x| x["codec_type"] == "audio");
    if let Some(video) = streams.iter().find(|x| x["codec_type"] == "video") {
        m.width = video["width"].as_u64().unwrap_or(0) as u32;
        m.height = video["height"].as_u64().unwrap_or(0) as u32;
    }
    let cache = root.join(".framecraft/cache").join(&m.id);
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let ffmpeg = required("ffmpeg", &s.ffmpeg_path)?;
    if m.kind == "video" {
        let count = if m.role == "reference" { 16 } else { 6 };
        m.frames.clear();
        for i in 0..count {
            let time = m.duration * (i as f64 + 0.5) / count as f64;
            let frame = cache.join(format!("frame-{i:02}.jpg"));
            let mut c = command(&ffmpeg);
            c.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-ss",
                &time.to_string(),
                "-i",
                &m.path,
                "-frames:v",
                "1",
                "-vf",
                "scale=640:-2",
                "-q:v",
                "3",
            ]);
            c.arg(&frame);
            let (ok, _, err) = execute(c, job, 60, |_| {})?;
            if !ok {
                return Err(format!("Cannot sample {}: {err}", m.name));
            }
            if frame.is_file() {
                m.frames.push(Frame {
                    time,
                    path: frame.to_string_lossy().into(),
                });
            }
        }
    }
    if m.has_audio && m.transcript.is_none() {
        if let Some(whisper) = resolve("whisper", &s.whisper_path) {
            log(format!("Transcribing {} with local Whisper", m.name));
            let wav = cache.join("speech.wav");
            let mut c = command(&ffmpeg);
            c.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                &m.path,
                "-vn",
                "-ar",
                "16000",
                "-ac",
                "1",
            ]);
            c.arg(&wav);
            let (ok, _, err) = execute(c, job, 120, |_| {})?;
            if !ok {
                return Err(err);
            }
            let mut c = command(&whisper);
            c.arg(&wav)
                .args(["--model", "base", "--output_format", "srt", "--output_dir"])
                .arg(&cache)
                .args(["--fp16", "False"]);
            let (ok, _, err) = execute(c, job, 1800, |_| {})?;
            if ok {
                m.transcript = Some(small_text(&cache.join("speech.srt"))?)
            } else {
                m.warning = Some(format!(
                    "Local transcription failed: {}",
                    err.chars().take(200).collect::<String>()
                ));
            }
        } else {
            m.warning=Some("No transcript: speech and audio content cannot be verified. Attach SRT/TXT or configure local Whisper.".into());
        }
    }
    log(format!(
        "Indexed {} · {:.1}s · {} frames{}",
        m.name,
        m.duration,
        m.frames.len(),
        if m.transcript.is_some() {
            " · transcript ready"
        } else {
            ""
        }
    ));
    Ok(())
}
pub fn validate_plan(plan: &EditPlan, w: &Workspace) -> Result<f64, String> {
    if plan.clips.is_empty() || plan.clips.len() > 80 {
        return Err("Edit plan needs 1–80 clips".into());
    }
    let mut duration = 0.;
    for (i, c) in plan.clips.iter().enumerate() {
        let m = w
            .media
            .iter()
            .find(|m| m.id == c.source_id && m.kind == "video")
            .ok_or_else(|| format!("Clip {} references an unknown video", i + 1))?;
        if !c.start.is_finite()
            || !c.end.is_finite()
            || c.start < 0.
            || c.end > m.duration + 0.025
            || c.end - c.start < 0.15
        {
            return Err(format!(
                "Clip {} is outside {} or shorter than 0.15 seconds",
                i + 1,
                m.name
            ));
        }
        if c.caption.chars().count() > 300 {
            return Err("A clip caption exceeds 300 characters".into());
        }
        duration += c.end - c.start;
    }
    if duration > 1800. {
        return Err("Timeline exceeds 30 minutes".into());
    }
    Ok(duration)
}
fn srt_time(t: f64) -> String {
    let ms = (t * 1000.).round() as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3600000,
        (ms / 60000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}
fn clip_duration(c: &Clip) -> f64 {
    ((c.end - c.start) * 30.0).round() / 30.0
}
pub fn captions(plan: &EditPlan) -> String {
    let mut t = 0.;
    let mut count = 0;
    let mut out = String::new();
    for c in &plan.clips {
        let end = t + clip_duration(c);
        if !c.caption.trim().is_empty() {
            count += 1;
            out.push_str(&format!(
                "{count}\n{} --> {}\n{}\n\n",
                srt_time(t),
                srt_time(end),
                c.caption.trim().replace("\r", "")
            ));
        }
        t = end;
    }
    out
}
pub fn render_backend() -> &'static str {
    if cfg!(target_os = "macos") {
        "h264_videotoolbox"
    } else {
        "libx264"
    }
}
fn encode_args() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &[
            "-c:v",
            "h264_videotoolbox",
            "-allow_sw",
            "0",
            "-b:v",
            "12M",
            "-profile:v",
            "high",
            "-g",
            "60",
            "-bf",
            "0",
        ]
    } else {
        &["-c:v", "libx264", "-preset", "fast", "-crf", "20"]
    }
}
fn render_filter(width: u32, height: u32, hardware_scale: bool) -> String {
    let scale = if hardware_scale {
        format!("format=nv12,hwupload,scale_vt=w='trunc(min({width}/iw,{height}/ih)*iw/2)*2':h='trunc(min({width}/iw,{height}/ih)*ih/2)*2',hwdownload,format=nv12")
    } else {
        format!("scale={width}:{height}:force_original_aspect_ratio=decrease")
    };
    format!("{scale},pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps=30,format=yuv420p")
}
pub fn render(
    w: &Workspace,
    plan: &EditPlan,
    run_id: &str,
    s: &Settings,
    job: &Job,
    mut log: impl FnMut(String),
) -> Result<Value, String> {
    validate_plan(plan, w)?;
    let duration: f64 = plan.clips.iter().map(clip_duration).sum();
    let root = PathBuf::from(&w.root);
    let dir = root.join("exports").join(run_id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let work = root.join(".framecraft/runs").join(run_id).join("render");
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let ffmpeg = required("ffmpeg", &s.ffmpeg_path)?;
    let (width, height) = match w.brief.aspect.as_str() {
        "16:9" => (1920, 1080),
        "1:1" => (1080, 1080),
        _ => (1080, 1920),
    };
    let mac_hardware = cfg!(target_os = "macos");
    let hardware_scale = if mac_hardware {
        let mut probe = command(&ffmpeg);
        probe.args(["-hide_banner", "-encoders"]);
        let (ok, out, _) = execute(probe, job, 15, |_| {})?;
        if !ok || !out.split_whitespace().any(|v| v == "h264_videotoolbox") {
            return Err("FFmpeg needs VideoToolbox support for Mac hardware rendering. Install a current macOS FFmpeg build; CPU encoding is disabled.".into());
        }
        let mut probe = command(&ffmpeg);
        probe.args(["-hide_banner", "-filters"]);
        let (ok, out, _) = execute(probe, job, 15, |_| {})?;
        ok && out.split_whitespace().any(|v| v == "scale_vt")
    } else {
        false
    };
    log(format!(
        "Render backend: {} · {}",
        render_backend(),
        if mac_hardware {
            "Apple hardware encoding required; software encoding disabled"
        } else {
            "software encoding"
        }
    ));
    if mac_hardware {
        log(format!(
            "VideoToolbox decode requested · {}",
            if hardware_scale {
                "VideoToolbox scaling"
            } else {
                "CPU frame fitting (scale_vt unavailable)"
            }
        ));
    }
    let vf = render_filter(width, height, hardware_scale);
    let mut list = String::new();
    for (i, c) in plan.clips.iter().enumerate() {
        job.check()?;
        let m = w.media.iter().find(|m| m.id == c.source_id).unwrap();
        verify_source(m)?;
        let seconds = clip_duration(c);
        let out = work.join(format!("clip-{i:03}.mov"));
        let mut cmd = command(&ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        if mac_hardware {
            if hardware_scale {
                cmd.args([
                    "-init_hw_device",
                    "videotoolbox=vt",
                    "-filter_hw_device",
                    "vt",
                ]);
            }
            cmd.args(["-hwaccel", "videotoolbox"]);
        }
        cmd.args(["-ss", &c.start.to_string(), "-i", &m.path]);
        if !m.has_audio {
            cmd.args(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo"]);
        }
        cmd.args([
            "-t",
            &seconds.to_string(),
            "-map",
            "0:v:0",
            "-map",
            if m.has_audio { "0:a:0" } else { "1:a:0" },
            "-vf",
            &vf,
            "-af",
            &format!("aresample=48000,apad,atrim=duration={seconds}"),
        ]);
        cmd.args(encode_args());
        cmd.args([
            "-c:a",
            "pcm_s16le",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-video_track_timescale",
            "90000",
            "-movflags",
            "+faststart",
        ])
        .arg(&out);
        log(format!(
            "Rendering clip {}/{} · {} [{:.2}–{:.2}s]",
            i + 1,
            plan.clips.len(),
            m.name,
            c.start,
            c.end
        ));
        let (ok, _, err) = execute(cmd, job, 1800, |_| {})?;
        if !ok {
            return Err(format!(
                "Clip render failed using {}: {err}",
                render_backend()
            ));
        }
        list.push_str(&format!("file 'clip-{i:03}.mov'\n"));
    }
    let concat = work.join("concat.txt");
    fs::write(&concat, list).map_err(|e| e.to_string())?;
    let output = dir.join("video.mp4");
    let partial = dir.join("video.partial.mp4");
    let mut cmd = command(&ffmpeg);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "concat",
        "-safe",
        "1",
        "-i",
    ])
    .arg(&concat)
    .args(["-c:v", "copy", "-c:a", "aac", "-movflags", "+faststart"])
    .arg(&partial);
    log("Combining clips and writing the caption sidecar".into());
    let (ok, _, err) = execute(cmd, job, 1800, |_| {})?;
    if !ok {
        return Err(format!("Assembly failed: {err}"));
    }
    job.check()?;
    fs::rename(partial, &output).map_err(|e| e.to_string())?;
    let sub = dir.join("captions.srt");
    fs::write(&sub, captions(plan)).map_err(|e| e.to_string())?;
    crate::store::write(&dir.join("edit-plan.json"), plan)?;
    Ok(
        serde_json::json!({"video":output.to_string_lossy(),"captions":sub.to_string_lossy(),"duration":duration,"width":width,"height":height,"clipCount":plan.clips.len(),"captionMode":"sidecar","encoder":render_backend(),"hardwareEncoding":mac_hardware,"hardwareScaling":hardware_scale,"plan":plan}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captions_use_output_timing() {
        let p = EditPlan {
            title: "".into(),
            summary: "".into(),
            warnings: vec![],
            clips: vec![
                Clip {
                    source_id: "x".into(),
                    start: 10.,
                    end: 12.5,
                    beat: "".into(),
                    reason: "".into(),
                    caption: "Hi".into(),
                },
                Clip {
                    source_id: "x".into(),
                    start: 20.,
                    end: 22.,
                    beat: "".into(),
                    reason: "".into(),
                    caption: "Next".into(),
                },
            ],
        };
        assert!(captions(&p).contains("00:00:02,500 --> 00:00:04,500"));
    }
    #[test]
    fn unsupported_extension_is_ignored() {
        assert_eq!(kind(Path::new("script.exe")), None);
        assert_eq!(kind(Path::new("Take.MOV")), Some("video"));
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use crate::store;
    #[test]
    #[ignore = "Requires local FFmpeg and FFprobe; creates and removes synthetic video fixtures"]
    fn real_media_pipeline() {
        let root = std::env::temp_dir().join(format!("framecraft-media-{}", id()));
        fs::create_dir_all(root.join("raw")).unwrap();
        let ffmpeg = required("ffmpeg", "").unwrap();
        for (name, color, audio) in [
            ("reference", "red", true),
            ("raw/take one", "blue", true),
            ("raw/take two", "green", false),
        ] {
            let path = root.join(format!("{name}.mp4"));
            let mut c = command(&ffmpeg);
            c.args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &format!("color=c={color}:s=320x180:r=30:d=3"),
            ]);
            if audio {
                c.args(["-f", "lavfi", "-i", "sine=frequency=440:duration=3"]);
            }
            c.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-t", "3"]);
            if audio {
                c.args(["-c:a", "aac"]);
            }
            c.arg(&path);
            let (ok, _, err) = execute(c, &Job::default(), 30, |_| {}).unwrap();
            assert!(ok, "{err}");
            fs::write(
                path.with_extension("srt"),
                "1\n00:00:00,000 --> 00:00:03,000\nSynthetic test transcript.\n",
            )
            .unwrap();
        }
        let mut w = store::create("Test workspace".into(), root.to_string_lossy().into()).unwrap();
        w.brief.aspect = "16:9".into();
        w.reference = Some(inventory(&root.join("reference.mp4"), "reference").unwrap());
        w.media = scan(&root.join("raw")).unwrap();
        let s = Settings::default();
        let job = Job::default();
        prepare(w.reference.as_mut().unwrap(), &root, &s, &job, |_| {}).unwrap();
        for m in &mut w.media {
            prepare(m, &root, &s, &job, |_| {}).unwrap();
        }
        assert_eq!(w.reference.as_ref().unwrap().frames.len(), 16);
        assert_eq!(w.media.len(), 2);
        assert!(w
            .media
            .iter()
            .all(|m| m.duration > 2.9 && m.frames.len() == 6));
        let plan = EditPlan {
            title: "Reviewed test".into(),
            summary: "Mixed audio and silent source clips".into(),
            warnings: vec![],
            clips: vec![
                Clip {
                    source_id: w.media[0].id.clone(),
                    start: 0.25,
                    end: 1.75,
                    beat: "Hook".into(),
                    reason: "Test".into(),
                    caption: "First caption".into(),
                },
                Clip {
                    source_id: w.media[1].id.clone(),
                    start: 0.5,
                    end: 2.,
                    beat: "Finish".into(),
                    reason: "Test".into(),
                    caption: "Second caption".into(),
                },
            ],
        };
        let data = render(&w, &plan, "fixture-export", &s, &job, |_| {}).unwrap();
        if cfg!(target_os = "macos") {
            assert_eq!(data["encoder"], "h264_videotoolbox");
            assert_eq!(data["hardwareEncoding"], true);
        }
        let output = data["video"].as_str().unwrap();
        assert!(Path::new(output).is_file());
        let mut c = command(&required("ffprobe", "").unwrap());
        c.args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
            output,
        ]);
        let (_, out, _) = execute(c, &job, 30, |_| {}).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let duration = v["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((duration - 3.).abs() < 0.2, "{duration}");
        assert!(v["streams"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["codec_type"] == "audio"));
        assert!(fs::read_to_string(data["captions"].as_str().unwrap())
            .unwrap()
            .contains("00:00:01,500 --> 00:00:03,000"));
        let mut invalid = plan.clone();
        invalid.clips[0].end = 99.;
        assert!(validate_plan(&invalid, &w).is_err());
        invalid.clips[0].end = 1.;
        invalid.clips[0].source_id = "missing".into();
        assert!(validate_plan(&invalid, &w).is_err());
        let input = Path::new(&w.media[0].path);
        let file = fs::OpenOptions::new().append(true).open(input).unwrap();
        use std::io::Write;
        writeln!(&file, "changed").unwrap();
        assert!(verify_source(&w.media[0]).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
