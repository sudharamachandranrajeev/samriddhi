use serde::{Serialize, ser::Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the database key is missing from this device")]
    KeyMissing,
    #[error("the stored database key is damaged")]
    KeyDamaged,
    #[error("could not keep the database key safe: {0}")]
    Store(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
