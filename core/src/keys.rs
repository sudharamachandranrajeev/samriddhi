//! Keys. The database key is 32 random bytes kept in the device keystore; the
//! user also gets it once as a recovery key. The optional app lock PIN is kept
//! only as an Argon2id hash: it guards the screen, it does not encrypt data.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

pub type Key = [u8; 32];

pub fn new_key() -> Key {
    let mut k = [0u8; 32];
    getrandom::fill(&mut k).expect("the operating system has no random source");
    k
}

/// 64 hex digits in groups of four, e.g. `1f2e-...`, the same form Sachiva uses.
pub fn recovery_text(key: &Key) -> String {
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    hex.as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("-")
}

/// Reads a recovery key as typed: dashes, spaces and capitals are fine.
pub fn parse_recovery(text: &str) -> Option<Key> {
    let hex: Vec<u8> = text
        .bytes()
        .filter(|b| !matches!(b, b'-' | b' ' | b'\n' | b'\t'))
        .collect();
    if hex.len() != 64 {
        return None;
    }
    let mut k = [0u8; 32];
    for (i, pair) in hex.chunks(2).enumerate() {
        k[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(k)
}

pub const PIN_MIN: usize = 4;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("a PIN needs at least {PIN_MIN} digits, and only digits")]
pub struct WeakPin;

/// Hash a new app-lock PIN for storage.
pub fn pin_hash(pin: &str) -> Result<String, WeakPin> {
    if pin.len() < PIN_MIN || !pin.bytes().all(|b| b.is_ascii_digit()) {
        return Err(WeakPin);
    }
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).expect("the operating system has no random source");
    let salt = SaltString::encode_b64(&salt).expect("16 bytes is a valid salt");
    Ok(Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .expect("Argon2 default parameters are valid")
        .to_string())
}

pub fn pin_check(pin: &str, stored: &str) -> bool {
    PasswordHash::new(stored).is_ok_and(|h| {
        Argon2::default()
            .verify_password(pin.as_bytes(), &h)
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_key_round_trips_however_it_is_typed() {
        let k = new_key();
        let text = recovery_text(&k);
        assert_eq!(text.len(), 64 + 15);
        assert_eq!(parse_recovery(&text), Some(k));
        assert_eq!(
            parse_recovery(&text.to_uppercase().replace('-', " ")),
            Some(k)
        );
        assert_eq!(parse_recovery("abcd"), None);
        assert_eq!(parse_recovery(&"zz".repeat(32)), None);
        assert_ne!(new_key(), k);
    }

    #[test]
    fn pin_is_hashed_and_checked() {
        let h = pin_hash("482913").unwrap();
        assert!(!h.contains("482913"));
        assert!(pin_check("482913", &h));
        assert!(!pin_check("482914", &h));
        assert!(!pin_check("482913", "not a hash"));
        assert_eq!(pin_hash("12"), Err(WeakPin));
        assert_eq!(pin_hash("12ab"), Err(WeakPin));
    }
}
