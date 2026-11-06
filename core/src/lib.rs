//! Samriddhi core: exact money, the encrypted store and its keys, the
//! wizard profile, the risk questions, the dated rules catalogue and the
//! next-best-step engine; accounts and holdings with their import; and the
//! planning engines (asset classes with dated assumptions, mean-variance
//! optimiser, per-goal plan, rebalancing, forecast, backtest and the
//! direct-indexing simulator); the deterministic agents that add their
//! steps to the same feed, and the rounded, nameless digest an AI model may
//! narrate. No UI; the Tauri shell and tests drive it.
//!
//! Safety boundary, kept in code: nothing here moves money, places trades,
//! calls a data aggregator or sends anything off the device.

pub mod agents;
pub mod assets;
pub mod backtest;
pub mod catalogue;
pub mod digest;
pub mod direct_indexing;
pub mod export;
pub mod forecast;
pub mod holdings_import;
pub mod keys;
pub mod money;
pub mod mvo;
pub mod plan;
pub mod profile;
pub mod rebalance;
pub mod risk;
pub mod step_log;
pub mod steps;
pub mod store;
pub mod synthetic;
pub mod wealth;

pub use store::{Error, Result, Store};
