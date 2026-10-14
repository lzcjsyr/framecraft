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

pub struct AdcAccess {
    pub token: String,
    pub expires_in: u64,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdcStatus {
    pub logged_in: bool,
    pub project: Option<String>,
    pub checked: u64,
    pub expires_at: Option<u64>,
    pub error: Option<String>,
}
pub fn adc_project(adc: &serde_json::Value, configured: &str) -> Result<Option<String>, String> {
    let project = if configured.trim().is_empty() {
        adc["quota_project_id"].as_str().unwrap_or_default().trim()
    } else {
        configured.trim()
    };
    if project.len() > 100
        || !project
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Enter a valid Google Cloud project ID".into());
    }
    Ok((!project.is_empty()).then(|| project.to_owned()))
}
fn adc_access(success: bool, value: &serde_json::Value) -> Result<AdcAccess, String> {
    if !success {
        return Err(match value["error"].as_str() {
            Some("invalid_grant" | "invalid_client" | "unauthorized_client") =>
                "Google ADC login expired or was revoked. Run gcloud auth application-default login.",
            _ => "Google rejected ADC verification. Check your network and Google Cloud login, then retry.",
        }.into());
    }
    let token = value["access_token"]
        .as_str()
        .filter(|s| !s.trim().is_empty());
    let expires = value["expires_in"].as_u64().filter(|n| *n > 0);
    if !value["token_type"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case("Bearer"))
    {
        return Err("Google returned an invalid ADC access token".into());
    }
    Ok(AdcAccess {
        token: token.ok_or("Google returned no ADC access token")?.into(),
        expires_in: expires.ok_or("Google returned an expired or invalid ADC access token")?,
    })
}
/// Shared by verification and generation. Never return credentials in diagnostics.
pub async fn refresh_adc(
    client: &reqwest::Client,
    adc: &serde_json::Value,
) -> Result<AdcAccess, String> {
    let mut response = client
        .post("https://oauth2.googleapis.com/token")
        .timeout(std::time::Duration::from_secs(20))
        .form(&[
            ("grant_type", "refresh_token"),
            (
                "client_id",
                adc["client_id"].as_str().ok_or("Invalid ADC client ID")?,
            ),
            (
                "client_secret",
                adc["client_secret"]
                    .as_str()
                    .ok_or("Invalid ADC client credentials")?,
            ),
            (
                "refresh_token",
                adc["refresh_token"].as_str().ok_or("Invalid ADC login")?,
            ),
        ])
        .send()
        .await
        .map_err(|_| "Cannot verify Google ADC login. Check your network and retry.")?;
    let success = response.status().is_success();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Google ADC verification was interrupted")?
    {
        if body.len() + chunk.len() > 65536 {
            return Err("Invalid Google ADC authentication response".into());
        }
        body.extend(chunk);
    }
    let value =
        serde_json::from_slice(&body).map_err(|_| "Invalid Google ADC authentication response")?;
    adc_access(success, &value)
}
pub async fn inspect_adc(configured_project: &str) -> AdcStatus {
    let mut status = AdcStatus {
        logged_in: false,
        project: None,
        checked: crate::model::now(),
        expires_at: None,
        error: None,
    };
    let result = async {
        let adc = adc()?;
        status.project = adc_project(&adc, configured_project)?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Cannot initialize ADC verification")?;
        refresh_adc(&client, &adc).await
    }
    .await;
    status.checked = crate::model::now();
    match result {
        Ok(access) => {
            status.logged_in = true;
            status.expires_at = Some(status.checked.saturating_add(access.expires_in));
        }
        Err(error) => status.error = Some(error),
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adc_verification_fails_closed_and_does_not_return_secrets() {
        let valid = serde_json::json!({"access_token":"fixture-secret-token","expires_in":3600,"token_type":"Bearer"});
        assert!(adc_access(true, &valid).is_ok());
        for invalid in [
            serde_json::json!({}),
            serde_json::json!({"error":"invalid_grant"}),
            serde_json::json!({"access_token":"","expires_in":3600,"token_type":"Bearer"}),
            serde_json::json!({"access_token":"secret","expires_in":0,"token_type":"Bearer"}),
            serde_json::json!({"access_token":"secret","expires_in":3600,"token_type":"Basic"}),
        ] {
            assert!(adc_access(true, &invalid).is_err());
        }
        let error = adc_access(
            false,
            &serde_json::json!({"error":"invalid_grant","error_description":"private-token"}),
        )
        .err()
        .unwrap();
        assert!(error.contains("gcloud auth application-default login"));
        assert!(!error.contains("private-token"));
        let status = AdcStatus {
            logged_in: true,
            project: None,
            checked: 10,
            expires_at: Some(3610),
            error: None,
        };
        assert!(!serde_json::to_string(&status)
            .unwrap()
            .contains("access_token"));
        assert_eq!(
            adc_project(&serde_json::json!({"quota_project_id":"quota-project"}), "")
                .unwrap()
                .as_deref(),
            Some("quota-project")
        );
        assert_eq!(
            adc_project(&valid, " override-project ")
                .unwrap()
                .as_deref(),
            Some("override-project")
        );
        assert!(adc_project(&valid, "bad/project").is_err());
        assert!(adc_project(&valid, "").unwrap().is_none());
    }
    #[test]
    #[ignore = "Checks the existing Google ADC login over HTTPS; no model requests or Keychain access"]
    fn real_adc_login_status() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let status = runtime.block_on(inspect_adc(""));
        assert!(status.logged_in, "{}", status.error.unwrap_or_default());
        assert!(status.expires_at.unwrap() > status.checked);
        println!("Google ADC login verified; token remains in native memory");
    }
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
