//! A backtest of a target mix against monthly returns, with three
//! rebalancing rules. Rebalancing is simulated on past (or synthetic) data
//! only; no trade is ever placed, and costs are not modelled. The path is
//! worked in f64 minor units and each month-end value is shown as exact
//! money, rounded half to even.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::assets::{ALL, AssetClass, ClassWeights};
use crate::money::Money;
use crate::rebalance::{Bands, breaches_bands};
use crate::store::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RebalanceRule {
    None,
    Annual,
    Threshold,
}

impl RebalanceRule {
    pub fn label(&self) -> &'static str {
        match self {
            RebalanceRule::None => "Never rebalance",
            RebalanceRule::Annual => "Rebalance every 12 months",
            RebalanceRule::Threshold => "Rebalance when a band is breached",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestResult {
    pub rule: RebalanceRule,
    /// Value at the start and after each month.
    pub values: Vec<Money>,
    pub rebalances: usize,
    pub cagr: f64,
    pub annual_vol: f64,
    /// Largest peak-to-trough fall, as a positive fraction.
    pub max_drawdown: f64,
}

impl BacktestResult {
    pub fn final_value(&self) -> Money {
        *self.values.last().expect("at least the start value")
    }
}

/// Replay `monthly` returns for a portfolio started at `target` weights with
/// `initial` money, applying `rule`. Every class needs the same number of
/// months (two or more); a missing class counts as returning nothing.
pub fn backtest(
    monthly: &BTreeMap<AssetClass, Vec<f64>>,
    target: &ClassWeights,
    rule: RebalanceRule,
    bands: Bands,
    initial: Money,
) -> Result<BacktestResult> {
    let n = monthly.values().next().map_or(0, Vec::len);
    if n < 2 || monthly.values().any(|v| v.len() != n) {
        return Err(Error::Invalid(
            "a backtest needs two or more months for every class".into(),
        ));
    }
    if initial.minor <= 0 {
        return Err(Error::Invalid("a backtest needs a starting amount".into()));
    }
    let ret = |c: AssetClass, m: usize| monthly.get(&c).map_or(0.0, |v| v[m]);
    let t = |c: AssetClass| target.get(&c).copied().unwrap_or(0.0);
    let start = initial.minor as f64;
    let mut held: Vec<f64> = ALL.iter().map(|c| start * t(*c)).collect();
    let mut path = vec![start];
    let mut rets = Vec::with_capacity(n);
    let mut rebalances = 0;
    for m in 0..n {
        let before: f64 = held.iter().sum();
        for c in ALL {
            held[c.index()] *= 1.0 + ret(c, m);
        }
        let total: f64 = held.iter().sum();
        rets.push(total / before - 1.0);
        path.push(total);
        let due = match rule {
            RebalanceRule::None => false,
            RebalanceRule::Annual => (m + 1) % 12 == 0,
            RebalanceRule::Threshold => {
                let now: ClassWeights = ALL.iter().map(|c| (*c, held[c.index()] / total)).collect();
                breaches_bands(target, &now, bands)
            }
        };
        if due {
            held = ALL.iter().map(|c| total * t(*c)).collect();
            rebalances += 1;
        }
    }
    let years = n as f64 / 12.0;
    let mean = rets.iter().sum::<f64>() / rets.len() as f64;
    let variance =
        rets.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / (rets.len() - 1) as f64;
    let (mut peak, mut dd) = (path[0], 0.0_f64);
    for v in &path {
        peak = peak.max(*v);
        dd = dd.max((peak - v) / peak);
    }
    let last = *path.last().expect("start value");
    Ok(BacktestResult {
        rule,
        values: path
            .iter()
            .map(|v| Money::new(v.round_ties_even() as i64, initial.currency))
            .collect(),
        rebalances,
        cagr: (last / start).powf(1.0 / years) - 1.0,
        annual_vol: (variance * 12.0).sqrt(),
        max_drawdown: dd,
    })
}

/// Ported from the prototype's `test/core/rebalance_test.dart` ("sample
/// data + backtest" group).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Assumptions;
    use crate::money::Currency;
    use crate::synthetic::sample_monthly_returns;

    fn target() -> ClassWeights {
        [(AssetClass::Equity, 0.6), (AssetClass::Bonds, 0.4)]
            .into_iter()
            .collect()
    }

    fn pounds(p: i64) -> Money {
        Money::new(p * 100, Currency::GBP)
    }

    #[test]
    fn never_rebalance_drifts_threshold_keeps_drift_near_the_band() {
        let data = sample_monthly_returns(&Assumptions::builtin(), 7);
        let run = |rule| backtest(&data, &target(), rule, Bands::STANDARD, pounds(10_000)).unwrap();
        let none = run(RebalanceRule::None);
        let thr = run(RebalanceRule::Threshold);
        let ann = run(RebalanceRule::Annual);
        assert_eq!(none.rebalances, 0);
        assert_eq!(ann.rebalances, 20);
        assert!(thr.rebalances > 0 && thr.rebalances < 240);
        for r in [&none, &thr, &ann] {
            assert_eq!(r.values.len(), 241);
            assert!((0.0..=1.0).contains(&r.max_drawdown));
            assert!(r.annual_vol > 0.0);
        }
    }

    #[test]
    fn single_asset_class_zero_return_portfolio_is_flat() {
        let flat: BTreeMap<AssetClass, Vec<f64>> =
            ALL.iter().map(|c| (*c, vec![0.0; 24])).collect();
        let r = backtest(
            &flat,
            &target(),
            RebalanceRule::Annual,
            Bands::STANDARD,
            pounds(1_000),
        )
        .unwrap();
        assert_eq!(r.final_value(), pounds(1_000));
        assert!(r.cagr.abs() < 1e-9);
        assert_eq!(r.max_drawdown, 0.0);
    }

    #[test]
    fn too_little_data_or_no_money_is_refused() {
        let one: BTreeMap<AssetClass, Vec<f64>> = ALL.iter().map(|c| (*c, vec![0.0])).collect();
        assert!(
            backtest(
                &one,
                &target(),
                RebalanceRule::None,
                Bands::STANDARD,
                pounds(1)
            )
            .is_err()
        );
        let two: BTreeMap<AssetClass, Vec<f64>> = ALL.iter().map(|c| (*c, vec![0.0; 2])).collect();
        assert!(
            backtest(
                &two,
                &target(),
                RebalanceRule::None,
                Bands::STANDARD,
                pounds(0)
            )
            .is_err()
        );
    }
}
