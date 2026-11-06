//! Samriddhi core: exact money, the encrypted store and its keys, the
//! wizard profile, the risk questions, the dated rules catalogue and the
//! next-best-step engine. No UI; the Tauri shell and tests drive it.
//!
//! Safety boundary, kept in code: nothing here moves money, places trades,
//! calls a data aggregator or sends anything off the device.

pub mod catalogue;
pub mod export;
pub mod keys;
pub mod money;
pub mod profile;
pub mod risk;
pub mod steps;
pub mod store;

pub use store::{Error, Result, Store};
