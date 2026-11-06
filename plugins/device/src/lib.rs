//! The Samriddhi database key, kept in the OS keyring (desktop) or
//! wrapped by the Android Keystore. Adapted from Arogya Swasthya's device plugin.

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

mod error;
pub use error::{Error, Result};

#[cfg(desktop)]
mod desktop;
#[cfg(desktop)]
pub use desktop::Device;

#[cfg(mobile)]
mod mobile;
#[cfg(mobile)]
pub use mobile::Device;

/// Where the database key lives, shown in Settings → Security.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyStorage {
    /// The OS keyring (Secret Service on Linux, Credential Manager on Windows).
    Keyring,
    /// A file only this user can read, when no keyring is available.
    File,
    /// Encrypted by a key that never leaves the Android Keystore.
    AndroidKeystore,
}

pub struct DbKey {
    pub key: [u8; 32],
    /// True when the key was made just now.
    pub created: bool,
    pub storage: KeyStorage,
}

pub trait DeviceExt<R: Runtime> {
    fn device(&self) -> &Device<R>;
}

impl<R: Runtime, T: Manager<R>> DeviceExt<R> for T {
    fn device(&self) -> &Device<R> {
        self.state::<Device<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("samriddhi-device")
        .setup(|app, api| {
            #[cfg(mobile)]
            let device = mobile::init(app, api)?;
            #[cfg(desktop)]
            let device = desktop::init(app, api);
            app.manage(device);
            Ok(())
        })
        .build()
}

pub(crate) fn random_key() -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|e| Error::Store(e.to_string()))?;
    Ok(key)
}

pub fn to_hex(key: &[u8; 32]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads a key typed by the user: 64 hex digits, spaces and dashes ignored.
pub fn from_hex(s: &str) -> Option<[u8; 32]> {
    let digits: Vec<u8> = s
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'-')
        .collect();
    if digits.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (i, pair) in digits.chunks(2).enumerate() {
        key[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    #[test]
    fn hex_round_trip_ignores_grouping() {
        let key = [0xab; 32];
        let hex = super::to_hex(&key);
        let grouped: String = hex
            .as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("-");
        assert_eq!(super::from_hex(&grouped), Some(key));
        assert_eq!(super::from_hex("abcd"), None);
        assert_eq!(super::from_hex(&"zz".repeat(32)), None);
    }
}
