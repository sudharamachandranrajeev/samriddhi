//! The wrong-PIN count and the wait it earns, kept across restarts.
//!
//! Where: a small file in the app data folder, next to the database. The
//! encrypted store cannot hold it, because the store is open but the screen
//! is locked, and the count must be known and updated *before* the PIN is
//! checked. The OS keyring was the other choice; it is missing on some Linux
//! desktops and on phones, so a file works everywhere the same way.
//!
//! Model: the PIN guards the screen, not the data (the data key lives in the
//! keyring or Keystore). Someone who can delete this file can already read the
//! app folder, so deleting it to reset the count is a known limit, not a hole
//! this file could close. What it does stop: closing and reopening the app to
//! get five more free tries. To make that cheap trick fail even when the app is
//! killed mid-try, the raised count is written before the PIN is checked.

use std::io::Write;
use std::path::Path;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

pub const FILE: &str = "pin-lockout.json";
/// Wrong PINs allowed before each wait starts.
pub const FREE_TRIES: u32 = 5;
/// The longest wait. samridhi capped at one hour; Samriddhi keeps 15 minutes
/// because the PIN only guards the screen and the recovery key always works,
/// so a longer cap would mostly punish a forgetful owner.
pub const MAX_WAIT_SECS: i64 = 15 * 60;

/// Wait before the next PIN try after `wrong` wrong ones in a row: none for
/// the first [`FREE_TRIES`], then 30 s doubling up to [`MAX_WAIT_SECS`].
pub fn pin_wait(wrong: u32) -> Option<SignedDuration> {
    let extra = wrong.checked_sub(FREE_TRIES)?;
    Some(SignedDuration::from_secs(
        (30i64 << extra.min(10)).min(MAX_WAIT_SECS),
    ))
}

/// What is kept: the wrong count and when the wait ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockout {
    pub wrong: u32,
    /// Unix seconds.
    pub until: Option<i64>,
}

impl Lockout {
    /// The state after `wrong` wrong PINs, the last one at `now`.
    pub fn after(wrong: u32, now: Timestamp) -> Lockout {
        Lockout {
            wrong,
            until: pin_wait(wrong).map(|d| (now + d).as_second()),
        }
    }

    pub fn until_ts(&self) -> Option<Timestamp> {
        self.until.and_then(|s| Timestamp::from_second(s).ok())
    }

    /// Read the kept state. No file means a clean start. A file that cannot
    /// be read counts as the first wait, so damage is not a free reset. A wait
    /// further off than the schedule allows (the clock was set back) is cut
    /// to the schedule.
    pub fn load(path: &Path, now: Timestamp) -> Lockout {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Lockout::default(),
            Err(_) => return Lockout::after(FREE_TRIES, now),
        };
        let Ok(mut l) = serde_json::from_str::<Lockout>(&text) else {
            return Lockout::after(FREE_TRIES, now);
        };
        if let (Some(until), Some(max)) = (l.until, pin_wait(l.wrong)) {
            l.until = Some(until.min((now + max).as_second()));
        } else {
            l.until = None;
        }
        l
    }

    /// Write it, whole or not at all. Nothing to keep removes the file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if *self == Lockout::default() {
            return clear(path);
        }
        let tmp = path.with_extension("tmp");
        let _ = std::fs::remove_file(&tmp);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(serde_json::to_string(self)?.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    }
}

/// Forget the kept state.
pub fn clear(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Timestamp {
        "2026-10-29T10:00:00Z".parse().unwrap()
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
    fn state_round_trips_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        assert_eq!(Lockout::load(&path, t0()), Lockout::default());
        let l = Lockout::after(6, t0());
        l.save(&path).unwrap();
        assert_eq!(Lockout::load(&path, t0()), l);
        // Saving a clean state removes the file.
        Lockout::default().save(&path).unwrap();
        assert!(!path.exists());
        clear(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        Lockout::after(5, t0()).save(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0);
    }

    #[test]
    fn damage_is_not_a_free_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(&path, "not json").unwrap();
        let l = Lockout::load(&path, t0());
        assert_eq!(l, Lockout::after(FREE_TRIES, t0()));
        assert!(l.until_ts().unwrap() > t0());
    }

    #[test]
    fn a_clock_set_back_cannot_stretch_the_wait() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        // A wait written when the clock said a day later than it does now.
        let later = t0() + SignedDuration::from_hours(24);
        Lockout::after(5, later).save(&path).unwrap();
        let l = Lockout::load(&path, t0());
        assert_eq!(
            l.until,
            Some((t0() + SignedDuration::from_secs(30)).as_second())
        );
    }

    #[test]
    fn a_count_with_no_wait_loads_without_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        Lockout::after(3, t0()).save(&path).unwrap();
        let l = Lockout::load(&path, t0());
        assert_eq!((l.wrong, l.until), (3, None));
    }
}
