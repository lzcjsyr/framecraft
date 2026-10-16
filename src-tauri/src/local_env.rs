//! Local development credentials; this module never accesses the system keychain.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
static WRITE_GATE: Mutex<()> = Mutex::new(());

pub fn path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("FRAMECRAFT_ENV_FILE") {
        return Some(path.into());
    }
    cfg!(debug_assertions).then(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".env")
    })
}
fn name(line: &str) -> Option<&str> {
    let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
    if line.starts_with('#') {
        return None;
    }
    line.split_once('=').map(|(key, _)| key.trim())
}
fn parse(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut keys = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let Some(key) = name(line).filter(|key| crate::credentials::PROVIDERS.contains(key)) else {
            continue;
        };
        let value = line.split_once('=').unwrap().1.trim();
        let value = if value.starts_with('"') {
            serde_json::from_str::<String>(value)
                .map_err(|_| format!("Invalid quoted value in .env line {}", index + 1))?
        } else if value.starts_with('\'') {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
                .ok_or_else(|| format!("Invalid quoted value in .env line {}", index + 1))?
                .to_owned()
        } else {
            value
                .split(" #")
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        if !value.is_empty() {
            keys.insert(key.into(), value);
        }
    }
    Ok(keys)
}
fn contents(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(_) => Err("Cannot read the local .env credential file".into()),
    }
}
pub fn read(path: &Path) -> Result<BTreeMap<String, String>, String> {
    parse(&contents(path)?)
}
pub fn save(path: &Path, changes: &BTreeMap<String, String>) -> Result<(), String> {
    let _guard = WRITE_GATE
        .lock()
        .map_err(|_| "Local credential file unavailable")?;
    if changes.iter().any(|(key, value)| {
        !crate::credentials::PROVIDERS.contains(&key.as_str())
            || value.len() > 2400
            || !value.is_ascii()
            || value
                .trim()
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
    }) {
        return Err("Enter API keys without whitespace, up to 2,400 characters".into());
    }
    let text = contents(path)?;
    parse(&text)?; // Never silently replace a malformed credential file.
    let changes: BTreeMap<_, _> = changes
        .iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .collect();
    if changes.is_empty() {
        return Ok(());
    }
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| {
            name(line).is_none_or(|key| !changes.keys().any(|changed| changed.as_str() == key))
        })
        .map(str::to_owned)
        .collect();
    lines.extend(
        changes
            .iter()
            .map(|(key, value)| format!("{key}={}", value.trim())),
    );
    let parent = path.parent().ok_or("Invalid .env location")?;
    fs::create_dir_all(parent).map_err(|_| "Cannot create the .env directory")?;
    let temp = parent.join(format!(".env.{}.tmp", crate::model::id()));
    let result = (|| {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|_| "Cannot create the local credential file")?;
        file.write_all(format!("{}\n", lines.join("\n")).as_bytes())
            .map_err(|_| "Cannot save local credentials")?;
        file.sync_all()
            .map_err(|_| "Cannot finish saving local credentials")?;
        fs::rename(&temp, path).map_err(|_| "Cannot update the local .env file")
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_keys_support_quotes_and_preserve_other_configuration() {
        let dir = std::env::temp_dir().join(format!("framecraft-env-{}", crate::model::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");
        fs::write(&path,"# Local configuration\nOTHER_SETTING=keep-me\nexport MIMO_API_KEY='old-value'\nVOLCENGINE_API_KEY=seed-value # comment\n").unwrap();
        assert_eq!(read(&path).unwrap()["MIMO_API_KEY"], "old-value");
        save(
            &path,
            &BTreeMap::from([
                ("MIMO_API_KEY".into(), "new-value".into()),
                ("VOLCENGINE_API_KEY".into(), "".into()),
            ]),
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("OTHER_SETTING=keep-me"));
        assert!(text.contains("# Local configuration"));
        assert!(!text.contains("old-value"));
        let keys = read(&path).unwrap();
        assert_eq!(keys["MIMO_API_KEY"], "new-value");
        assert_eq!(keys["VOLCENGINE_API_KEY"], "seed-value");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(save(
            &path,
            &BTreeMap::from([("MIMO_API_KEY".into(), "secret\nINJECT=value".into())])
        )
        .is_err());
        assert!(parse("MIMO_API_KEY=\"private-unfinished")
            .unwrap_err()
            .find("private-unfinished")
            .is_none());
        assert!(read(&dir.join("missing.env")).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
