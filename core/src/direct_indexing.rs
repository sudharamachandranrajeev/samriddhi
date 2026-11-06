//! Education-only direct-indexing simulator. It runs on a small synthetic
//! universe (seeded random, not any real index or market history) and shows
//! how tax-loss harvesting trades a little tracking difference for realised
//! losses. Nothing here places or suggests a real trade. Wash-sale rules,
//! costs, tax lots, real index weights and the user's own tax rules are not
//! modelled, and the "tax offset" is mostly a deferral.

use serde::Serialize;

use crate::money::Money;
use crate::store::{Error, Result};
use crate::synthetic::Rng;

pub const INDEXING_DATA_LABEL: &str =
    "Synthetic constituents, for illustration only, not real market history.";

const SECTORS: usize = 4;

/// monthly[stock][month] simple returns. Stock i is in sector i % 4.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticUniverse {
    pub monthly: Vec<Vec<f64>>,
}

impl SyntheticUniverse {
    pub fn stocks(&self) -> usize {
        self.monthly.len()
    }

    pub fn months(&self) -> usize {
        self.monthly.first().map_or(0, Vec::len)
    }
}

/// The prototype's defaults: seed 11, 40 stocks, 60 months.
pub fn synthetic_universe(seed: u64, stocks: usize, months: usize) -> SyntheticUniverse {
    let mut rnd = Rng::new(seed);
    let mut out = vec![Vec::with_capacity(months); stocks];
    for _ in 0..months {
        let market = 0.007 + 0.04 * rnd.normal();
        let sector: Vec<f64> = (0..SECTORS).map(|_| 0.025 * rnd.normal()).collect();
        for (i, s) in out.iter_mut().enumerate() {
            s.push(market + sector[i % SECTORS] + 0.06 * rnd.normal());
        }
    }
    SyntheticUniverse { monthly: out }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectIndexResult {
    /// Start value then month-end values, for the plain index and the
    /// harvesting portfolio.
    pub index_values: Vec<Money>,
    pub portfolio_values: Vec<Money>,
    /// Total losses realised by selling a position below cost.
    pub harvested_losses: Money,
    pub harvests: usize,
    /// Harvested losses times the tax rate: what they could offset if the
    /// investor has gains to use them against. Mostly a deferral, since each
    /// swap lowers the new position's cost basis.
    pub tax_offset_value: Money,
    /// Annualised standard deviation of monthly portfolio-minus-index return.
    pub tracking_difference: f64,
    pub label: &'static str,
}

fn money(minor: f64, like: Money) -> Money {
    Money::new(minor.round_ties_even() as i64, like.currency)
}

/// Holds every stock in equal weight (the "index") and, in parallel, a copy
/// that sells any position down `harvest_threshold` or more from its cost
/// and moves the money into a same-sector peer. Prototype defaults:
/// threshold 0.10, tax rate 0.20.
pub fn simulate_direct_indexing(
    u: &SyntheticUniverse,
    initial: Money,
    harvest_threshold: f64,
    tax_rate: f64,
) -> Result<DirectIndexResult> {
    let n = u.stocks();
    if n <= SECTORS || u.months() < 2 || u.monthly.iter().any(|s| s.len() != u.months()) {
        return Err(Error::Invalid(
            "the universe needs more stocks than sectors and two or more months".into(),
        ));
    }
    if initial.minor <= 0 || !(0.0..=1.0).contains(&tax_rate) {
        return Err(Error::Invalid(
            "a starting amount and a tax rate from 0 to 1 are needed".into(),
        ));
    }
    let start = initial.minor as f64;
    let each = start / n as f64;
    let mut index = vec![each; n];
    let mut value = vec![each; n];
    let mut basis = vec![each; n];
    let (mut idx, mut port) = (vec![start], vec![start]);
    let mut diffs = Vec::with_capacity(u.months());
    let mut losses = 0.0;
    let mut harvests = 0;
    for m in 0..u.months() {
        for i in 0..n {
            index[i] *= 1.0 + u.monthly[i][m];
            value[i] *= 1.0 + u.monthly[i][m];
        }
        for i in 0..n {
            if value[i] > 0.0 && value[i] <= basis[i] * (1.0 - harvest_threshold) {
                losses += basis[i] - value[i];
                harvests += 1;
                let peer = (i + SECTORS) % n;
                value[peer] += value[i];
                basis[peer] += value[i];
                value[i] = 0.0;
                basis[i] = 0.0;
            }
        }
        let a: f64 = index.iter().sum();
        let b: f64 = value.iter().sum();
        diffs.push(b / port[port.len() - 1] - a / idx[idx.len() - 1]);
        idx.push(a);
        port.push(b);
    }
    let mean = diffs.iter().sum::<f64>() / diffs.len() as f64;
    let variance =
        diffs.iter().map(|d| (d - mean) * (d - mean)).sum::<f64>() / (diffs.len() - 1) as f64;
    Ok(DirectIndexResult {
        index_values: idx.iter().map(|v| money(*v, initial)).collect(),
        portfolio_values: port.iter().map(|v| money(*v, initial)).collect(),
        harvested_losses: money(losses, initial),
        harvests,
        tax_offset_value: money(losses * tax_rate, initial),
        tracking_difference: variance.sqrt() * 12f64.sqrt(),
        label: INDEXING_DATA_LABEL,
    })
}

/// Ported from the prototype's `test/core/direct_indexing_test.dart`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    fn u() -> SyntheticUniverse {
        synthetic_universe(11, 40, 60)
    }

    fn initial() -> Money {
        Money::new(100_000 * 100, Currency::GBP)
    }

    #[test]
    fn universe_is_deterministic_and_shaped_as_asked() {
        let u = u();
        assert_eq!(u.stocks(), 40);
        assert_eq!(u.months(), 60);
        assert_eq!(
            synthetic_universe(11, 40, 60).monthly[3][5],
            u.monthly[3][5]
        );
    }

    #[test]
    fn a_threshold_nothing_can_reach_harvests_nothing_and_tracks_exactly() {
        let r = simulate_direct_indexing(&u(), initial(), 0.99, 0.20).unwrap();
        assert_eq!(r.harvests, 0);
        assert_eq!(r.harvested_losses.minor, 0);
        assert!(r.tracking_difference.abs() < 1e-9);
        assert_eq!(r.portfolio_values.last(), r.index_values.last());
    }

    #[test]
    fn harvesting_realises_positive_losses_and_costs_some_tracking() {
        let r = simulate_direct_indexing(&u(), initial(), 0.10, 0.20).unwrap();
        assert!(r.harvests > 0);
        assert!(r.harvested_losses.minor > 0);
        // Each is rounded to the penny once, so they agree within one penny.
        assert!(
            (r.tax_offset_value.minor as f64 - r.harvested_losses.minor as f64 * 0.20).abs() <= 1.0
        );
        assert!(r.tracking_difference > 0.0);
        assert_eq!(r.portfolio_values.len(), r.index_values.len());
        assert_eq!(r.label, INDEXING_DATA_LABEL);
    }

    #[test]
    fn a_universe_too_small_is_refused() {
        let tiny = synthetic_universe(1, 4, 12);
        assert!(simulate_direct_indexing(&tiny, initial(), 0.1, 0.2).is_err());
        assert!(simulate_direct_indexing(&u(), initial(), 0.1, 1.5).is_err());
    }
}
