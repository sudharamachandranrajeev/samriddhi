//! Backups the user can see and act on: list, back up now, save a copy
//! anywhere, a second folder, and restore. A restore always backs up the
//! current data first, and never changes the file it restores from.

use std::path::{Path, PathBuf};

use samriddhi_core::keys::{self, Key};
use samriddhi_core::store::{self, Store, Summary};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::{App, BACKUP_DIR, CommandResult, DB_FILE, LOCKED, err, get, now};

const MANUAL: &str = "manual-";
const BEFORE_RESTORE: &str = "before-restore-";
pub(crate) const BEFORE_IMPORT: &str = "before-import-";
const KEEP_MANUAL: usize = 20;
const KEEP_BEFORE_RESTORE: usize = 5;
/// Where a file picked for restoring is copied, so it can be read like any other.
const PICKED: &str = "picked.db";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    name: String,
    /// daily, manual, beforeRestore or beforeImport
    kind: &'static str,
    bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backups {
    folder: String,
    files: Vec<Backup>,
    /// The extra folder the user chose, if any, and the daily backups in it.
    user_folder: Option<String>,
    user_files: Vec<String>,
}

impl App {
    pub fn backup_dir(&self) -> PathBuf {
        self.dir.join(BACKUP_DIR)
    }

    /// A backup in the backup folder by name. Names come from the UI, so
    /// anything that is not a plain file name there is refused.
    fn backup_path(&self, name: &str) -> CommandResult<PathBuf> {
        let ok = name.ends_with(".db") && !name.contains(['/', '\\']) && !name.starts_with('.');
        let path = self.backup_dir().join(name);
        if !ok || !path.is_file() {
            return Err("That backup is not in the backup folder.".into());
        }
        Ok(path)
    }

    /// The open, unlocked store and its key.
    pub(crate) fn unlocked<T>(
        &self,
        f: impl FnOnce(&Store, &Key) -> CommandResult<T>,
    ) -> CommandResult<T> {
        let inner = self.lock_inner()?;
        let (Some(store), Some(key), false) = (&inner.store, &inner.key, inner.locked) else {
            return Err(LOCKED.into());
        };
        f(store, key)
    }

    /// Export the open store to a new file in the backup folder.
    pub(crate) fn backup_as(&self, prefix: &str, keep: usize) -> CommandResult<String> {
        self.unlocked(|store, key| {
            let dir = self.backup_dir();
            std::fs::create_dir_all(&dir).map_err(err)?;
            let name = format!("{prefix}{}.db", now().strftime("%Y-%m-%d-%H%M%S"));
            store.export(&dir.join(&name), key).map_err(err)?;
            prune(&dir, prefix, keep);
            Ok(name)
        })
    }

    /// Replace the database with `restored` (already locked with the device
    /// key). The current data is backed up first; if the new file does not
    /// open, the old one is put back.
    fn swap_in(&self, restored: &Path) -> CommandResult<String> {
        let saved = self.backup_as(BEFORE_RESTORE, KEEP_BEFORE_RESTORE)?;
        let mut inner = self.lock_inner()?;
        let key = inner.key.ok_or(LOCKED)?;
        // Closing the store checkpoints the write-ahead log into the file.
        inner.store = None;
        match self.replace_db(restored, |db| Store::open(db, &key).map_err(err)) {
            Ok(store) => {
                inner.store = Some(store);
                Ok(saved)
            }
            Err(e) => {
                inner.store = Some(Store::open(&self.dir.join(DB_FILE), &key).map_err(err)?);
                Err(format!(
                    "The backup could not be opened, so nothing was changed ({e})."
                ))
            }
        }
    }

    /// Move `new` into place as the database and check it with `open`. If
    /// that fails, the old file is put back. The store must be closed.
    fn replace_db<T>(
        &self,
        new: &Path,
        open: impl FnOnce(&Path) -> CommandResult<T>,
    ) -> CommandResult<T> {
        let db = self.dir.join(DB_FILE);
        let old = self.dir.join(format!("{DB_FILE}.old"));
        for side in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(self.dir.join(format!("{DB_FILE}{side}")));
        }
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&db, &old).map_err(err)?;
        match std::fs::rename(new, &db)
            .map_err(err)
            .and_then(|_| open(&db))
        {
            Ok(v) => {
                let _ = std::fs::remove_file(&old);
                Ok(v)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&db);
                std::fs::rename(&old, &db).map_err(err)?;
                Err(e)
            }
        }
    }
}

fn prune(dir: &Path, prefix: &str, keep: usize) {
    let mut all: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    all.retain(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".db"))
    });
    all.sort();
    for old in &all[..all.len().saturating_sub(keep)] {
        let _ = std::fs::remove_file(old);
    }
}

fn kind(name: &str) -> &'static str {
    if name.starts_with(MANUAL) {
        "manual"
    } else if name.starts_with(BEFORE_RESTORE) {
        "beforeRestore"
    } else if name.starts_with(BEFORE_IMPORT) {
        "beforeImport"
    } else {
        "daily"
    }
}

fn daily_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = store::backups(dir)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| p.file_name()?.to_str().map(String::from))
        .collect();
    v.sort();
    v.reverse();
    v
}

/// Where a restore reads from: a backup in the folder by name, or the
/// picked file.
fn source(state: &App, name: Option<&str>) -> CommandResult<PathBuf> {
    match name {
        Some(n) => state.backup_path(n),
        None => {
            let p = state.dir.join(PICKED);
            if !p.is_file() {
                return Err("Choose a file first.".into());
            }
            Ok(p)
        }
    }
}

/// Open the file with this device's key, then with the recovery key if one
/// was typed (a backup from another device opens only with that device's
/// recovery key). The first that works wins.
fn with_key<T>(
    state: &App,
    recovery: Option<&str>,
    f: impl Fn(&Key) -> store::Result<T>,
) -> CommandResult<T> {
    let mut tries = vec![state.lock_inner()?.key.ok_or(LOCKED)?];
    if let Some(text) = recovery.filter(|t| !t.trim().is_empty()) {
        tries.push(
            keys::parse_recovery(text)
                .ok_or("A recovery key is 64 letters and digits (0-9, a-f).")?,
        );
    }
    let mut last = String::new();
    for key in &tries {
        match f(key) {
            Ok(v) => return Ok(v),
            Err(store::Error::WrongKey) => {
                last = if recovery.is_some() {
                    "That recovery key does not open this file.".into()
                } else {
                    "This file was made on another device. Type that device's recovery key to open it.".into()
                }
            }
            Err(store::Error::TooNew(v)) => {
                return Err(format!(
                    "This backup was made by a newer Samriddhi (data version {v}). Update the app first."
                ));
            }
            Err(e) => {
                return Err(format!(
                    "This file could not be read as an Samriddhi backup ({e})."
                ));
            }
        }
    }
    Err(last)
}

#[tauri::command]
pub fn backups(state: State<App>) -> CommandResult<Backups> {
    let user_folder = state.with(|s| Ok(get::<String>(s, "backup.folder")))?;
    let dir = state.backup_dir();
    let mut files: Vec<Backup> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().into_string().ok()?;
                name.ends_with(".db").then(|| Backup {
                    kind: kind(&name),
                    bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
                    name,
                })
            })
            .collect()
        })
        .unwrap_or_default();
    // Every name carries its date right after the prefix: newest first.
    files.sort_by(|a, b| {
        let date = |n: &str| {
            n.trim_start_matches(|c: char| !c.is_ascii_digit())
                .to_owned()
        };
        date(&b.name).cmp(&date(&a.name))
    });
    Ok(Backups {
        folder: dir.display().to_string(),
        files,
        user_files: user_folder
            .as_deref()
            .map(|f| daily_names(Path::new(f)))
            .unwrap_or_default(),
        user_folder,
    })
}

/// Choose the extra backup folder (`None` stops copying there). Today's
/// backup is written to it at once, which also proves it is writable.
#[tauri::command]
pub fn set_backup_folder(state: State<App>, folder: Option<String>) -> CommandResult<()> {
    if let Some(f) = &folder {
        let path = Path::new(f);
        if !path.is_absolute() || !path.is_dir() {
            return Err(format!("{f} is not a folder on this device."));
        }
        state.unlocked(|store, key| {
            store
                .daily_backup(path, key, now())
                .map(drop)
                .map_err(|e| format!("Could not write a backup to {f}: {e}"))
        })?;
    }
    state.with(|s| s.set_setting("backup.folder", &folder))
}

/// Back up now, whether or not today's backup exists.
#[tauri::command]
pub fn backup_now(state: State<App>) -> CommandResult<String> {
    state.backup_as(MANUAL, KEEP_MANUAL)
}

/// Write a backup to a file the user picked in the save dialog. On Android
/// that is a content URI, which only the fs plugin can open.
#[tauri::command]
pub fn save_backup_to(app: AppHandle, state: State<App>, target: String) -> CommandResult<()> {
    let tmp = state.dir.join("save-backup.partial");
    let _ = std::fs::remove_file(&tmp);
    state.unlocked(|store, key| store.export(&tmp, key).map_err(err))?;
    let copied = copy_to(&app, &tmp, &target);
    let _ = std::fs::remove_file(&tmp);
    copied.map_err(|e| format!("Could not save the backup: {e}"))
}

#[cfg(mobile)]
fn copy_to(app: &AppHandle, from: &Path, target: &str) -> std::io::Result<()> {
    use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
    let path: FilePath = target.parse().map_err(std::io::Error::other)?;
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut out = app.fs().open(path, opts)?;
    std::io::copy(&mut std::fs::File::open(from)?, &mut out)?;
    Ok(())
}

#[cfg(desktop)]
fn copy_to(_app: &AppHandle, from: &Path, target: &str) -> std::io::Result<()> {
    std::fs::copy(from, target).map(|_| ())
}

#[cfg(mobile)]
pub(crate) fn read_picked(
    app: &AppHandle,
    path: tauri_plugin_dialog::FilePath,
) -> std::io::Result<Vec<u8>> {
    use tauri_plugin_fs::FsExt;
    app.fs().read(path)
}

#[cfg(desktop)]
pub(crate) fn read_picked(
    _app: &AppHandle,
    path: tauri_plugin_dialog::FilePath,
) -> std::io::Result<Vec<u8>> {
    std::fs::read(path.into_path().map_err(std::io::Error::other)?)
}

/// Let the user pick a backup file (saved with "Save a backup as…", or from
/// another device). Returns its file name, or nothing when cancelled.
#[tauri::command]
pub async fn pick_backup(app: AppHandle, state: State<'_, App>) -> CommandResult<Option<String>> {
    let dialog = app.dialog().file().set_title("Choose a backup to restore");
    let Some(path) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(err)?
    else {
        return Ok(None);
    };
    let label = path
        .to_string()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("backup")
        .to_owned();
    let bytes =
        read_picked(&app, path).map_err(|e| format!("That file could not be read ({e})."))?;
    std::fs::write(state.dir.join(PICKED), bytes).map_err(err)?;
    Ok(Some(label))
}

/// What a backup holds. `name` is a backup in the folder; without it, the
/// picked file, opened with this device's key or `recovery`.
#[tauri::command]
pub fn inspect_backup(
    state: State<App>,
    name: Option<String>,
    recovery: Option<String>,
) -> CommandResult<Summary> {
    let path = source(&state, name.as_deref())?;
    with_key(&state, recovery.as_deref(), |k| {
        store::inspect(&path, k, &state.dir)
    })
}

/// Restore a backup over the current data. Returns the name of the backup
/// made of the data it replaced.
#[tauri::command]
pub fn restore_backup(
    state: State<App>,
    name: Option<String>,
    recovery: Option<String>,
) -> CommandResult<String> {
    let path = source(&state, name.as_deref())?;
    let key = state.lock_inner()?.key.ok_or(LOCKED)?;
    let dest = state.dir.join("restore.tmp");
    let _ = std::fs::remove_file(&dest);
    with_key(&state, recovery.as_deref(), |k| {
        store::restore_copy(&path, k, &dest, &key)
    })?;
    let saved = state.swap_in(&dest)?;
    if name.is_none() {
        let _ = std::fs::remove_file(state.dir.join(PICKED));
    }
    Ok(saved)
}
