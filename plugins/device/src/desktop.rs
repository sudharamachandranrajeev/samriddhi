use std::marker::PhantomData;
use std::path::Path;

use serde::de::DeserializeOwned;
use tauri::plugin::PluginApi;
use tauri::{AppHandle, Runtime};

use crate::{DbKey, Error, KeyStorage, Result, from_hex, random_key, to_hex};

/// Keyring entries are named `<app id>.<purpose>`.
const SERVICE: &str = "dev.brahma.samriddhi.db-key";
const ACCOUNT: &str = "samriddhi";
const KEY_FILE: &str = "db.key";
/// The user's own AI API key: only ever in the OS keyring, never in a file.
const AI_SERVICE: &str = "dev.brahma.samriddhi.ai";

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> Device<R> {
    Device {
        keyring: init_keyring().err(),
        _r: PhantomData,
    }
}

pub struct Device<R: Runtime> {
    /// Why the keyring is unavailable, if it is.
    keyring: Option<String>,
    _r: PhantomData<fn() -> R>,
}

fn init_keyring() -> std::result::Result<(), String> {
    #[cfg(target_os = "linux")]
    keyring_core::set_default_store(
        zbus_secret_service_keyring_store::Store::new().map_err(|e| e.to_string())?,
    );
    #[cfg(windows)]
    keyring_core::set_default_store(
        windows_native_keyring_store::Store::new().map_err(|e| e.to_string())?,
    );
    #[cfg(not(any(target_os = "linux", windows)))]
    return Err("no supported keyring on this system".into());
    #[allow(unreachable_code)]
    Ok(())
}

impl<R: Runtime> Device<R> {
    /// The database key. A new one is made only when `may_create` is true,
    /// that is when there is no database yet: making a fresh key for an
    /// existing database would lock the user out of it.
    pub fn db_key(&self, dir: &Path, may_create: bool) -> Result<DbKey> {
        let file = dir.join(KEY_FILE);
        if file.exists() {
            let key = from_hex(std::fs::read_to_string(&file)?.trim()).ok_or(Error::KeyDamaged)?;
            return Ok(DbKey {
                key,
                created: false,
                storage: KeyStorage::File,
            });
        }
        if self.keyring.is_none() {
            let entry = keyring_core::Entry::new(SERVICE, ACCOUNT)
                .map_err(|e| Error::Store(e.to_string()))?;
            match entry.get_password() {
                Ok(hex) => {
                    let key = from_hex(&hex).ok_or(Error::KeyDamaged)?;
                    return Ok(DbKey {
                        key,
                        created: false,
                        storage: KeyStorage::Keyring,
                    });
                }
                Err(keyring_core::Error::NoEntry) if may_create => {
                    let key = random_key()?;
                    if entry.set_password(&to_hex(&key)).is_ok() {
                        return Ok(DbKey {
                            key,
                            created: true,
                            storage: KeyStorage::Keyring,
                        });
                    }
                }
                Err(keyring_core::Error::NoEntry) => return Err(Error::KeyMissing),
                // A locked or broken keyring must not lead to a new key.
                Err(e) if !may_create => return Err(Error::Store(e.to_string())),
                Err(_) => {}
            }
        }
        if !may_create {
            return Err(Error::KeyMissing);
        }
        let key = random_key()?;
        write_private(&file, &to_hex(&key))?;
        Ok(DbKey {
            key,
            created: true,
            storage: KeyStorage::File,
        })
    }

    /// Keep a key the user typed in (their recovery key).
    pub fn set_db_key(&self, dir: &Path, key: &[u8; 32]) -> Result<KeyStorage> {
        let file = dir.join(KEY_FILE);
        if self.keyring.is_none() && !file.exists() {
            let entry = keyring_core::Entry::new(SERVICE, ACCOUNT)
                .map_err(|e| Error::Store(e.to_string()))?;
            if entry.set_password(&to_hex(key)).is_ok() {
                return Ok(KeyStorage::Keyring);
            }
        }
        write_private(&file, &to_hex(key))?;
        Ok(KeyStorage::File)
    }
}

impl<R: Runtime> Device<R> {
    fn ai_entry(&self) -> Result<keyring_core::Entry> {
        if let Some(why) = &self.keyring {
            return Err(Error::Keyring(why.clone()));
        }
        keyring_core::Entry::new(AI_SERVICE, ACCOUNT).map_err(|e| Error::Keyring(e.to_string()))
    }

    /// The user's AI API key, if they gave one.
    pub fn ai_key(&self) -> Result<Option<String>> {
        match self.ai_entry()?.get_password() {
            Ok(k) => Ok(Some(k)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Keyring(e.to_string())),
        }
    }

    /// Keep the AI API key in the OS keyring. With no keyring there is no
    /// fallback file: the key is refused instead.
    pub fn set_ai_key(&self, key: &str) -> Result<()> {
        self.ai_entry()?
            .set_password(key)
            .map_err(|e| Error::Keyring(e.to_string()))
    }

    /// Forget the AI API key.
    pub fn clear_ai_key(&self) -> Result<()> {
        match self.ai_entry()?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Keyring(e.to_string())),
        }
    }
}

/// Write a file only the current user can read.
fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
