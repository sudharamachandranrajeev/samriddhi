//! The encrypted local store (SQLCipher). Every table is prefixed `sam_` so
//! it can sit beside other Brahma apps' tables without a clash. The schema
//! version lives in `sam_meta`; migrations run in order, each in its own
//! transaction. Backups are encrypted copies made with `sqlcipher_export`.
//! Dates are UTC ISO strings ("2026-10-30T09:00:00Z").

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::keys::Key;
use crate::money::MoneyError;

/// Daily backups kept in each backup folder.
pub const KEEP_BACKUPS: usize = 14;
const BACKUP_PREFIX: &str = "samriddhi-";
/// Name prefix of the backups made before an update.
pub const PRE_UPDATE: &str = "pre-update-";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the key does not open this database")]
    WrongKey,
    #[error("this database was made by a newer Samriddhi (schema {0}); update the app")]
    TooNew(i64),
    #[error("{0} already exists")]
    Exists(PathBuf),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Money(#[from] MoneyError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Schema steps; index + 1 is the version they lead to. Never edit a step
/// that has shipped: add a new one, and a test for it.
pub(crate) const MIGRATIONS: &[&str] = &["
    CREATE TABLE sam_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    INSERT INTO sam_meta (key, value) VALUES ('app', 'samriddhi');
    CREATE TABLE sam_settings (key TEXT PRIMARY KEY, json TEXT NOT NULL);
    -- One row per fact the user gave or the app worked out.
    CREATE TABLE sam_profile (
        key TEXT PRIMARY KEY,
        json TEXT NOT NULL,
        source TEXT NOT NULL CHECK (source IN ('wizard', 'asked', 'derived')),
        asked_at TEXT NOT NULL,
        why TEXT NOT NULL
    );
    CREATE TABLE sam_countries (
        code TEXT PRIMARY KEY CHECK (length(code) = 2 AND code = upper(code)),
        residence INTEGER NOT NULL DEFAULT 0 CHECK (residence IN (0, 1)),
        tax_home INTEGER NOT NULL DEFAULT 0 CHECK (tax_home IN (0, 1)),
        citizen INTEGER NOT NULL DEFAULT 0 CHECK (citizen IN (0, 1)),
        holds_money INTEGER NOT NULL DEFAULT 0 CHECK (holds_money IN (0, 1)),
        ties TEXT NOT NULL DEFAULT '',
        source TEXT NOT NULL CHECK (source IN ('wizard', 'asked', 'derived')),
        why TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    -- One tax home at most.
    CREATE UNIQUE INDEX sam_countries_tax_home ON sam_countries(tax_home) WHERE tax_home = 1;
    CREATE TABLE sam_goals (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        rank INTEGER NOT NULL,
        target_minor INTEGER,
        target_currency TEXT,
        target_date TEXT,
        source TEXT NOT NULL CHECK (source IN ('wizard', 'asked', 'derived')),
        why TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        CHECK ((target_minor IS NULL) = (target_currency IS NULL))
    );
    CREATE TABLE sam_risk_answers (
        question_id TEXT PRIMARY KEY,
        option_id TEXT NOT NULL,
        version INTEGER NOT NULL,
        answered_at TEXT NOT NULL
    );
    CREATE TABLE sam_steps_dismissed (
        step_id TEXT PRIMARY KEY,
        kind TEXT NOT NULL CHECK (kind IN ('later', 'notForMe')),
        reason TEXT,
        at TEXT NOT NULL
    );
    -- Who read or wrote what; the always-on brain adds remote readers later.
    CREATE TABLE sam_audit (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        at TEXT NOT NULL,
        actor TEXT NOT NULL,
        action TEXT NOT NULL CHECK (action IN ('read', 'write')),
        what TEXT NOT NULL
    );
"];

/// Every table, in the order exports list them.
pub const TABLES: &[&str] = &[
    "sam_meta",
    "sam_settings",
    "sam_profile",
    "sam_countries",
    "sam_goals",
    "sam_risk_answers",
    "sam_steps_dismissed",
    "sam_audit",
];

/// What a backup holds, shown before restoring it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub schema: i64,
    /// Rows in each table.
    pub counts: BTreeMap<String, i64>,
}

pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// Open (or create) the database at `path` with a 32-byte key.
    pub fn open(path: &Path, key: &Key) -> Result<Store> {
        let conn = Connection::open(path)?;
        conn.execute_batch(&format!("PRAGMA key = \"{}\";", key_literal(key)))?;
        Store::init(conn)
    }

    /// An in-memory store for tests and previews.
    pub fn memory() -> Result<Store> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        readable(&conn)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<i64> {
        schema_of(&self.conn)
    }

    fn migrate(&mut self) -> Result<()> {
        migrate_to(&mut self.conn, MIGRATIONS.len())
    }

    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let j: Option<String> = self
            .conn
            .query_row("SELECT json FROM sam_settings WHERE key = ?", [key], |r| {
                r.get(0)
            })
            .optional()?;
        j.map(|j| Ok(serde_json::from_str(&j)?)).transpose()
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sam_settings (key, json) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET json = ?2",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    /// Note who read or wrote what. `actor` is "local" for this device.
    pub fn audit(&self, actor: &str, action: Action, what: &str, now: Timestamp) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sam_audit (at, actor, action, what) VALUES (?, ?, ?, ?)",
            params![iso(now), actor, action.as_str(), what],
        )?;
        Ok(())
    }

    /// The newest `limit` audit rows, newest first: (at, actor, action, what).
    pub fn audit_log(&self, limit: usize) -> Result<Vec<(String, String, String, String)>> {
        let mut st = self
            .conn
            .prepare("SELECT at, actor, action, what FROM sam_audit ORDER BY seq DESC LIMIT ?")?;
        let rows = st.query_map([limit as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Write an encrypted copy to `path` (which must not exist), locked with `key`.
    pub fn export(&self, path: &Path, key: &Key) -> Result<()> {
        export_conn(&self.conn, path, key)
    }

    /// Today's backup into `dir`, keeping the newest [`KEEP_BACKUPS`]. Call it
    /// for the app's own folder and again for the folder the user picked.
    /// Returns the new file, or `None` if today's backup already exists.
    pub fn daily_backup(&self, dir: &Path, key: &Key, now: Timestamp) -> Result<Option<PathBuf>> {
        std::fs::create_dir_all(dir)?;
        let name = format!("{BACKUP_PREFIX}{}.db", now.strftime("%Y-%m-%d"));
        let path = dir.join(&name);
        if path.exists() {
            return Ok(None);
        }
        let tmp = dir.join(format!("{name}.partial"));
        let _ = std::fs::remove_file(&tmp);
        self.export(&tmp, key)?;
        std::fs::rename(&tmp, &path)?;
        prune(dir, BACKUP_PREFIX, KEEP_BACKUPS)?;
        Ok(Some(path))
    }

    /// Backup made just before installing another version, named after the
    /// version running now, so going back can restore it. Kept apart from the
    /// daily ones: the newest `keep` stay.
    pub fn pre_update_backup(
        &self,
        dir: &Path,
        key: &Key,
        version: &str,
        now: Timestamp,
        keep: usize,
    ) -> Result<PathBuf> {
        if version.is_empty()
            || !version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        {
            return Err(Error::Invalid(format!("odd version {version:?}")));
        }
        std::fs::create_dir_all(dir)?;
        let name = format!(
            "{PRE_UPDATE}{}-{version}.db",
            now.strftime("%Y-%m-%d-%H%M%S")
        );
        let path = dir.join(&name);
        let tmp = dir.join(format!("{name}.partial"));
        let _ = std::fs::remove_file(&tmp);
        self.export(&tmp, key)?;
        std::fs::rename(&tmp, &path)?;
        prune(dir, PRE_UPDATE, keep)?;
        Ok(path)
    }

    pub fn summary(&self) -> Result<Summary> {
        summarise(&self.conn)
    }
}

/// Reads or writes, for the audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Read,
    Write,
}

impl Action {
    fn as_str(&self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Write => "write",
        }
    }
}

/// A wrong key only shows when the first page is read.
fn readable(conn: &Connection) -> Result<()> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map(drop)
    .map_err(|_| Error::WrongKey)
}

/// The schema version: 0 for a new, empty database.
fn schema_of(conn: &Connection) -> Result<i64> {
    let has_meta: bool = conn.query_row(
        "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'sam_meta'",
        [],
        |r| r.get(0),
    )?;
    if !has_meta {
        return Ok(0);
    }
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM sam_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    v.map_or(Ok(0), |v| {
        v.parse()
            .map_err(|_| Error::Invalid(format!("odd schema version {v:?}")))
    })
}

/// Run the steps after the current version up to `target`. A database newer
/// than this app is refused, never touched.
fn migrate_to(conn: &mut Connection, target: usize) -> Result<()> {
    let v = schema_of(conn)?;
    if v > MIGRATIONS.len() as i64 {
        return Err(Error::TooNew(v));
    }
    for (i, step) in MIGRATIONS.iter().enumerate().take(target).skip(v as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(step)?;
        tx.execute(
            "INSERT INTO sam_meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            [(i + 1).to_string()],
        )?;
        tx.commit()?;
    }
    Ok(())
}

/// Backup files in `dir`, in no particular order.
pub fn backups(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_backup(p, BACKUP_PREFIX))
        .collect())
}

fn is_backup(p: &Path, prefix: &str) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".db"))
}

/// Keep the newest `keep` backups whose names start with `prefix`. Names
/// carry the date first, so name order is age order.
fn prune(dir: &Path, prefix: &str, keep: usize) -> Result<()> {
    let mut all: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_backup(p, prefix))
        .collect();
    all.sort();
    let excess = all.len().saturating_sub(keep);
    for old in &all[..excess] {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

fn export_conn(conn: &Connection, path: &Path, key: &Key) -> Result<()> {
    if path.exists() {
        return Err(Error::Exists(path.into()));
    }
    let p = path
        .to_str()
        .ok_or_else(|| Error::Io(std::io::Error::other("backup path is not UTF-8")))?;
    conn.execute(
        "ATTACH DATABASE ?1 AS backup KEY ?2",
        params![p, key_literal(key)],
    )?;
    let r = conn.query_row("SELECT sqlcipher_export('backup')", [], |_| Ok(()));
    conn.execute("DETACH DATABASE backup", [])?;
    Ok(r?)
}

fn summarise(conn: &Connection) -> Result<Summary> {
    let schema = schema_of(conn)?;
    let mut counts = BTreeMap::new();
    for table in TABLES {
        let exists: bool = conn.query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?",
            [table],
            |r| r.get(0),
        )?;
        if exists {
            // Table names come from TABLES, never from outside.
            let n: i64 =
                conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            counts.insert((*table).to_owned(), n);
        }
    }
    Ok(Summary { schema, counts })
}

/// Open a copy of the backup at `src` with `key` (in `work`, removed again)
/// and say what it holds. `src` itself is only read.
pub fn inspect(src: &Path, key: &Key, work: &Path) -> Result<Summary> {
    let copy = work.join("inspect.db");
    let r = with_copy(src, key, &copy, summarise);
    let _ = std::fs::remove_file(&copy);
    r
}

/// Turn the backup at `src` (opened with `src_key`, which may be another
/// device's) into a database locked with `key`, written to `dest` (which must
/// not exist). Refuses a backup from a newer app. `src` itself is only read;
/// the caller swaps `dest` in.
pub fn restore_copy(src: &Path, src_key: &Key, dest: &Path, key: &Key) -> Result<Summary> {
    let copy = dest.with_extension("source");
    let r = with_copy(src, src_key, &copy, |conn| {
        let sum = summarise(conn)?;
        export_conn(conn, dest, key)?;
        Ok(sum)
    });
    let _ = std::fs::remove_file(&copy);
    if r.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    r
}

fn with_copy<T>(
    src: &Path,
    key: &Key,
    copy: &Path,
    f: impl FnOnce(&Connection) -> Result<T>,
) -> Result<T> {
    let _ = std::fs::remove_file(copy);
    std::fs::copy(src, copy)?;
    let conn = Connection::open(copy)?;
    conn.execute_batch(&format!("PRAGMA key = \"{}\";", key_literal(key)))?;
    readable(&conn)?;
    let v = schema_of(&conn)?;
    if v > MIGRATIONS.len() as i64 {
        return Err(Error::TooNew(v));
    }
    f(&conn)
}

/// SQLCipher's raw-key form: x'<64 hex digits>', used as-is with no key derivation.
fn key_literal(key: &Key) -> String {
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    format!("x'{hex}'")
}

/// A UTC ISO string, the form every date column holds.
pub fn iso(t: Timestamp) -> String {
    t.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jiff::SignedDuration;

    pub(crate) fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }
    pub(crate) const KEY: Key = [7; 32];
    pub(crate) const NOW: &str = "2026-11-04T09:00:00Z";

    fn tables(conn: &Connection) -> Vec<String> {
        let mut st = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'sam_%' ORDER BY name")
            .unwrap();
        st.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn migration_1_makes_the_phase_1_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(schema_of(&conn).unwrap(), 0);
        migrate_to(&mut conn, 1).unwrap();
        assert_eq!(schema_of(&conn).unwrap(), 1);
        let mut want: Vec<String> = TABLES.iter().map(|t| t.to_string()).collect();
        want.sort();
        assert_eq!(tables(&conn), want);
        let app: String = conn
            .query_row("SELECT value FROM sam_meta WHERE key = 'app'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(app, "samriddhi");
        // The checks hold: one tax home, known sources, known dismissal kinds.
        let add = |code: &str, tax_home: i64| {
            conn.execute(
                "INSERT INTO sam_countries (code, tax_home, source, why, updated_at) VALUES (?, ?, 'wizard', 'w', 't')",
                params![code, tax_home],
            )
        };
        add("GB", 1).unwrap();
        add("IN", 0).unwrap();
        assert!(add("AE", 1).is_err(), "a second tax home");
        assert!(add("gb", 0).is_err(), "lower-case code");
        assert!(
            conn.execute(
                "INSERT INTO sam_profile (key, json, source, asked_at, why) VALUES ('k', '1', 'guessed', 't', 'w')",
                [],
            )
            .is_err()
        );
        assert!(
            conn.execute(
                "INSERT INTO sam_steps_dismissed (step_id, kind, at) VALUES ('s', 'never', 't')",
                [],
            )
            .is_err()
        );
        assert!(
            conn.execute(
                "INSERT INTO sam_goals (id, kind, rank, target_minor, source, why, updated_at) VALUES ('g', 'k', 1, 5, 'wizard', 'w', 't')",
                [],
            )
            .is_err(),
            "an amount needs its currency"
        );
    }

    #[test]
    fn migrations_run_once_and_survive_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("samriddhi.db");
        let s = Store::open(&db, &KEY).unwrap();
        assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len() as i64);
        s.set_setting("ui.theme", &"swarna").unwrap();
        drop(s);
        let s = Store::open(&db, &KEY).unwrap();
        assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(
            s.setting::<String>("ui.theme").unwrap().as_deref(),
            Some("swarna")
        );
    }

    #[test]
    fn wrong_key_and_newer_schema_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("samriddhi.db");
        drop(Store::open(&db, &KEY).unwrap());
        assert!(matches!(Store::open(&db, &[1; 32]), Err(Error::WrongKey)));
        let s = Store::open(&db, &KEY).unwrap();
        s.conn
            .execute(
                "UPDATE sam_meta SET value = '99' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        drop(s);
        assert!(matches!(Store::open(&db, &KEY), Err(Error::TooNew(99))));
    }

    #[test]
    fn settings_round_trip_and_overwrite() {
        let s = Store::memory().unwrap();
        assert_eq!(s.setting::<bool>("x").unwrap(), None);
        s.set_setting("x", &true).unwrap();
        s.set_setting("x", &false).unwrap();
        assert_eq!(s.setting::<bool>("x").unwrap(), Some(false));
    }

    #[test]
    fn audit_keeps_who_did_what_newest_first() {
        let s = Store::memory().unwrap();
        s.audit("local", Action::Write, "profile", at(NOW)).unwrap();
        s.audit("local", Action::Read, "export", at(NOW)).unwrap();
        let log = s.audit_log(10).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].2, "read");
        assert_eq!(log[0].0, "2026-11-04T09:00:00Z");
        assert_eq!(s.audit_log(1).unwrap().len(), 1);
    }

    #[test]
    fn encrypted_daily_backups_inspect_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("samriddhi.db");
        let backups_dir = dir.path().join("backups");
        let s = Store::open(&db, &KEY).unwrap();
        s.set_setting("marker", &"Zebracorn").unwrap();
        let day = at(NOW);
        let first = s.daily_backup(&backups_dir, &KEY, day).unwrap().unwrap();
        assert!(
            s.daily_backup(&backups_dir, &KEY, day).unwrap().is_none(),
            "one a day"
        );
        for d in 1..=20 {
            s.daily_backup(&backups_dir, &KEY, day + SignedDuration::from_hours(24 * d))
                .unwrap();
        }
        assert_eq!(backups(&backups_dir).unwrap().len(), KEEP_BACKUPS);
        assert!(!first.exists(), "oldest backups are removed");
        let keep = backups(&backups_dir).unwrap().into_iter().min().unwrap();

        // The backup is encrypted: the wrong key cannot open it, and its bytes do not show the data.
        assert!(matches!(Store::open(&keep, &[9; 32]), Err(Error::WrongKey)));
        assert!(
            !std::fs::read(&keep)
                .unwrap()
                .windows(9)
                .any(|w| w == b"Zebracorn")
        );

        // What the backup holds, read without touching it.
        let sum = inspect(&keep, &KEY, dir.path()).unwrap();
        assert_eq!(sum.schema, MIGRATIONS.len() as i64);
        assert_eq!(sum.counts["sam_settings"], 1);
        assert_eq!(sum.counts.len(), TABLES.len());
        assert!(matches!(
            inspect(&keep, &[9; 32], dir.path()),
            Err(Error::WrongKey)
        ));

        // A backup becomes a database locked with another device's key.
        s.set_setting("marker", &"changed").unwrap();
        let dest = dir.path().join("restored.db");
        let other = [3; 32];
        restore_copy(&keep, &KEY, &dest, &other).unwrap();
        let r = Store::open(&dest, &other).unwrap();
        assert_eq!(
            r.setting::<String>("marker").unwrap().as_deref(),
            Some("Zebracorn"),
            "the old value is back"
        );
        assert!(Store::open(&dest, &KEY).is_err());

        // A key that does not open the backup leaves nothing behind.
        let bad = dir.path().join("bad.db");
        assert!(restore_copy(&keep, &[9; 32], &bad, &KEY).is_err());
        assert!(!bad.exists());
        // Nor does an export over an existing file.
        assert!(matches!(s.export(&keep, &KEY), Err(Error::Exists(_))));
    }

    #[test]
    fn pre_update_backups_are_kept_apart() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("samriddhi.db"), &KEY).unwrap();
        s.set_setting("marker", &"Zebracorn").unwrap();
        let backups_dir = dir.path().join("backups");
        s.daily_backup(&backups_dir, &KEY, at(NOW)).unwrap();
        for m in 0..5 {
            s.pre_update_backup(
                &backups_dir,
                &KEY,
                "0.1.0",
                at(NOW) + SignedDuration::from_mins(m),
                3,
            )
            .unwrap();
        }
        let mut names: Vec<String> = std::fs::read_dir(&backups_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names.len(), 4, "{names:?}");
        assert_eq!(names[0], "pre-update-2026-11-04-090200-0.1.0.db");
        assert_eq!(names[2], "pre-update-2026-11-04-090400-0.1.0.db");
        assert!(names[3].starts_with("samriddhi-"));
        // The daily rotation does not see them, nor remove them.
        assert_eq!(backups(&backups_dir).unwrap().len(), 1);
        for d in 1..=20 {
            s.daily_backup(
                &backups_dir,
                &KEY,
                at(NOW) + SignedDuration::from_hours(24 * d),
            )
            .unwrap();
        }
        let pre = |n: &String| n.starts_with(PRE_UPDATE);
        let after: Vec<String> = std::fs::read_dir(&backups_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(after.iter().filter(|n| pre(n)).count(), 3);
        assert_eq!(backups(&backups_dir).unwrap().len(), KEEP_BACKUPS);
        // It is a full, encrypted copy that the restore screen can open.
        let newest = backups_dir.join(&names[2]);
        let sum = inspect(&newest, &KEY, dir.path()).unwrap();
        assert_eq!(sum.counts["sam_settings"], 1);
        assert!(matches!(
            inspect(&newest, &[9; 32], dir.path()),
            Err(Error::WrongKey)
        ));
        // A version that could climb out of the folder is refused.
        for bad in ["../x", "", "0.1.0/x"] {
            assert!(
                s.pre_update_backup(&backups_dir, &KEY, bad, at(NOW), 3)
                    .is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_backup_from_a_newer_app_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("samriddhi.db"), &KEY).unwrap();
        let file = dir.path().join("newer.db");
        s.export(&file, &KEY).unwrap();
        let newer = Store::open(&file, &KEY).unwrap();
        newer
            .conn
            .execute(
                "UPDATE sam_meta SET value = '42' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        drop(newer);
        assert!(matches!(
            inspect(&file, &KEY, dir.path()),
            Err(Error::TooNew(42))
        ));
        let dest = dir.path().join("dest.db");
        assert!(matches!(
            restore_copy(&file, &KEY, &dest, &KEY),
            Err(Error::TooNew(42))
        ));
        assert!(!dest.exists());
    }

    #[test]
    fn dates_are_utc_iso_strings() {
        assert_eq!(iso(at("2026-04-05T23:59:59Z")), "2026-04-05T23:59:59Z");
        assert_eq!(iso(at("2026-04-06T01:30:00+02:00")), "2026-04-05T23:30:00Z");
    }
}
