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

    /// AI keys are not kept on phones yet (no Keystore entry for them), so
    /// only a local model without a key can be set up there.
    pub fn ai_key(&self) -> Result<Option<String>> {
        Ok(None)
    }

    pub fn set_ai_key(&self, _key: &str) -> Result<()> {
        Err(Error::Unsupported)
    }

    pub fn clear_ai_key(&self) -> Result<()> {
        Ok(())
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
