//! Samriddhi's app shell: creates or opens the encrypted store with the key
//! kept by this device, guards it with an optional app lock (PIN, or
//! fingerprint/face on phones), keeps daily backups, and answers the UI's
//! commands. Local mode only: the core runs in this process.

mod data;
mod export;
mod wealth;

use std::path::PathBuf;
use std::sync::Mutex;

use jiff::{SignedDuration, Timestamp};
use samriddhi_core::export::Export;
use samriddhi_core::keys::{self, Key};
use samriddhi_core::store::{self, Store};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_samriddhi_device::{DeviceExt, KeyStorage};

const DB_FILE: &str = "samriddhi.db";
const BACKUP_DIR: &str = "backups";
/// Wrong PINs allowed before each wait starts.
const FREE_TRIES: u32 = 5;
const MAX_WAIT_SECS: i64 = 15 * 60;
const LOCKED: &str = "Samriddhi is locked.";

type CommandResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Default)]
struct Inner {
    store: Option<Store>,
    key: Option<Key>,
    key_storage: Option<KeyStorage>,
    /// Why the store is not open, in plain words.
    problem: Option<String>,
    /// The app lock is on: the store is open but nothing may read it.
    locked: bool,
    wrong_pins: u32,
    wait_until: Option<Timestamp>,
    /// Something the user should hear about once.
    notice: Option<String>,
    /// An export file read for import, waiting for the user to confirm.
    pending_import: Option<Export>,
}

struct App {
    inner: Mutex<Inner>,
    dir: PathBuf,
}

impl App {
    fn lock_inner(&self) -> CommandResult<std::sync::MutexGuard<'_, Inner>> {
        self.inner.lock().map_err(err)
    }

    /// Run `f` on the open, unlocked store.
    fn with<T>(&self, f: impl FnOnce(&mut Store) -> store::Result<T>) -> CommandResult<T> {
        let mut inner = self.lock_inner()?;
        if inner.locked {
            return Err(LOCKED.into());
        }
        let store = inner
            .store
            .as_mut()
            .ok_or("Your data is not open yet: set up Samriddhi or enter your recovery key.")?;
        f(store).map_err(err)
    }

    fn db(&self) -> PathBuf {
        self.dir.join(DB_FILE)
    }

    /// At start: open the store with the key from this device, if there is
    /// a store. A new install waits for `create_store`.
    fn open(&self, app: &AppHandle) {
        let db = self.db();
        if !db.exists() {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        // Never make a key for a database that exists: it could not open it.
        match app.device().db_key(&self.dir, false) {
            Ok(k) => match Store::open(&db, &k.key) {
                Ok(store) => {
                    let locked = lock_on(&store);
                    *inner = Inner {
                        store: Some(store),
                        key: Some(k.key),
                        key_storage: Some(k.storage),
                        locked,
                        ..Inner::default()
                    }
                }
                Err(e) => {
                    inner.problem = Some(format!("The saved key does not open your data ({e})."))
                }
            },
            Err(e) => inner.problem = Some(format!("{e}.")),
        }
    }

    /// Daily backups: the app folder, then the user's folder. Each runs at
    /// most once a day, so this is safe to call often. Backups need no
    /// unlock: they copy the encrypted data as it is.
    fn maintain(&self) {
        let now = Timestamp::now();
        let Ok(inner) = self.inner.lock() else {
            return;
        };
        let Inner {
            store: Some(store),
            key: Some(key),
            ..
        } = &*inner
        else {
            return;
        };
        let _ = store.daily_backup(&self.dir.join(BACKUP_DIR), key, now);
        if let Some(folder) = get::<String>(store, "backup.folder") {
            let _ = store.daily_backup(std::path::Path::new(&folder), key, now);
        }
    }
}

/// A setting, treating a cleared one (stored as null) as unset.
fn get<T: serde::de::DeserializeOwned>(store: &Store, key: &str) -> Option<T> {
    store.setting::<Option<T>>(key).ok().flatten().flatten()
}

/// The app lock is on when the user set a PIN or turned on biometric unlock.
fn lock_on(store: &Store) -> bool {
    get::<String>(store, "lock.pin").is_some()
        || get::<bool>(store, "lock.biometric").unwrap_or(false)
}

fn now() -> Timestamp {
    Timestamp::now()
}

/// Where the app is, for the first screen to pick what to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Phase {
    /// No data yet: show the welcome and set-up.
    NeedsSetup,
    /// Data exists but cannot be opened here: ask for the recovery key.
    NeedsRecovery,
    /// Open, behind the app lock.
    Locked,
    Open,
}

fn phase(db_exists: bool, inner: &Inner) -> Phase {
    match (&inner.store, inner.locked) {
        (Some(_), true) => Phase::Locked,
        (Some(_), false) => Phase::Open,
        (None, _) if db_exists => Phase::NeedsRecovery,
        (None, _) => Phase::NeedsSetup,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusView {
    phase: Phase,
    problem: Option<String>,
    /// The user has not yet confirmed they saved the recovery key.
    recovery_pending: bool,
    key_storage: Option<KeyStorage>,
    has_pin: bool,
    biometric: bool,
    /// Seconds before another PIN may be tried.
    wait_secs: i64,
    platform: &'static str,
    notice: Option<String>,
}

#[tauri::command]
fn status(state: State<App>) -> CommandResult<StatusView> {
    let mut inner = state.lock_inner()?;
    let notice = inner.notice.take();
    let store = inner.store.as_ref();
    let flag = |k: &str| store.and_then(|s| get::<bool>(s, k)).unwrap_or(false);
    Ok(StatusView {
        phase: phase(state.db().exists(), &inner),
        problem: inner.problem.clone(),
        recovery_pending: store.is_some() && !flag("recovery.saved"),
        key_storage: inner.key_storage,
        has_pin: store.and_then(|s| get::<String>(s, "lock.pin")).is_some(),
        biometric: flag("lock.biometric"),
        wait_secs: inner
            .wait_until
            .map(|t| (t - now()).get_seconds().max(0))
            .unwrap_or(0),
        platform: std::env::consts::OS,
        notice,
    })
}

/// First run: make the database key (kept by this device), create the
/// encrypted store, and return the recovery key for the user to write down.
#[tauri::command]
fn create_store(app: AppHandle, state: State<App>) -> CommandResult<String> {
    let db = state.db();
    if db.exists() {
        return Err("Samriddhi is already set up on this device.".into());
    }
    std::fs::create_dir_all(&state.dir).map_err(err)?;
    let k = app.device().db_key(&state.dir, true).map_err(err)?;
    let store = Store::open(&db, &k.key).map_err(err)?;
    *state.lock_inner()? = Inner {
        store: Some(store),
        key: Some(k.key),
        key_storage: Some(k.storage),
        ..Inner::default()
    };
    state.maintain();
    Ok(keys::recovery_text(&k.key))
}

/// The recovery key again, only until the user confirms they saved it (a
/// crash between set-up and that confirmation must not lose it), and only
/// while unlocked.
#[tauri::command]
fn recovery_key(state: State<App>) -> CommandResult<String> {
    let inner = state.lock_inner()?;
    let (Some(key), Some(store), false) = (&inner.key, &inner.store, inner.locked) else {
        return Err(LOCKED.into());
    };
    if get::<bool>(store, "recovery.saved").unwrap_or(false) {
        return Err("The recovery key was shown once and you confirmed you saved it.".into());
    }
    Ok(keys::recovery_text(key))
}

#[tauri::command]
fn recovery_saved(state: State<App>) -> CommandResult<()> {
    state.with(|s| s.set_setting("recovery.saved", &true))
}

/// Open the data with the recovery key, then keep the key on this device
/// again. The recovery key also clears the app lock.
#[tauri::command]
fn unlock_with_recovery(app: AppHandle, state: State<App>, recovery: String) -> CommandResult<()> {
    let key = keys::parse_recovery(&recovery)
        .ok_or("A recovery key is 64 letters and digits (0-9, a-f).")?;
    let store =
        Store::open(&state.db(), &key).map_err(|_| "That recovery key does not open this data.")?;
    let storage = app.device().set_db_key(&state.dir, &key).map_err(err)?;
    *state.lock_inner()? = Inner {
        store: Some(store),
        key: Some(key),
        key_storage: Some(storage),
        ..Inner::default()
    };
    state.maintain();
    Ok(())
}

/// Set or change the PIN. Only while unlocked.
#[tauri::command]
fn set_pin(state: State<App>, pin: String) -> CommandResult<()> {
    let hash = keys::pin_hash(&pin).map_err(err)?;
    state.with(|s| s.set_setting("lock.pin", &hash))
}

/// Remove the PIN. Only while unlocked.
#[tauri::command]
fn clear_pin(state: State<App>) -> CommandResult<()> {
    state.with(|s| s.set_setting("lock.pin", &None::<String>))
}

/// Try a PIN. After [`FREE_TRIES`] wrong ones each further try waits longer,
/// doubling up to [`MAX_WAIT_SECS`]. The recovery key always works.
#[tauri::command]
fn unlock_pin(state: State<App>, pin: String) -> CommandResult<bool> {
    let ok = state.lock_inner()?.try_pin(&pin, now())?;
    if ok {
        state.maintain();
    }
    Ok(ok)
}

/// Wait before the next PIN try after `wrong` wrong ones in a row: none for
/// the first [`FREE_TRIES`], then 30 s doubling up to [`MAX_WAIT_SECS`].
fn pin_wait(wrong: u32) -> Option<SignedDuration> {
    let extra = wrong.checked_sub(FREE_TRIES)?;
    Some(SignedDuration::from_secs(
        (30i64 << extra.min(10)).min(MAX_WAIT_SECS),
    ))
}

impl Inner {
    /// Check a PIN, keeping count of wrong ones. Unlocks on a match.
    fn try_pin(&mut self, pin: &str, now: Timestamp) -> CommandResult<bool> {
        if let Some(t) = self.wait_until
            && t > now
        {
            return Err(format!(
                "Too many wrong PINs. Try again in {} seconds.",
                (t - now).get_seconds() + 1
            ));
        }
        let hash = self
            .store
            .as_ref()
            .and_then(|s| get::<String>(s, "lock.pin"))
            .ok_or("No PIN is set.")?;
        if keys::pin_check(pin, &hash) {
            self.locked = false;
            self.wrong_pins = 0;
            self.wait_until = None;
            return Ok(true);
        }
        self.wrong_pins += 1;
        self.wait_until = pin_wait(self.wrong_pins).map(|d| now + d);
        Ok(false)
    }
}

/// Unlock with fingerprint or face (phones only).
#[tauri::command]
fn unlock_biometric(app: AppHandle, state: State<App>) -> CommandResult<()> {
    biometric_check(&app, "Unlock Samriddhi")?;
    state.lock_inner()?.locked = false;
    state.maintain();
    Ok(())
}

/// Turn biometric unlock on (after one successful check) or off.
#[tauri::command]
fn set_biometric(app: AppHandle, state: State<App>, on: bool) -> CommandResult<()> {
    if on {
        biometric_check(&app, "Turn on fingerprint or face unlock")?;
    }
    state.with(|s| s.set_setting("lock.biometric", &on))
}

#[cfg(mobile)]
fn biometric_check(app: &AppHandle, reason: &str) -> CommandResult<()> {
    use tauri_plugin_biometric::{AuthOptions, BiometricExt};
    let options = AuthOptions {
        allow_device_credential: true,
        ..AuthOptions::default()
    };
    app.biometric()
        .authenticate(reason.into(), options)
        .map_err(err)
}

#[cfg(desktop)]
fn biometric_check(_app: &AppHandle, _reason: &str) -> CommandResult<()> {
    Err("Fingerprint and face unlock are for phones; use a PIN on this computer.".into())
}

/// Lock now. Does nothing unless a PIN or biometric unlock is set.
#[tauri::command]
fn lock(state: State<App>) -> CommandResult<bool> {
    let mut inner = state.lock_inner()?;
    inner.locked = inner.store.as_ref().is_some_and(lock_on);
    Ok(inner.locked)
}

/// Settings the screens may read and write. Everything else (PIN, backup
/// folder, recovery state) has its own command with its own checks.
const UI_SETTINGS: &[&str] = &[
    "ui.theme",
    "ui.mode",
    "ui.motion",
    "terms.accepted",
    "setup.done",
];

fn ui_setting(key: &str) -> CommandResult<()> {
    UI_SETTINGS
        .contains(&key)
        .then_some(())
        .ok_or_else(|| format!("{key} is not a setting the screens can change."))
}

/// A screen setting and a value it may take.
fn check_setting(key: &str, value: &Value) -> CommandResult<()> {
    ui_setting(key)?;
    let one_of = |allowed: &[&str]| value.as_str().is_some_and(|v| allowed.contains(&v));
    let ok = match key {
        "ui.theme" => value.as_str().is_some_and(|v| {
            !v.is_empty()
                && v.len() <= 40
                && v.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        }),
        "ui.mode" => one_of(&["system", "light", "dark"]),
        "ui.motion" => one_of(&["system", "calm", "full"]),
        _ => value.is_boolean(),
    };
    ok.then_some(())
        .ok_or_else(|| format!("{value} is not a value {key} can take."))
}

#[tauri::command]
fn get_setting(state: State<App>, key: String) -> CommandResult<Option<Value>> {
    ui_setting(&key)?;
    state.with(|s| Ok(get::<Value>(s, &key)))
}

#[tauri::command]
fn put_setting(state: State<App>, key: String, value: Value) -> CommandResult<()> {
    check_setting(&key, &value)?;
    state.with(|s| s.set_setting(&key, &value))
}

/// Desktop: re-check the daily backup every hour while the app runs.
#[cfg(desktop)]
fn upkeep_loop(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
            app.state::<App>().maintain();
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if std::env::args().any(|a| a == "--version") {
        println!("Samriddhi {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        // One window on one database: a second start brings the first forward.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }));
    }
    #[cfg(mobile)]
    {
        builder = builder
            .plugin(tauri_plugin_biometric::init())
            .plugin(tauri_plugin_fs::init());
    }
    builder
        .plugin(tauri_plugin_samriddhi_device::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(dir.join(BACKUP_DIR))?;
            // The database and backups are encrypted; keep their folders private too.
            #[cfg(unix)]
            for d in [dir.clone(), dir.join(BACKUP_DIR)] {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700));
            }
            app.manage(App {
                inner: Mutex::new(Inner::default()),
                dir,
            });
            let handle = app.handle().clone();
            let state = app.state::<App>();
            state.open(&handle);
            state.maintain();
            #[cfg(desktop)]
            upkeep_loop(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            status,
            create_store,
            recovery_key,
            recovery_saved,
            unlock_with_recovery,
            set_pin,
            clear_pin,
            unlock_pin,
            unlock_biometric,
            set_biometric,
            lock,
            get_setting,
            put_setting,
            data::backups,
            data::set_backup_folder,
            data::backup_now,
            data::save_backup_to,
            data::pick_backup,
            data::inspect_backup,
            data::restore_backup,
            wealth::profile_get,
            wealth::set_fact,
            wealth::clear_fact,
            wealth::set_countries,
            wealth::set_goals,
            wealth::risk_questions,
            wealth::save_risk_answers,
            wealth::risk_result,
            wealth::steps_list,
            wealth::step_later,
            wealth::step_not_for_me,
            wealth::step_restore,
            export::export_json,
            export::export_csv,
            export::export_csv_files,
            export::import_pick,
            export::import_apply,
            export::import_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Samriddhi");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_pin(pin: &str) -> Inner {
        let store = Store::memory().unwrap();
        store
            .set_setting("lock.pin", &keys::pin_hash(pin).unwrap())
            .unwrap();
        let locked = lock_on(&store);
        Inner {
            store: Some(store),
            locked,
            ..Inner::default()
        }
    }

    #[test]
    fn waits_grow_after_five_wrong_pins_and_stop_at_fifteen_minutes() {
        assert_eq!(pin_wait(4), None);
        assert_eq!(pin_wait(5), Some(SignedDuration::from_secs(30)));
        assert_eq!(pin_wait(6), Some(SignedDuration::from_secs(60)));
        assert_eq!(pin_wait(9), Some(SignedDuration::from_secs(480)));
        assert_eq!(pin_wait(10), Some(SignedDuration::from_secs(MAX_WAIT_SECS)));
        assert_eq!(
            pin_wait(500),
            Some(SignedDuration::from_secs(MAX_WAIT_SECS))
        );
    }

    #[test]
    fn right_pin_unlocks_and_wrong_ones_lock_out_for_a_while() {
        let t0: Timestamp = "2026-10-29T10:00:00Z".parse().unwrap();
        let mut inner = with_pin("2468");
        assert!(inner.locked);
        for _ in 0..4 {
            assert_eq!(inner.try_pin("1111", t0), Ok(false));
        }
        assert_eq!(inner.wait_until, None);
        assert_eq!(inner.try_pin("1111", t0), Ok(false));
        // Even the right PIN waits until the time is up.
        let err = inner
            .try_pin("2468", t0 + SignedDuration::from_secs(10))
            .unwrap_err();
        assert!(err.contains("Try again in 21 seconds"), "{err}");
        assert!(inner.locked);
        assert_eq!(
            inner.try_pin("2468", t0 + SignedDuration::from_secs(30)),
            Ok(true)
        );
        assert!(!inner.locked);
        assert_eq!((inner.wrong_pins, inner.wait_until), (0, None));
    }

    #[test]
    fn removed_pin_and_biometric_switch_the_lock_off() {
        let inner = with_pin("2468");
        let store = inner.store.as_ref().unwrap();
        store.set_setting("lock.pin", &None::<String>).unwrap();
        assert!(!lock_on(store));
        store.set_setting("lock.biometric", &true).unwrap();
        assert!(lock_on(store));
        store.set_setting("lock.biometric", &false).unwrap();
        assert!(!lock_on(store));
    }

    #[test]
    fn no_pin_set_is_an_error_not_an_unlock() {
        let mut inner = Inner {
            store: Some(Store::memory().unwrap()),
            locked: true,
            ..Inner::default()
        };
        assert_eq!(
            inner.try_pin("0000", Timestamp::now()),
            Err("No PIN is set.".into())
        );
        assert!(inner.locked);
    }

    #[test]
    fn phase_follows_the_store_and_the_lock() {
        let mut inner = Inner::default();
        assert_eq!(phase(false, &inner), Phase::NeedsSetup);
        assert_eq!(phase(true, &inner), Phase::NeedsRecovery);
        inner.store = Some(Store::memory().unwrap());
        assert_eq!(phase(true, &inner), Phase::Open);
        inner.locked = true;
        assert_eq!(phase(true, &inner), Phase::Locked);
    }

    #[test]
    fn screens_can_only_set_their_own_settings_to_sensible_values() {
        use serde_json::json;
        assert!(check_setting("ui.theme", &json!("swarna")).is_ok());
        assert!(check_setting("ui.theme", &json!("Swarna; drop")).is_err());
        assert!(check_setting("ui.mode", &json!("dark")).is_ok());
        assert!(check_setting("ui.mode", &json!("purple")).is_err());
        assert!(check_setting("ui.motion", &json!("calm")).is_ok());
        assert!(check_setting("terms.accepted", &json!(true)).is_ok());
        assert!(check_setting("terms.accepted", &json!("yes")).is_err());
        for key in [
            "lock.pin",
            "lock.biometric",
            "backup.folder",
            "recovery.saved",
        ] {
            let e = check_setting(key, &json!(true)).unwrap_err();
            assert!(e.contains("not a setting"), "{key}");
        }
    }
}
