//! The security status rows for Settings, and the one switch that lives with
//! them (block screenshots, Android only). Everything here is local: nothing
//! is sent anywhere, and no row ever blocks the app. The root hint and the
//! Sentinel row are informational and claim no more than they checked.

use std::path::Path;
use std::time::SystemTime;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_samriddhi_device::{DeviceExt, KeyStorage};

use crate::sentinel::{self, SentinelStatus};
use crate::{App, CommandResult, get};

/// The setting for "Block screenshots and screen recording". Off by default.
pub const BLOCK_CAPTURE: &str = "security.blockCapture";
/// A backup older than this many days is called out.
const BACKUP_STALE_DAYS: i64 = 2;
/// How an unencrypted SQLite file starts. An encrypted one looks random.
const PLAIN_SQLITE: &[u8] = b"SQLite format 3\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Good,
    Info,
    Warn,
    /// Does not apply on this device.
    Na,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityRow {
    /// lock, biometric, captureBlock, rootHint, sentinel, dbEncrypted, lastBackup
    pub id: &'static str,
    pub label: &'static str,
    pub level: Level,
    /// One plain sentence.
    pub detail: String,
    /// For rows that are a switch: whether it is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityStatus {
    pub rows: Vec<SecurityRow>,
}

/// What the rows are made from.
pub struct Facts {
    pub has_pin: bool,
    pub biometric: bool,
    pub phone: bool,
    /// `None`: this device cannot block capture. `Some`: the switch.
    pub capture_block: Option<bool>,
    /// `None`: not checkable here (no row). Empty: none found.
    pub root_hints: Option<Vec<String>>,
    /// `None`: not Windows (no row).
    pub sentinel: Option<SentinelStatus>,
    pub key_storage: Option<KeyStorage>,
    pub db_looks_plain: bool,
    pub backup_age_secs: Option<i64>,
}

fn row(
    id: &'static str,
    label: &'static str,
    level: Level,
    detail: impl Into<String>,
) -> SecurityRow {
    SecurityRow {
        id,
        label,
        level,
        detail: detail.into(),
        on: None,
    }
}

/// "just now", "5 hours", "3 days".
fn ago(secs: i64) -> String {
    let (n, unit) = match secs.max(0) {
        s if s < 3600 => return "less than an hour ago".into(),
        s if s < 86_400 => (s / 3600, "hour"),
        s => (s / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

pub fn rows(f: &Facts) -> Vec<SecurityRow> {
    let mut out = Vec::new();

    out.push(match (f.has_pin, f.biometric) {
        (false, false) => row(
            "lock",
            "App lock",
            Level::Info,
            "Off. Set a PIN to hide your data when you step away.",
        ),
        (true, false) => row("lock", "App lock", Level::Good, "On, with a PIN."),
        (false, true) => row(
            "lock",
            "App lock",
            Level::Good,
            "On, with fingerprint or face.",
        ),
        (true, true) => row(
            "lock",
            "App lock",
            Level::Good,
            "On, with a PIN and fingerprint or face.",
        ),
    });

    out.push(if !f.phone {
        row(
            "biometric",
            "Fingerprint or face",
            Level::Na,
            "For phones; use a PIN on this computer.",
        )
    } else if f.biometric {
        row("biometric", "Fingerprint or face", Level::Good, "On.")
    } else {
        row("biometric", "Fingerprint or face", Level::Info, "Off.")
    });

    out.push(match f.capture_block {
        None => row(
            "captureBlock",
            "Block screenshots and screen recording",
            Level::Na,
            "For phones.",
        ),
        Some(on) => SecurityRow {
            on: Some(on),
            ..row(
                "captureBlock",
                "Block screenshots and screen recording",
                if on { Level::Good } else { Level::Info },
                if on {
                    "On. Screenshots, recording and the app-switcher preview are blocked."
                } else {
                    "Off. Turn it on to block screenshots, recording and the app-switcher preview."
                },
            )
        },
    });

    if let Some(hints) = &f.root_hints {
        out.push(if hints.is_empty() {
            row(
                "rootHint",
                "Rooted phone",
                Level::Good,
                "No signs of root found. This is a best-effort check.",
            )
        } else {
            row(
                "rootHint",
                "Rooted phone",
                Level::Warn,
                format!(
                    "This phone may be rooted ({}). For information only: other apps on a \
                     rooted phone can read more than they should.",
                    hints.join(", ")
                ),
            )
        });
    }

    if let Some(s) = &f.sentinel {
        out.push(match s {
            SentinelStatus::Running { version } => row(
                "sentinel",
                "Sentinel",
                Level::Good,
                format!("Sentinel {version} is running."),
            ),
            SentinelStatus::Restricted => row(
                "sentinel",
                "Sentinel",
                Level::Info,
                "Sentinel is running, but its status needs administrator rights.",
            ),
            SentinelStatus::Absent => row(
                "sentinel",
                "Sentinel",
                Level::Na,
                "Not installed. It is optional.",
            ),
            SentinelStatus::Incompatible { detail } => row(
                "sentinel",
                "Sentinel",
                Level::Info,
                format!("Found, but it speaks a different version ({detail})."),
            ),
            SentinelStatus::Error { detail } => row(
                "sentinel",
                "Sentinel",
                Level::Info,
                format!("Found, but it did not answer ({detail})."),
            ),
        });
    }

    out.push(if f.db_looks_plain {
        row(
            "dbEncrypted",
            "Data encrypted",
            Level::Warn,
            "The data file does not look encrypted. Restore from a backup or contact support.",
        )
    } else {
        let (level, place) = match f.key_storage {
            Some(KeyStorage::Keyring) => (Level::Good, "the system keyring"),
            Some(KeyStorage::AndroidKeystore) => (Level::Good, "the Android Keystore"),
            Some(KeyStorage::File) => (
                Level::Info,
                "a private file in the app folder, because no system keyring was found",
            ),
            None => (Level::Info, "this device"),
        };
        row(
            "dbEncrypted",
            "Data encrypted",
            level,
            format!("Yes. The key is kept in {place}."),
        )
    });

    out.push(match f.backup_age_secs {
        None => row(
            "lastBackup",
            "Last backup",
            Level::Warn,
            "No backup yet. One is made each day the app is open.",
        ),
        Some(s) if s > BACKUP_STALE_DAYS * 86_400 => row(
            "lastBackup",
            "Last backup",
            Level::Warn,
            format!("{}. Open the app or press Back up now.", ago(s)),
        ),
        Some(s) => row("lastBackup", "Last backup", Level::Good, ago(s)),
    });

    out
}

/// Does the file start like an unencrypted SQLite database?
pub fn looks_plain(db: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 16];
    std::fs::File::open(db)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && head == *PLAIN_SQLITE
}

/// How old the newest backup in `dir` is, by file time.
pub fn newest_backup_age(dir: &Path, now: SystemTime) -> Option<i64> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.ends_with(".db") && !n.starts_with('.')
        })
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
        .map(|m| {
            now.duration_since(m)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        })
}

/// The rows for the Settings security section. Only while unlocked.
#[tauri::command]
pub async fn security_status(
    app: AppHandle,
    state: State<'_, App>,
) -> CommandResult<SecurityStatus> {
    let (has_pin, biometric, blocked) = state.with(|s| {
        Ok((
            get::<String>(s, "lock.pin").is_some(),
            get::<bool>(s, "lock.biometric").unwrap_or(false),
            get::<bool>(s, BLOCK_CAPTURE).unwrap_or(false),
        ))
    })?;
    let key_storage = state.lock_inner()?.key_storage;
    let device = app.device();
    let sentinel = if cfg!(windows) {
        Some(
            tauri::async_runtime::spawn_blocking(sentinel::query)
                .await
                .unwrap_or(SentinelStatus::Error {
                    detail: "the check did not finish".into(),
                }),
        )
    } else {
        None
    };
    Ok(SecurityStatus {
        rows: rows(&Facts {
            has_pin,
            biometric,
            phone: cfg!(mobile),
            capture_block: device.supports_capture_block().then_some(blocked),
            root_hints: device.root_hints().ok().flatten(),
            sentinel,
            key_storage,
            db_looks_plain: looks_plain(&state.db()),
            backup_age_secs: newest_backup_age(&state.backup_dir(), SystemTime::now()),
        }),
    })
}

/// Turn "Block screenshots and screen recording" on or off (phones only).
#[tauri::command]
pub fn set_block_capture(app: AppHandle, state: State<App>, on: bool) -> CommandResult<()> {
    if !app.device().supports_capture_block() {
        return Err("Blocking screenshots is for phones.".into());
    }
    state.with(|_| Ok(()))?;
    app.device()
        .set_capture_block(on)
        .map_err(|e| e.to_string())?;
    state.with(|s| s.set_setting(BLOCK_CAPTURE, &on))
}

/// At start: apply the saved choice to the window, before the lock screen is
/// used. The store is open (just locked) by now. Best-effort.
pub fn apply_at_launch(app: &AppHandle) {
    if !app.device().supports_capture_block() {
        return;
    }
    let state = app.state::<App>();
    let on = state
        .lock_inner()
        .ok()
        .and_then(|i| i.store.as_ref().and_then(|s| get::<bool>(s, BLOCK_CAPTURE)))
        .unwrap_or(false);
    let _ = app.device().set_capture_block(on);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn facts() -> Facts {
        Facts {
            has_pin: true,
            biometric: false,
            phone: false,
            capture_block: None,
            root_hints: None,
            sentinel: None,
            key_storage: Some(KeyStorage::Keyring),
            db_looks_plain: false,
            backup_age_secs: Some(3 * 3600),
        }
    }

    fn find<'a>(rows: &'a [SecurityRow], id: &str) -> Option<&'a SecurityRow> {
        rows.iter().find(|r| r.id == id)
    }

    #[test]
    fn desktop_rows_leave_out_what_does_not_apply() {
        let r = rows(&facts());
        let ids: Vec<_> = r.iter().map(|r| r.id).collect();
        assert_eq!(
            ids,
            [
                "lock",
                "biometric",
                "captureBlock",
                "dbEncrypted",
                "lastBackup"
            ]
        );
        assert_eq!(find(&r, "lock").unwrap().level, Level::Good);
        assert_eq!(find(&r, "biometric").unwrap().level, Level::Na);
        assert_eq!(find(&r, "captureBlock").unwrap().level, Level::Na);
        assert_eq!(find(&r, "captureBlock").unwrap().on, None);
    }

    #[test]
    fn lock_row_follows_pin_and_biometric() {
        let lock = |pin, bio| {
            let mut f = facts();
            (f.has_pin, f.biometric) = (pin, bio);
            find(&rows(&f), "lock").unwrap().clone()
        };
        assert_eq!(lock(false, false).level, Level::Info);
        assert!(lock(false, false).detail.starts_with("Off"));
        assert!(lock(true, false).detail.contains("PIN"));
        assert!(lock(false, true).detail.contains("fingerprint"));
        assert!(lock(true, true).detail.contains("PIN and fingerprint"));
    }

    #[test]
    fn phone_rows_show_the_switch_and_the_root_hint() {
        let mut f = facts();
        f.phone = true;
        f.biometric = true;
        f.capture_block = Some(false);
        f.root_hints = Some(vec![]);
        f.key_storage = Some(KeyStorage::AndroidKeystore);
        let r = rows(&f);
        assert_eq!(find(&r, "biometric").unwrap().level, Level::Good);
        let cap = find(&r, "captureBlock").unwrap();
        assert_eq!((cap.on, cap.level), (Some(false), Level::Info));
        assert_eq!(find(&r, "rootHint").unwrap().level, Level::Good);
        assert!(find(&r, "dbEncrypted").unwrap().detail.contains("Keystore"));
        f.capture_block = Some(true);
        f.root_hints = Some(vec!["su binary present".into(), "test-keys build".into()]);
        let r = rows(&f);
        assert_eq!(find(&r, "captureBlock").unwrap().on, Some(true));
        let root = find(&r, "rootHint").unwrap();
        assert_eq!(root.level, Level::Warn);
        assert!(root.detail.contains("su binary present, test-keys build"));
        assert!(root.detail.contains("For information only"));
    }

    #[test]
    fn sentinel_row_says_only_what_was_found() {
        let row_for = |s| {
            let mut f = facts();
            f.sentinel = Some(s);
            find(&rows(&f), "sentinel").unwrap().clone()
        };
        let running = row_for(SentinelStatus::Running {
            version: "0.3".into(),
        });
        assert_eq!(running.detail, "Sentinel 0.3 is running.");
        assert_eq!(row_for(SentinelStatus::Absent).level, Level::Na);
        assert!(
            row_for(SentinelStatus::Restricted)
                .detail
                .contains("administrator")
        );
        let odd = row_for(SentinelStatus::Incompatible {
            detail: "protocol 2".into(),
        });
        assert!(odd.detail.contains("protocol 2") && odd.level == Level::Info);
        assert!(
            row_for(SentinelStatus::Error {
                detail: "no answer".into()
            })
            .detail
            .contains("no answer")
        );
    }

    #[test]
    fn database_and_backup_rows() {
        let mut f = facts();
        f.db_looks_plain = true;
        assert_eq!(find(&rows(&f), "dbEncrypted").unwrap().level, Level::Warn);
        f.db_looks_plain = false;
        f.key_storage = Some(KeyStorage::File);
        let r = rows(&f);
        assert_eq!(find(&r, "dbEncrypted").unwrap().level, Level::Info);
        assert!(
            find(&r, "dbEncrypted")
                .unwrap()
                .detail
                .contains("private file")
        );
        assert_eq!(find(&r, "lastBackup").unwrap().detail, "3 hours ago");
        f.backup_age_secs = Some(5 * 86_400);
        let r = rows(&f);
        assert_eq!(find(&r, "lastBackup").unwrap().level, Level::Warn);
        assert!(
            find(&r, "lastBackup")
                .unwrap()
                .detail
                .starts_with("5 days ago")
        );
        f.backup_age_secs = None;
        assert!(
            find(&rows(&f), "lastBackup")
                .unwrap()
                .detail
                .starts_with("No backup")
        );
        assert_eq!(ago(90), "less than an hour ago");
        assert_eq!(ago(3600), "1 hour ago");
    }

    #[test]
    fn plain_sqlite_files_are_noticed_and_other_bytes_are_not() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.db");
        let mut bytes = PLAIN_SQLITE.to_vec();
        bytes.extend_from_slice(&[0; 100]);
        std::fs::write(&plain, bytes).unwrap();
        assert!(looks_plain(&plain));
        let sealed = dir.path().join("sealed.db");
        std::fs::write(&sealed, [0xa7u8; 200]).unwrap();
        assert!(!looks_plain(&sealed));
        assert!(!looks_plain(&dir.path().join("missing.db")));
        // A store made by the real code is not plain.
        let real = dir.path().join("real.db");
        let key: samriddhi_core::keys::Key = [7u8; 32];
        drop(samriddhi_core::store::Store::open(&real, &key).unwrap());
        assert!(!looks_plain(&real));
    }

    #[test]
    fn newest_backup_age_uses_the_newest_db_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(newest_backup_age(dir.path(), SystemTime::now()), None);
        let now = SystemTime::now();
        let make = |name: &str, age: u64| {
            let f = std::fs::File::create(dir.path().join(name)).unwrap();
            f.set_modified(now - Duration::from_secs(age)).unwrap();
        };
        make("daily-old.db", 10 * 86_400);
        make("daily-new.db", 7200);
        make("notes.txt", 10);
        make(".hidden.db", 5);
        assert_eq!(newest_backup_age(dir.path(), now), Some(7200));
        assert_eq!(newest_backup_age(&dir.path().join("none"), now), None);
    }
}
