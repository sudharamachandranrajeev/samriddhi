use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use tauri::plugin::{PluginApi, PluginHandle};
use tauri::{AppHandle, Runtime};

use crate::{DbKey, Error, KeyStorage, Result, from_hex, to_hex};

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<Device<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("dev.brahma.samriddhi.device", "DevicePlugin")?;
    #[cfg(target_os = "ios")]
    compile_error!("samriddhi-device has no iOS implementation yet");
    Ok(Device(handle))
}

pub struct Device<R: Runtime>(PluginHandle<R>);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyReply {
    key: Option<String>,
    created: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyArgs {
    may_create: bool,
    key: Option<String>,
}

impl<R: Runtime> Device<R> {
    /// See the desktop version: a key is only made when `may_create` is true.
    pub fn db_key(&self, _dir: &Path, may_create: bool) -> Result<DbKey> {
        let r: KeyReply = self.0.run_mobile_plugin(
            "dbKey",
            KeyArgs {
                may_create,
                key: None,
            },
        )?;
        let key = from_hex(&r.key.ok_or(Error::KeyMissing)?).ok_or(Error::KeyDamaged)?;
        Ok(DbKey {
            key,
            created: r.created,
            storage: KeyStorage::AndroidKeystore,
        })
    }

    pub fn set_db_key(&self, _dir: &Path, key: &[u8; 32]) -> Result<KeyStorage> {
        self.0.run_mobile_plugin::<serde_json::Value>(
            "setDbKey",
            KeyArgs {
                may_create: true,
                key: Some(to_hex(key)),
            },
        )?;
        Ok(KeyStorage::AndroidKeystore)
    }

    /// The AI key, decrypted with its own Keystore key (never the database's).
    /// An error means the stored key cannot be read, not that there is none.
    pub fn ai_key(&self) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct Reply {
            key: Option<String>,
        }
        let r: Reply = self
            .0
            .run_mobile_plugin("aiKey", serde_json::json!({}))
            .map_err(|_| Error::Keyring("the saved key cannot be read".into()))?;
        Ok(r.key)
    }

    /// Keep the AI key, encrypted by a Keystore key. Errors never carry the key.
    pub fn set_ai_key(&self, key: &str) -> Result<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("setAiKey", serde_json::json!({ "key": key }))
            .map_err(|_| Error::Keyring("the key could not be saved".into()))?;
        Ok(())
    }

    /// Forget the AI key. Does not need to read it, so it also clears a key
    /// that can no longer be decrypted.
    pub fn clear_ai_key(&self) -> Result<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("clearAiKey", serde_json::json!({}))
            .map_err(|_| Error::Keyring("the key could not be removed".into()))?;
        Ok(())
    }

    /// Blocking screenshots and screen recording (Android FLAG_SECURE) works here.
    pub fn supports_capture_block(&self) -> bool {
        true
    }

    /// Turn the screenshot and recording block on or off for this window.
    pub fn set_capture_block(&self, on: bool) -> Result<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("setSecure", serde_json::json!({ "on": on }))?;
        Ok(())
    }

    /// Signs the phone may be rooted (best-effort, informational). An empty
    /// list means none found.
    pub fn root_hints(&self) -> Result<Option<Vec<String>>> {
        #[derive(Deserialize)]
        struct Reply {
            hints: Vec<String>,
        }
        let r: Reply = self
            .0
            .run_mobile_plugin("rootHints", serde_json::json!({}))?;
        Ok(Some(r.hints))
    }

    /// GET a page from Samriddhi's GitHub releases (Kotlin refuses any other address).
    pub fn fetch_text(&self, url: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Reply {
            text: String,
        }
        let r: Reply = self
            .0
            .run_mobile_plugin("fetchText", serde_json::json!({ "url": url }))?;
        Ok(r.text)
    }

    /// Download an APK, check it against the release's SHA256SUMS.txt, and open
    /// Android's installer on it.
    pub fn install_apk(&self, apk_url: &str, sums_url: &str, name: &str) -> Result<()> {
        self.0.run_mobile_plugin::<serde_json::Value>(
            "installApk",
            serde_json::json!({ "apkUrl": apk_url, "sumsUrl": sums_url, "name": name }),
        )?;
        Ok(())
    }
}
