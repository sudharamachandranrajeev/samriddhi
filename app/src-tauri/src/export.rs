//! Full export (JSON, and CSV per table) and import from an export. An
//! import is shown as a preview of counts first, and the current data is
//! backed up before it is replaced.

use std::io::Write;

use samriddhi_core::export::{Export, ImportPreview};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::data::{BEFORE_IMPORT, read_picked};
use crate::{App, CommandResult, err, now};

const KEEP_BEFORE_IMPORT: usize = 5;
/// Exports are small; anything bigger is not one.
const MAX_IMPORT_BYTES: usize = 50 * 1024 * 1024;

#[cfg(mobile)]
fn create_target(app: &AppHandle, target: &str) -> std::io::Result<std::fs::File> {
    use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
    let path: FilePath = target.parse().map_err(std::io::Error::other)?;
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    app.fs().open(path, opts)
}

#[cfg(desktop)]
fn create_target(_app: &AppHandle, target: &str) -> std::io::Result<std::fs::File> {
    std::fs::File::create(target)
}

/// Save everything as one JSON file where the user chose (a path, or a
/// content URI on Android). Plain JSON: the user chose to take a readable
/// copy out.
#[tauri::command]
pub fn export_json(app: AppHandle, state: State<App>, target: String) -> CommandResult<()> {
    let export = state.with(|s| s.export_all(now()))?;
    let mut out = std::io::BufWriter::new(
        create_target(&app, &target).map_err(|e| format!("Could not save it there: {e}"))?,
    );
    serde_json::to_writer_pretty(&mut out, &export)
        .map_err(std::io::Error::from)
        .and_then(|()| out.flush())
        .map_err(|e| format!("Could not save the export: {e}"))
}

/// Desktop: write one CSV file per table into a new folder inside `folder`.
/// Returns the new folder.
#[tauri::command]
pub fn export_csv(state: State<App>, folder: String) -> CommandResult<String> {
    let base = std::path::Path::new(&folder);
    if !base.is_absolute() || !base.is_dir() {
        return Err(format!("{folder} is not a folder on this device."));
    }
    let t = now();
    let files = state.with(|s| s.export_csv(t))?;
    let dir = base.join(format!(
        "samriddhi-export-{}",
        t.strftime("%Y-%m-%d-%H%M%S")
    ));
    std::fs::create_dir(&dir).map_err(|e| format!("Could not make {}: {e}", dir.display()))?;
    for (name, body) in files {
        std::fs::write(dir.join(name), body).map_err(err)?;
    }
    Ok(dir.display().to_string())
}

/// The CSV files as (name, contents), for screens that save each one
/// through the system's save dialog (Android).
#[tauri::command]
pub fn export_csv_files(state: State<App>) -> CommandResult<Vec<(String, String)>> {
    state.with(|s| s.export_csv(now()))
}

/// Let the user pick an export file, check it, and say what importing it
/// would change. Nothing changes until `import_apply`. `None` when cancelled.
#[tauri::command]
pub async fn import_pick(
    app: AppHandle,
    state: State<'_, App>,
) -> CommandResult<Option<ImportPreview>> {
    let dialog = app
        .dialog()
        .file()
        .set_title("Choose a Samriddhi export to import")
        .add_filter("Samriddhi export", &["json"]);
    let Some(path) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(err)?
    else {
        return Ok(None);
    };
    let bytes =
        read_picked(&app, path).map_err(|e| format!("That file could not be read ({e})."))?;
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err("That file is too big to be a Samriddhi export.".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "That file is not a Samriddhi export.")?;
    let export = Export::from_json(&text).map_err(|_| "That file is not a Samriddhi export.")?;
    let preview = state.with(|s| s.preview_import(&export))?;
    state.lock_inner()?.pending_import = Some(export);
    Ok(Some(preview))
}

/// Import the picked export, after backing up the current data. Returns the
/// name of that backup.
#[tauri::command]
pub fn import_apply(state: State<App>) -> CommandResult<String> {
    let export = state
        .lock_inner()?
        .pending_import
        .take()
        .ok_or("Choose an export file first.")?;
    let saved = state.backup_as(BEFORE_IMPORT, KEEP_BEFORE_IMPORT)?;
    state.with(|s| s.import(&export, now()))?;
    Ok(saved)
}

#[tauri::command]
pub fn import_cancel(state: State<App>) -> CommandResult<()> {
    state.lock_inner()?.pending_import = None;
    Ok(())
}
