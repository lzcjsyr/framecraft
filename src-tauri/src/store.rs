use crate::model::*;
use crate::tools::Job;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct Studio {
    pub storage: PathBuf,
    pub gate: Mutex<()>,
    pub jobs: Mutex<HashMap<String, Arc<Job>>>,
}
pub fn read<T: DeserializeOwned>(p: &Path) -> Result<T, String> {
    let bytes = fs::read(p).map_err(|e| format!("Cannot read {}: {e}", p.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid {}: {e}", p.display()))
}
pub fn write<T: Serialize>(p: &Path, value: &T) -> Result<(), String> {
    fs::create_dir_all(p.parent().ok_or("Invalid save location")?).map_err(|e| e.to_string())?;
    let temp = p.with_extension(format!("{}.tmp", id()));
    fs::write(
        &temp,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&temp, p).map_err(|e| {
        let _ = fs::remove_file(&temp);
        e.to_string()
    })
}
impl Studio {
    pub fn data(&self) -> Result<AppData, String> {
        let p = self.storage.join("settings.json");
        if p.exists() {
            read(&p)
        } else {
            Ok(AppData::default())
        }
    }
    pub fn save_data(&self, d: &AppData) -> Result<(), String> {
        write(&self.storage.join("settings.json"), d)
    }
    pub fn root(&self, id: &str) -> Result<PathBuf, String> {
        let d = self.data()?;
        let p = d
            .recents
            .iter()
            .find(|r| r.id == id)
            .ok_or("Workspace is not registered. Open its folder again.")?;
        PathBuf::from(&p.root).canonicalize().map_err(|_| {
            "Workspace folder is unavailable. Reconnect it using Add workspace → Open existing."
                .into()
        })
    }
    pub fn load(&self, id: &str) -> Result<Workspace, String> {
        let root = self.root(id)?;
        let w: Workspace = read(&root.join("workspace.json"))?;
        if w.schema_version != 1 || w.id != id {
            return Err("Unsupported workspace manifest".into());
        }
        Ok(w)
    }
    pub fn save(&self, w: &Workspace) -> Result<(), String> {
        let root = self.root(&w.id)?;
        write(&root.join("workspace.json"), w)?;
        let mut d = self.data()?;
        if let Some(r) = d.recents.iter_mut().find(|r| r.id == w.id) {
            r.name = w.name.clone();
            r.updated = w.updated;
        }
        self.save_data(&d)
    }
    pub fn idle(&self, id: &str) -> Result<(), String> {
        if self
            .jobs
            .lock()
            .map_err(|e| e.to_string())?
            .contains_key(id)
        {
            Err("This workspace is running. Stop the run before changing its inputs.".into())
        } else {
            Ok(())
        }
    }
    pub fn register(&self, mut w: Workspace, root: &Path) -> Result<Workspace, String> {
        w.root = root.to_string_lossy().into();
        for r in &mut w.runs {
            if r.status == "running" || r.status == "queued" {
                r.status = "interrupted".into();
                r.finished = Some(now());
                r.message = "App closed during this run. Start the stage again.".into();
            }
        }
        let mut d = self.data()?;
        d.recents.retain(|r| r.id != w.id);
        d.recents.insert(
            0,
            Recent {
                id: w.id.clone(),
                name: w.name.clone(),
                root: w.root.clone(),
                updated: now(),
            },
        );
        self.save_data(&d)?;
        self.save(&w)?;
        Ok(w)
    }
}
pub fn create(name: String, folder: String) -> Result<Workspace, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err("Workspace name must contain 1–120 characters".into());
    }
    let root = PathBuf::from(folder)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("Choose a folder".into());
    }
    if root.join("workspace.json").exists() {
        return Err("This folder already contains a workspace. Use Open existing.".into());
    }
    for dir in [
        "references",
        "analysis",
        "scripts",
        "edits",
        "exports",
        ".framecraft/cache",
        ".framecraft/runs",
        ".framecraft/skills",
    ] {
        fs::create_dir_all(root.join(dir)).map_err(|e| e.to_string())?;
    }
    let w = Workspace {
        schema_version: 1,
        id: id(),
        name: name.into(),
        root: root.to_string_lossy().into(),
        created: now(),
        updated: now(),
        revision: 0,
        brief: Brief {
            language: "简体中文".into(),
            platform: "Douyin / TikTok".into(),
            duration: 60,
            aspect: "9:16".into(),
            ..Default::default()
        },
        reference: None,
        raw_folder: None,
        media: vec![],
        skills: builtins(),
        selected_skills: vec![
            "reference-study".into(),
            "story-adaptation".into(),
            "footage-editing".into(),
        ],
        artifacts: vec![],
        runs: vec![],
    };
    write(&root.join("workspace.json"), &w)?;
    Ok(w)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_roundtrip_and_interruption_recovery() {
        let root = std::env::temp_dir().join(format!("framecraft-store-{}", id()));
        fs::create_dir_all(&root).unwrap();
        let studio = Studio {
            storage: root.join("settings"),
            gate: Mutex::new(()),
            jobs: Mutex::new(HashMap::new()),
        };
        let mut w = create(
            "Independent workspace".into(),
            root.to_string_lossy().into(),
        )
        .unwrap();
        w.runs.push(Run {
            id: id(),
            stage: "analyze".into(),
            status: "running".into(),
            started: now(),
            finished: None,
            message: "".into(),
            logs: vec![],
            artifact_id: None,
            input_revision: 0,
        });
        let w = studio.register(w, &root).unwrap();
        assert_eq!(w.runs[0].status, "interrupted");
        assert_eq!(studio.load(&w.id).unwrap().name, "Independent workspace");
        assert!(create("Duplicate".into(), root.to_string_lossy().into()).is_err());
        assert!(studio.load("unregistered").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
