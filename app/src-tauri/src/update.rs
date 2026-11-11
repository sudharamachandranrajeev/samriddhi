//! Updates and going back a version. Samriddhi checks at start (unless the
//! user turned it off) and always asks first. Every install, forward or back,
//! makes an encrypted backup of the data before it, and every download is
//! checked: on the desktop against the updater signature (the key is in
//! tauri.conf.json), on Android against the release's SHA256SUMS.txt and by
//! Android's own signature check. Ported from Arogya Swasthya.
//!
//! The only addresses this talks to are the project's GitHub releases (the
//! update list and the files it names) and, through the opener, the GitHub
//! release and new-issue pages.

use samriddhi_core::store::PRE_UPDATE;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::data::KEEP_PRE_UPDATE;
use crate::{App, CommandResult, LOCKED, err, get, now};

const REPO: &str = "https://github.com/sudharamachandranrajeev/samriddhi-wealth-manager";

/// How this copy of the app was installed, which decides how it updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallKind {
    /// Windows installer (NSIS).
    Installer,
    AppImage,
    /// Arch Linux package, installed with pacman.
    Pacman,
    /// Windows portable zip: cannot update itself.
    Portable,
    #[cfg_attr(desktop, allow(dead_code))]
    Android,
    /// Built from source, or a package the app does not know how to update.
    Other,
}

impl InstallKind {
    fn can_install(self) -> bool {
        !matches!(self, InstallKind::Portable | InstallKind::Other)
    }
}

pub fn install_kind() -> InstallKind {
    #[cfg(target_os = "android")]
    return InstallKind::Android;
    #[cfg(not(target_os = "android"))]
    {
        use tauri::utils::config::BundleType;
        use tauri::utils::platform::bundle_type;
        let exe = std::env::current_exe().ok();
        if cfg!(windows) {
            // The portable zip ships a file named "portable" next to the program.
            if exe
                .as_ref()
                .and_then(|e| e.parent())
                .is_some_and(|d| d.join("portable").is_file())
            {
                return InstallKind::Portable;
            }
            if bundle_type() == Some(BundleType::Nsis) {
                return InstallKind::Installer;
            }
        }
        if cfg!(target_os = "linux") {
            // The Arch package marks itself: its program is the same file the
            // other Linux bundles start from, so it cannot tell by itself.
            let marked = std::fs::read_to_string("/usr/lib/samriddhi/install-type")
                .is_ok_and(|t| t.trim() == "pacman");
            if marked && exe.as_deref() == Some(std::path::Path::new("/usr/bin/samriddhi")) {
                return InstallKind::Pacman;
            }
            if std::env::var_os("APPIMAGE").is_some() && bundle_type() == Some(BundleType::AppImage)
            {
                return InstallKind::AppImage;
            }
        }
        InstallKind::Other
    }
}

/// "ask": check at start and ask before installing. "off": never check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Ask,
    Off,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    kind: InstallKind,
    can_install: bool,
    current: String,
    /// The version before the last update, which "go back" returns to.
    previous: Option<String>,
    mode: Mode,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
    date: Option<String>,
    /// Download size in bytes, when the update list gives it.
    size: Option<u64>,
    can_install: bool,
}

/// What going back would do, shown before the user confirms.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackPlan {
    version: String,
    /// The backup made just before updating from that version, if kept.
    backup: Option<String>,
    /// That version cannot read the data as it is now, so the backup must
    /// replace it: changes made since the update are then lost.
    needs_restore: bool,
}

fn current(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn previous(state: &App) -> Option<String> {
    state
        .with(|s| Ok(get::<String>(s, "update.previous")))
        .ok()
        .flatten()
}

/// `0.1.10` > `0.1.9`. Anything that is not three numbers counts as 0.0.0.
#[cfg_attr(desktop, allow(dead_code))]
fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> (u64, u64, u64) {
        let n: Vec<u64> = v
            .trim_start_matches('v')
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect();
        (
            n.first().copied().unwrap_or(0),
            n.get(1).copied().unwrap_or(0),
            n.get(2).copied().unwrap_or(0),
        )
    };
    parse(a) > parse(b)
}

fn plain_version(v: &str) -> CommandResult<&str> {
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.') {
        Ok(v)
    } else {
        Err(format!("{v:?} is not a version number."))
    }
}

/// The download size for `target` from the update list's optional
/// `sizes` object (written by the release workflow). The updater names the
/// target by the OS alone ("linux") unless one was set ("linux-x86_64-pacman"),
/// while the list is keyed like the platforms ("linux-x86_64").
fn size_of(manifest: &serde_json::Value, target: &str) -> Option<u64> {
    let key = if target.contains('-') {
        target.to_owned()
    } else {
        format!("{target}-{}", std::env::consts::ARCH)
    };
    manifest.get("sizes")?.get(key)?.as_u64()
}

/// The newest pre-update backup made while `version` was running.
fn pre_update_backup_of(state: &App, version: &str) -> Option<String> {
    let tail = format!("-{version}.db");
    let mut names: Vec<String> = std::fs::read_dir(state.backup_dir())
        .ok()?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(PRE_UPDATE) && n.ends_with(&tail))
        .collect();
    names.sort();
    names.pop()
}

/// Back up before installing, and remember which version to go back to.
fn before_install(app: &AppHandle, state: &App) -> CommandResult<()> {
    let version = current(app);
    let inner = state.lock_inner()?;
    let (Some(store), Some(key), false) = (&inner.store, &inner.key, inner.locked) else {
        return Err(LOCKED.into());
    };
    store
        .pre_update_backup(&state.backup_dir(), key, &version, now(), KEEP_PRE_UPDATE)
        .map_err(|e| {
            format!("The backup before updating failed, so nothing was installed ({e}).")
        })?;
    store.set_setting("update.previous", &version).map_err(err)
}

#[tauri::command]
pub fn update_status(app: AppHandle, state: State<App>) -> UpdateStatus {
    let kind = install_kind();
    UpdateStatus {
        kind,
        can_install: kind.can_install(),
        current: current(&app),
        previous: previous(&state),
        mode: state
            .with(|s| Ok(get(s, "updates.mode")))
            .ok()
            .flatten()
            .unwrap_or_default(),
    }
}

#[tauri::command]
pub fn set_update_mode(state: State<App>, mode: Mode) -> CommandResult<()> {
    state.with(|s| s.set_setting("updates.mode", &mode))
}

/// The fields a problem report may carry, after scrubbing. `theme` must be a
/// theme id; `errors` keeps at most three lines, each scrubbed.
fn report_fields(fields: std::collections::BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (k, v) in fields {
        match k.as_str() {
            "theme" => {
                let ok = !v.is_empty()
                    && v.len() <= 40
                    && v.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
                if ok {
                    out.push((k, v));
                }
            }
            "errors" => {
                let lines: Vec<String> = v
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .take(3)
                    .map(scrub)
                    .collect();
                out.push((k, lines.join("\n")));
            }
            _ => {}
        }
    }
    out
}

/// Open one of the app's own pages: the download page, or a new issue
/// prefilled with the version, system, install type and the scrubbed
/// `fields` (only theme and errors are used).
#[tauri::command]
pub fn open_page(
    app: AppHandle,
    page: String,
    fields: std::collections::BTreeMap<String, String>,
) -> CommandResult<()> {
    let url = match page.as_str() {
        "releases" => format!("{REPO}/releases/latest"),
        "bug" | "idea" => {
            let template = if page == "bug" { "bug.yml" } else { "idea.yml" };
            let mut params = vec![
                ("template".to_owned(), template.to_owned()),
                ("version".to_owned(), current(&app)),
                (
                    "os".to_owned(),
                    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
                ),
                ("install".to_owned(), format!("{:?}", install_kind())),
            ];
            params.extend(report_fields(fields));
            tauri::Url::parse_with_params(&format!("{REPO}/issues/new"), &params)
                .map_err(err)?
                .to_string()
        }
        _ => return Err("Unknown page.".into()),
    };
    app.opener().open_url(url, None::<&str>).map_err(err)
}

/// Currency signs and codes that mark a word as money.
const CURRENCY_SIGNS: &[char] = &['£', '$', '€', '₹', '¥', '﷼'];
const CURRENCY_CODES: &[&str] = &["GBP", "INR", "AED", "USD", "EUR", "Rs", "Rs."];

/// A word that looks like an amount: a currency sign or code stuck to a
/// number, digits with thousands or decimal separators (1,200 or 12.50), or a
/// number with a size suffix (12k, 3m, 2cr).
fn money_like(word: &str) -> bool {
    if !word.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if word.contains(CURRENCY_SIGNS) {
        return true;
    }
    let letters: String = word.chars().filter(|c| c.is_alphabetic()).collect();
    if CURRENCY_CODES
        .iter()
        .any(|c| letters.eq_ignore_ascii_case(c.trim_end_matches('.')))
    {
        return true;
    }
    let c: Vec<char> = word.chars().collect();
    let separated = c
        .windows(3)
        .any(|w| w[0].is_ascii_digit() && matches!(w[1], ',' | '.' | '_') && w[2].is_ascii_digit());
    let suffix = {
        let lower = word.to_ascii_lowercase();
        let digits_end = lower.trim_end_matches(|ch: char| ch.is_ascii_alphabetic());
        digits_end.ends_with(|ch: char| ch.is_ascii_digit())
            && ["k", "m", "bn", "cr", "l", "lakh", "lakhs", "crore"]
                .contains(&&lower[digits_end.len()..])
    };
    separated || suffix
}

/// One error line, made safe to share: anything in quotes (an account or
/// person's name, a label the user typed) is cut, file paths and e-mail
/// addresses are replaced, anything that looks like an amount becomes
/// `<amount>`, ids become `<id>`, any other digits become `#`, and the line
/// is cut to 300 characters.
fn scrub(line: &str) -> String {
    let mut text = String::new();
    let mut quoted = None;
    for c in line.chars() {
        match (quoted, c) {
            (None, '"' | '“' | '\'' | '‘') => {
                quoted = Some(match c {
                    '“' => '”',
                    '‘' => '’',
                    c => c,
                });
                text.push('…');
            }
            (Some(end), c) if c == end => quoted = None,
            (Some(_), _) => {}
            (None, c) => text.push(c),
        }
    }
    let words: Vec<String> = text
        .split(' ')
        .map(|w| {
            let core = w.trim_end_matches([',', '.', ':', ';', ')', '(']);
            let core = core.trim_start_matches(['(', '-', '+']);
            let tail = &w[w.trim_end_matches([',', '.', ':', ';', ')']).len()..];
            if core.contains(['/', '\\']) {
                return format!("<path>{tail}");
            }
            if core.contains('@') {
                return format!("<address>{tail}");
            }
            if core.len() >= 16 && core.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                return format!("<id>{tail}");
            }
            if money_like(core) {
                return format!("<amount>{tail}");
            }
            let mut out = String::new();
            for c in w.chars() {
                if !c.is_ascii_digit() {
                    out.push(c);
                } else if !out.ends_with('#') {
                    out.push('#');
                }
            }
            out
        })
        .collect();
    words.join(" ").chars().take(300).collect()
}

#[cfg(desktop)]
fn updater(
    app: &AppHandle,
    kind: InstallKind,
    exact: Option<String>,
) -> CommandResult<tauri_plugin_updater::Updater> {
    use tauri_plugin_updater::UpdaterExt;
    let mut b = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(30));
    if kind == InstallKind::Pacman {
        b = b.target("linux-x86_64-pacman");
    }
    if let Some(v) = exact {
        // That release's own manifest, so going back is checked like going
        // forward: .../releases/latest/download/latest.json becomes
        // .../releases/download/v<version>/latest.json.
        let latest = app
            .config()
            .plugins
            .0
            .get("updater")
            .and_then(|u| u.get("endpoints")?.get(0)?.as_str().map(str::to_owned))
            .ok_or("No update address is configured.")?;
        let url = latest
            .replace(
                "/releases/latest/download/",
                &format!("/releases/download/v{v}/"),
            )
            .parse()
            .map_err(err)?;
        b = b
            .endpoints(vec![url])
            .map_err(err)?
            .version_comparator(move |_, r| r.version.to_string() == v);
    }
    b.build().map_err(err)
}

#[cfg(mobile)]
#[derive(Deserialize)]
struct Manifest {
    version: String,
    notes: Option<String>,
    pub_date: Option<String>,
    #[serde(default)]
    sizes: serde_json::Value,
}

/// Is there a newer version? Checks even when this copy cannot install it,
/// so the portable version can say so.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> CommandResult<Option<UpdateInfo>> {
    let kind = install_kind();
    #[cfg(desktop)]
    {
        let found = updater(&app, kind, None)?
            .check()
            .await
            .map_err(|e| format!("Could not check for updates ({e})."))?;
        Ok(found.map(|u| UpdateInfo {
            size: size_of(&u.raw_json, &u.target),
            version: u.version,
            notes: u.body,
            date: u.date.map(|d| d.to_string()),
            can_install: kind.can_install(),
        }))
    }
    #[cfg(mobile)]
    {
        use tauri_plugin_samriddhi_device::DeviceExt;
        let text = app
            .device()
            .fetch_text(&format!("{REPO}/releases/latest/download/latest.json"))
            .map_err(err)?;
        let m: Manifest = serde_json::from_str(&text)
            .map_err(|e| format!("The update list could not be read ({e})."))?;
        let size = size_of(&serde_json::json!({ "sizes": m.sizes }), "android-aarch64");
        Ok(newer(&m.version, &current(&app)).then(|| UpdateInfo {
            version: m.version,
            notes: m.notes,
            date: m.pub_date,
            size,
            can_install: kind.can_install(),
        }))
    }
}

/// Download, check and install `version` (forward, or back when `back`).
async fn install(app: &AppHandle, state: &App, version: &str, back: bool) -> CommandResult<()> {
    let kind = install_kind();
    if !kind.can_install() {
        return Err(
            "This copy of Samriddhi cannot update itself. Download the new version instead.".into(),
        );
    }
    let version = plain_version(version)?.to_owned();
    #[cfg(desktop)]
    {
        use tauri::Emitter;
        let exact = back.then(|| version.clone());
        let update = updater(app, kind, exact)?
            .check()
            .await
            .map_err(|e| format!("Could not reach the update server ({e})."))?
            .filter(|u| u.version == version)
            .ok_or(format!("Version {version} is not available."))?;
        let mut done = 0usize;
        let progress = app.clone();
        let bytes = update
            .download(
                move |n, total| {
                    done += n;
                    let _ = progress.emit("update-progress", (done, total));
                },
                || {},
            )
            .await
            .map_err(|e| {
                format!("The download failed or did not pass the signature check ({e}).")
            })?;
        before_install(app, state)?;
        if kind == InstallKind::Pacman {
            install_pacman(state, &version, &bytes)?;
        } else {
            // On Windows this starts the installer and closes the app.
            update
                .install(&bytes)
                .map_err(|e| format!("The update could not be installed ({e})."))?;
        }
        app.restart();
    }
    #[cfg(mobile)]
    {
        use tauri_plugin_samriddhi_device::DeviceExt;
        if back {
            return Err("Android cannot install an older version over a newer one. Restore the backup from before the update instead.".into());
        }
        before_install(app, state)?;
        let name = format!("Samriddhi_{version}_aarch64.apk");
        let base = format!("{REPO}/releases/download/v{version}");
        app.device()
            .install_apk(
                &format!("{base}/{name}"),
                &format!("{base}/SHA256SUMS.txt"),
                &name,
            )
            .map_err(err)?;
        Ok(())
    }
}

/// Install the signed package with pacman, asking for the password through
/// the desktop's own prompt (pkexec).
#[cfg(desktop)]
fn install_pacman(state: &App, version: &str, bytes: &[u8]) -> CommandResult<()> {
    let dir = state.dir.join("updates");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let file = dir.join(format!("samriddhi-{version}-1-x86_64.pkg.tar.zst"));
    std::fs::write(&file, bytes).map_err(err)?;
    let out = std::process::Command::new("pkexec")
        .args(["pacman", "-U", "--noconfirm"])
        .arg(&file)
        .output()
        .map_err(|e| {
            format!("Could not ask for the password to install ({e}). Is polkit installed?")
        })?;
    let _ = std::fs::remove_dir_all(&dir);
    if !out.status.success() {
        return Err(match out.status.code() {
            Some(126) | Some(127) => "Installing was cancelled: the password was not given.".into(),
            _ => format!(
                "pacman could not install the update: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, App>,
    version: String,
) -> CommandResult<()> {
    install(&app, &state, &version, false).await
}

#[tauri::command]
pub fn rollback_plan(state: State<App>) -> CommandResult<Option<RollbackPlan>> {
    let Some(version) = previous(&state) else {
        return Ok(None);
    };
    let backup = pre_update_backup_of(&state, &version);
    let now = state.with(|s| s.schema_version())?;
    let needs_restore = match &backup {
        Some(b) => {
            let key = state.lock_inner()?.key.ok_or(LOCKED)?;
            let path = state.backup_dir().join(b);
            samriddhi_core::store::inspect(&path, &key, &state.dir)
                .map(|s| s.schema < now)
                .unwrap_or(true)
        }
        // Without the backup we cannot tell; the older version refuses data
        // it cannot read, and offers a restore.
        None => false,
    };
    Ok(Some(RollbackPlan {
        version,
        backup,
        needs_restore,
    }))
}

/// Go back to the version before the last update. When that version cannot
/// read today's data, `restore` must be true: the pre-update backup then
/// replaces the data at the next start (the current data is backed up
/// first, as before any install).
#[tauri::command]
pub async fn rollback(app: AppHandle, state: State<'_, App>, restore: bool) -> CommandResult<()> {
    let plan = rollback_plan(state.clone())?.ok_or("There is no earlier version to go back to.")?;
    if plan.needs_restore {
        if !restore {
            return Err(
                "Going back needs the backup from before the update. Confirm to use it.".into(),
            );
        }
        let name = plan
            .backup
            .as_deref()
            .ok_or("The backup from before the update is missing.")?;
        state.stage_restore(name)?;
    }
    let r = install(&app, &state, &plan.version, true).await;
    if r.is_err() && plan.needs_restore {
        state.cancel_staged_restore();
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("garbage", "0.0.1"));
    }

    #[test]
    fn error_lines_lose_amounts_names_paths_and_numbers() {
        // Amounts in any of the app's currencies, however written.
        for (line, want) in [
            (
                "cash of £12,500.00 is below costs",
                "cash of <amount> is below costs",
            ),
            ("target ₹5,00,000 by 2030", "target <amount> by #"),
            (
                "balance AED 4500.75 too high",
                "balance AED <amount> too high",
            ),
            ("debt GBP1200 at 22%", "debt <amount> at #%"),
            ("pay 1200INR now", "pay <amount> now"),
            ("worth $3.2m (rough)", "worth <amount> (rough)"),
            (
                "about 12k, or 2cr in all",
                "about <amount>, or <amount> in all",
            ),
            ("-£40 overdrawn", "<amount> overdrawn"),
            ("1,234 rows", "<amount> rows"),
        ] {
            assert_eq!(scrub(line), want, "{line}");
        }
        // Account and people names arrive in quotes; ids, paths and addresses go too.
        assert_eq!(
            scrub("Could not save “Joint ISA with Priya”: disk full"),
            "Could not save …: disk full"
        );
        assert_eq!(
            scrub("no goal with id 3f2a9c1e-7b4d-4e8a-9f00-123456789abc, skipped"),
            "no goal with id <id>, skipped"
        );
        assert_eq!(
            scrub("Could not write a backup to /home/user/Backups: denied"),
            "Could not write a backup to <path>: denied"
        );
        assert_eq!(
            scrub("sent by someone@example.com today"),
            "sent by <address> today"
        );
        // No digit survives a scrub.
        let out = scrub("schema 3 needs 0.1.1; 42 left of 500");
        assert!(!out.chars().any(|c| c.is_ascii_digit()), "{out}");
        assert_eq!(scrub(&"x".repeat(400)).chars().count(), 300);
    }

    #[test]
    fn reports_keep_only_a_theme_id_and_three_scrubbed_errors() {
        let mut f = std::collections::BTreeMap::new();
        f.insert("theme".to_owned(), "swarna".to_owned());
        f.insert(
            "errors".to_owned(),
            "a £10 one\n\nb “Savings” two\nc 3 three\nd four".to_owned(),
        );
        f.insert("netWorth".to_owned(), "£250,000".to_owned());
        f.insert("profile".to_owned(), "lives in GB".to_owned());
        let got = report_fields(f);
        assert_eq!(
            got,
            [
                (
                    "errors".to_owned(),
                    "a <amount> one\nb … two\nc # three".to_owned()
                ),
                ("theme".to_owned(), "swarna".to_owned()),
            ]
        );
        let mut bad = std::collections::BTreeMap::new();
        bad.insert("theme".to_owned(), "Swarna £5".to_owned());
        assert!(report_fields(bad).is_empty());
    }

    #[test]
    fn sizes_come_from_the_update_list_when_given() {
        let m = serde_json::json!({ "sizes": { "linux-x86_64": 1234, "linux-x86_64-pacman": 99 } });
        assert_eq!(size_of(&m, "linux-x86_64"), Some(1234));
        assert_eq!(size_of(&m, "linux-x86_64-pacman"), Some(99));
        assert_eq!(size_of(&m, "windows-x86_64"), None);
        if std::env::consts::ARCH == "x86_64" {
            // The updater says only "linux" for the AppImage.
            assert_eq!(size_of(&m, "linux"), Some(1234));
        }
        assert_eq!(size_of(&serde_json::json!({}), "linux-x86_64"), None);
    }

    #[test]
    fn only_plain_versions_reach_a_url() {
        assert!(plain_version("0.1.2").is_ok());
        for bad in ["", "0.1.2/../../x", "0.1.2?x", "v0.1.2"] {
            assert!(plain_version(bad).is_err(), "{bad}");
        }
    }
}
