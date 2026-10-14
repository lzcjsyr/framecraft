//! Provider secrets never leave native memory or the operating system vault.
use keyring::{Entry, Error};
use std::{collections::BTreeMap, sync::Mutex};

pub const PROVIDERS: [&str; 4] = [
    "AGENT_API_KEY",
    "BYTEDANCE_TTS_API_KEY",
    "VOLCENGINE_API_KEY",
    "GOOGLE_CLOUD_API_KEY",
];
const SERVICE: &str = "com.framecraft.studio.media-keys";
const REFERENCE: &str = "com.book2video.studio.provider-keys";
static CACHE: Mutex<Option<Result<BTreeMap<String, String>, String>>> = Mutex::new(None);

fn load(service: &str) -> Result<BTreeMap<String, String>, String> {
    let entry =
        Entry::new(service, "provider-keys").map_err(|_| "Cannot access credential store")?;
    match entry.get_password() {
        Ok(json) => {
            serde_json::from_str(&json).map_err(|_| "Invalid provider credential vault".into())
        }
        Err(Error::NoEntry) => Ok(BTreeMap::new()),
        Err(_) => Err("Unlock the system credential store and retry".into()),
    }
}
type VaultResult = Result<BTreeMap<String, String>, String>;
fn load_once(cache: &mut Option<VaultResult>, fetch: impl FnOnce() -> VaultResult) -> VaultResult {
    // Remember denials too: visiting another page must not repeat the prompt.
    cache.get_or_insert_with(fetch).clone()
}
fn cached() -> VaultResult {
    let mut cache = CACHE.lock().map_err(|_| "Credential store unavailable")?;
    load_once(&mut cache, || {
        let mut keys = load(SERVICE)?;
        if !keys.contains_key("__agentMigrated") {
            match Entry::new("com.framecraft.studio", "agent-api-key")
                .map_err(|_| "Cannot access agent credentials")?
                .get_password()
            {
                Ok(value) if !value.is_empty() => {
                    keys.entry("AGENT_API_KEY".into()).or_insert(value);
                }
                Ok(_) | Err(Error::NoEntry) => {}
                Err(_) => {
                    return Err(
                        "Unlock the system credential store to migrate the agent key once".into(),
                    )
                }
            }
            keys.insert("__agentMigrated".into(), "1".into());
            write(&keys)?;
        }
        Ok(keys)
    })
}
/// Only an explicit user retry may re-open a denied vault read.
pub fn retry() -> Result<BTreeMap<String, bool>, String> {
    {
        let mut cache = CACHE.lock().map_err(|_| "Credential store unavailable")?;
        if cache.as_ref().is_some_and(Result::is_err) {
            *cache = None;
        }
    }
    status()
}
fn write(keys: &BTreeMap<String, String>) -> Result<(), String> {
    Entry::new(SERVICE, "provider-keys")
        .map_err(|_| "Cannot access credential store")?
        .set_password(&serde_json::to_string(keys).map_err(|_| "Cannot encode keys")?)
        .map_err(|_| "Cannot save keys in the system credential store".into())
}
pub fn optional(name: &str) -> Result<Option<String>, String> {
    Ok(cached()?.remove(name).filter(|k| !k.is_empty()))
}
pub fn save(changes: BTreeMap<String, String>) -> Result<BTreeMap<String, bool>, String> {
    if changes.keys().any(|k| !PROVIDERS.contains(&k.as_str())) {
        return Err("Unknown media provider".into());
    }
    let mut keys = cached()?;
    for (name, value) in changes {
        if !value.trim().is_empty() {
            keys.insert(name, value.trim().into());
        }
    }
    write(&keys)?;
    *CACHE.lock().map_err(|_| "Credential store unavailable")? = Some(Ok(keys));
    status()
}
pub fn status() -> Result<BTreeMap<String, bool>, String> {
    let keys = cached()?;
    let mut status: BTreeMap<String, bool> = PROVIDERS
        .iter()
        .map(|k| (k.to_string(), keys.get(*k).is_some_and(|v| !v.is_empty())))
        .collect();
    status.insert("GOOGLE_ADC".into(), adc().is_ok());
    Ok(status)
}
pub fn key(name: &str) -> Result<String, String> {
    cached()?
        .remove(name)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| format!("Save {name} in Settings first"))
}
/// Read the reference app's vault; never migrate or delete its credentials.
pub fn import_reference() -> Result<BTreeMap<String, bool>, String> {
    let existing = cached()?;
    let media_names = &PROVIDERS[1..];
    if media_names.iter().all(|name| existing.contains_key(*name)) {
        return status();
    }
    let reference = load(REFERENCE)?;
    let mut changes = BTreeMap::new();
    for &name in media_names {
        if existing.contains_key(name) {
            continue;
        }
        if let Some(value) = reference.get(name).filter(|v| !v.is_empty()) {
            changes.insert(name.into(), value.clone());
        } else {
            match Entry::new(REFERENCE, name)
                .map_err(|_| "Cannot access reference credentials")?
                .get_password()
            {
                Ok(v) if !v.is_empty() => {
                    changes.insert(name.into(), v);
                }
                Ok(_) | Err(Error::NoEntry) => {}
                Err(_) => {
                    return Err("Unlock the reference app's credential vault and retry".into())
                }
            }
        }
    }
    if changes.is_empty() {
        return status();
    }
    save(changes)
}

/// Desktop ADC created by gcloud; read-only, tokens remain in native memory.
pub fn adc() -> Result<serde_json::Value, String> {
    let path = if let Some(path) = std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS") {
        std::path::PathBuf::from(path)
    } else if cfg!(windows) {
        std::path::PathBuf::from(
            std::env::var_os("APPDATA").ok_or("No Windows app data directory")?,
        )
        .join("gcloud/application_default_credentials.json")
    } else {
        std::path::PathBuf::from(std::env::var_os("HOME").ok_or("No home directory")?)
            .join(".config/gcloud/application_default_credentials.json")
    };
    let meta = std::fs::metadata(&path)
        .map_err(|_| "Google Cloud login not found. Run gcloud auth application-default login.")?;
    if meta.len() > 1048576 {
        return Err("Invalid Google Cloud login file".into());
    }
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read Google Cloud login")?)
            .map_err(|_| "Invalid Google Cloud login file")?;
    if v["type"] != "authorized_user"
        || ["client_id", "client_secret", "refresh_token"]
            .iter()
            .any(|k| v[k].as_str().is_none_or(str::is_empty))
    {
        return Err(
            "Use a desktop Google Cloud login: gcloud auth application-default login".into(),
        );
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_denied_vault_read_is_not_repeated_until_explicit_retry() {
        let reads = std::cell::Cell::new(0);
        let mut cache = None;
        for _ in 0..4 {
            assert!(load_once(&mut cache, || {
                reads.set(reads.get() + 1);
                Err("Access denied".into())
            })
            .is_err());
        }
        assert_eq!(reads.get(), 1);
        cache = None; // the explicit Unlock saved keys action
        assert!(load_once(&mut cache, || {
            reads.set(reads.get() + 1);
            Ok(BTreeMap::new())
        })
        .is_ok());
        assert!(load_once(&mut cache, || panic!(
            "Should use the unlocked in-memory vault"
        ))
        .is_ok());
        assert_eq!(reads.get(), 2);
    }
}
