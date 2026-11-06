//! Drift bands and cash-flow rebalancing. Weights are fractions (f64);
//! every amount in or out is exact [`Money`], split so the parts add up to
//! the whole to the last minor unit. These are suggestions to the user,
//! never orders.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::assets::{ALL, AssetClass, ClassWeights};
use crate::money::{Currency, Money, MoneyError};
use crate::store::Result;

/// Drift tolerance bands, in absolute portfolio-weight fractions.
///
/// Defaults follow published Vanguard research: a 3% band for a typical
/// 60/40 portfolio, and a 200/175 bp asymmetric band for glide-path funds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bands {
    /// Band for a class running above target (current - target > over).
    pub over: f64,
    /// Band for a class running below target (target - current > under).
    pub under: f64,
}

impl Bands {
    pub const STANDARD: Bands = Bands {
        over: 0.03,
        under: 0.03,
    };
    pub const GLIDE_PATH: Bands = Bands {
        over: 0.02,
        under: 0.0175,
    };
}

impl Default for Bands {
    fn default() -> Self {
        Bands::STANDARD
    }
}

/// One class's position against its target.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftRow {
    pub asset_class: AssetClass,
    pub target: f64,
    pub current: f64,
}

impl DriftRow {
    pub fn drift(&self) -> f64 {
        self.current - self.target
    }
}

fn w(m: &ClassWeights, c: AssetClass) -> f64 {
    m.get(&c).copied().unwrap_or(0.0)
}

pub fn drift_rows(target: &ClassWeights, current: &ClassWeights) -> Vec<DriftRow> {
    ALL.iter()
        .map(|c| DriftRow {
            asset_class: *c,
            target: w(target, *c),
            current: w(current, *c),
        })
        .collect()
}

/// True if any class sits outside its band.
pub fn breaches_bands(target: &ClassWeights, current: &ClassWeights, bands: Bands) -> bool {
    drift_rows(target, current)
        .iter()
        .any(|r| r.drift() > bands.over + 1e-12 || -r.drift() > bands.under + 1e-12)
}

/// Where a cash flow goes, per class (positive = buy, negative =
/// sell/withdraw), and how the portfolio looks after.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebalancePlan {
    pub flows: BTreeMap<AssetClass, Money>,
    pub after_weights: ClassWeights,
    /// Whether bands are still breached after the flows.
    pub still_breached: bool,
}

fn minor(values: &BTreeMap<AssetClass, Money>, c: AssetClass) -> f64 {
    values.get(&c).map_or(0.0, |m| m.minor as f64)
}

fn pos(x: f64) -> f64 {
    if x < 0.0 { 0.0 } else { x }
}

/// Check every amount is in `currency`; give the total in minor units.
fn total_in(values: &BTreeMap<AssetClass, Money>, currency: Currency) -> Result<i64> {
    let all: Vec<Money> = values.values().copied().collect();
    if let Some(m) = all.iter().find(|m| m.currency != currency) {
        return Err(MoneyError::Mismatch(currency, m.currency).into());
    }
    Ok(Money::sum(currency, &all)?.minor)
}

/// Round `raw` minor-unit amounts to whole units that add up to exactly
/// `total`, by largest remainder. Zeros stay zero, so a contribution never
/// becomes a sale and a withdrawal never becomes a purchase.
pub(crate) fn apportion(raw: &[f64], total: i64) -> Vec<i64> {
    let mut out: Vec<i64> = raw.iter().map(|r| r.floor() as i64).collect();
    let mut left = total - out.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..raw.len()).filter(|i| raw[*i] != 0.0).collect();
    if order.is_empty() {
        return out;
    }
    // Largest fractional part first when adding, smallest first when taking.
    order.sort_by(|a, b| {
        let fa = raw[*a] - raw[*a].floor();
        let fb = raw[*b] - raw[*b].floor();
        fb.total_cmp(&fa).then(a.cmp(b))
    });
    let mut k = 0;
    while left > 0 {
        out[order[k % order.len()]] += 1;
        left -= 1;
        k += 1;
    }
    let mut k = 0;
    while left < 0 {
        out[order[order.len() - 1 - (k % order.len())]] -= 1;
        left += 1;
        k += 1;
    }
    out
}

fn weights_of(after: &[f64]) -> ClassWeights {
    let t: f64 = after.iter().sum();
    ALL.iter()
        .map(|c| (*c, if t == 0.0 { 0.0 } else { after[c.index()] / t }))
        .collect()
}

/// Cash-flow rebalancing: a contribution (+) buys the most underweight
/// classes first; a withdrawal (-) is drawn from the most overweight classes
/// first, and never more than is held. It never sells for a contribution or
/// buys for a withdrawal, so it avoids realising gains where it can.
///
/// `values` are what each class holds now, all in the cash flow's currency;
/// `target` sums to one.
pub fn cash_flow_rebalance(
    values: &BTreeMap<AssetClass, Money>,
    target: &ClassWeights,
    cash_flow: Money,
    bands: Bands,
) -> Result<RebalancePlan> {
    let cur = cash_flow.currency;
    let total_minor = total_in(values, cur)?;
    let total = total_minor as f64;
    let cf = cash_flow.minor as f64;
    let mut raw = [0.0; 5];
    let mut exact = 0;
    if cash_flow.minor > 0 {
        exact = cash_flow.minor;
        let new_total = total + cf;
        let short: Vec<f64> = ALL
            .iter()
            .map(|c| pos(w(target, *c) * new_total - minor(values, *c)))
            .collect();
        let short_sum: f64 = short.iter().sum();
        for c in ALL {
            let i = c.index();
            raw[i] = if short_sum >= cf {
                if short_sum == 0.0 {
                    0.0
                } else {
                    cf * short[i] / short_sum
                }
            } else {
                // Fill every shortfall, spread the rest by target weight.
                short[i] + (cf - short_sum) * w(target, c)
            };
        }
    } else if cash_flow.minor < 0 {
        // Cannot withdraw more than is held.
        let wd_minor = (-cash_flow.minor).min(total_minor);
        exact = -wd_minor;
        let wd = wd_minor as f64;
        let new_total = total - wd;
        let excess: Vec<f64> = ALL
            .iter()
            .map(|c| pos(minor(values, *c) - w(target, *c) * new_total))
            .collect();
        let excess_sum: f64 = excess.iter().sum();
        for c in ALL {
            let i = c.index();
            raw[i] = if excess_sum >= wd {
                if excess_sum == 0.0 {
                    0.0
                } else {
                    -wd * excess[i] / excess_sum
                }
            } else {
                let rest = wd - excess_sum;
                -excess[i]
                    - if total == 0.0 {
                        0.0
                    } else {
                        rest * minor(values, c) / total
                    }
            };
        }
    }
    let parts = apportion(&raw, exact);
    let flows: BTreeMap<AssetClass, Money> = ALL
        .iter()
        .map(|c| (*c, Money::new(parts[c.index()], cur)))
        .collect();
    let after: Vec<f64> = ALL
        .iter()
        .map(|c| minor(values, *c) + parts[c.index()] as f64)
        .collect();
    let after_weights = weights_of(&after);
    Ok(RebalancePlan {
        still_breached: breaches_bands(target, &after_weights, bands),
        flows,
        after_weights,
    })
}

/// Sell-based fallback: trades that bring every class to target, netting to
/// exactly zero. Only suggested when cash-flow rebalancing leaves bands
/// breached. A suggestion to the user, never an order.
pub fn full_rebalance_trades(
    values: &BTreeMap<AssetClass, Money>,
    target: &ClassWeights,
    currency: Currency,
) -> Result<BTreeMap<AssetClass, Money>> {
    let total = total_in(values, currency)? as f64;
    let raw: Vec<f64> = ALL
        .iter()
        .map(|c| w(target, *c) * total - minor(values, *c))
        .collect();
    let parts = apportion(&raw, 0);
    Ok(ALL
        .iter()
        .map(|c| (*c, Money::new(parts[c.index()], currency)))
        .collect())
}

/// Ported from the prototype's `test/core/rebalance_test.dart` ("bands" and
/// "cash-flow rebalancing" groups). Amounts there were doubles in pounds;
/// here they are exact GBP and the checks are exact too.
#[cfg(test)]
mod tests {
    use super::*;

    fn mix(eq: f64, bonds: f64) -> ClassWeights {
        [
            (AssetClass::Equity, eq),
            (AssetClass::Bonds, bonds),
            (AssetClass::Property, 0.0),
            (AssetClass::Gold, 0.0),
            (AssetClass::Cash, 0.0),
        ]
        .into_iter()
        .collect()
    }

    fn pounds(p: i64) -> Money {
        Money::new(p * 100, Currency::GBP)
    }

    /// Pounds of equity and bonds.
    fn held(eq: i64, bonds: i64) -> BTreeMap<AssetClass, Money> {
        ALL.iter()
            .map(|c| {
                let v = match c {
                    AssetClass::Equity => eq,
                    AssetClass::Bonds => bonds,
                    _ => 0,
                };
                (*c, pounds(v))
            })
            .collect()
    }

    fn sum(flows: &BTreeMap<AssetClass, Money>) -> Money {
        Money::sum(Currency::GBP, &flows.values().copied().collect::<Vec<_>>()).unwrap()
    }

    fn target() -> ClassWeights {
        mix(0.6, 0.4)
    }

    #[test]
    fn three_percent_band_629_inside_631_outside() {
        assert!(!breaches_bands(
            &target(),
            &mix(0.629, 0.371),
            Bands::STANDARD
        ));
        assert!(breaches_bands(
            &target(),
            &mix(0.631, 0.369),
            Bands::STANDARD
        ));
    }

    #[test]
    fn glide_path_band_is_asymmetric() {
        let b = Bands::GLIDE_PATH;
        // Equity +1.9% over (inside 2.0), bonds -1.9% under (outside 1.75).
        assert!(breaches_bands(&target(), &mix(0.619, 0.381), b));
        let t: ClassWeights = [(AssetClass::Equity, 0.6), (AssetClass::Cash, 0.4)]
            .into_iter()
            .collect();
        let c: ClassWeights = [(AssetClass::Equity, 0.619), (AssetClass::Cash, 0.4)]
            .into_iter()
            .collect();
        assert!(!breaches_bands(&t, &c, b));
    }

    #[test]
    fn contribution_buys_underweight_first_no_sells() {
        // 100k: equity 70k (over), bonds 30k (under). Contribute 20k.
        let r = cash_flow_rebalance(
            &held(70_000, 30_000),
            &target(),
            pounds(20_000),
            Bands::STANDARD,
        )
        .unwrap();
        assert_eq!(r.flows[&AssetClass::Equity], pounds(2_000));
        assert_eq!(r.flows[&AssetClass::Bonds], pounds(18_000));
        assert_eq!(sum(&r.flows), pounds(20_000));
        assert!(r.flows.values().all(|f| !f.is_negative()));
        // After: 72k/48k of 120k, exactly on target.
        assert!(!r.still_breached);
    }

    #[test]
    fn small_contribution_is_split_in_proportion_to_shortfall() {
        let r = cash_flow_rebalance(
            &held(70_000, 30_000),
            &target(),
            pounds(5_000),
            Bands::STANDARD,
        )
        .unwrap();
        assert_eq!(r.flows[&AssetClass::Bonds], pounds(5_000));
        assert!(r.still_breached, "70/105 = 66.7% equity");
    }

    #[test]
    fn surplus_beyond_all_shortfalls_is_spread_by_target_weight() {
        let r = cash_flow_rebalance(
            &held(60_000, 40_000),
            &target(),
            pounds(10_000),
            Bands::STANDARD,
        )
        .unwrap();
        assert_eq!(r.flows[&AssetClass::Equity], pounds(6_000));
        assert_eq!(r.flows[&AssetClass::Bonds], pounds(4_000));
    }

    #[test]
    fn withdrawal_is_drawn_from_the_overweight_class_first() {
        let r = cash_flow_rebalance(
            &held(70_000, 30_000),
            &target(),
            pounds(-10_000),
            Bands::STANDARD,
        )
        .unwrap();
        assert_eq!(r.flows[&AssetClass::Equity], pounds(-10_000));
        assert_eq!(r.flows[&AssetClass::Bonds], pounds(0));
        assert!(r.flows.values().all(|f| f.minor <= 0));
    }

    #[test]
    fn withdrawal_beyond_all_excess_falls_back_pro_rata() {
        let r = cash_flow_rebalance(
            &held(60_000, 40_000),
            &target(),
            pounds(-10_000),
            Bands::STANDARD,
        )
        .unwrap();
        assert_eq!(r.flows[&AssetClass::Equity], pounds(-6_000));
        assert_eq!(r.flows[&AssetClass::Bonds], pounds(-4_000));
    }

    #[test]
    fn withdrawal_is_capped_at_the_portfolio_value() {
        let r = cash_flow_rebalance(&held(500, 500), &target(), pounds(-5_000), Bands::STANDARD)
            .unwrap();
        assert_eq!(sum(&r.flows), pounds(-1_000));
    }

    #[test]
    fn full_rebalance_trades_net_to_zero_and_land_on_target() {
        let t = full_rebalance_trades(&held(70_000, 30_000), &target(), Currency::GBP).unwrap();
        assert_eq!(sum(&t), pounds(0));
        assert_eq!(t[&AssetClass::Equity], pounds(-10_000));
        assert_eq!(t[&AssetClass::Bonds], pounds(10_000));
    }

    // --- Samriddhi-specific: exact money ---

    #[test]
    fn odd_amounts_split_to_the_penny_and_currencies_do_not_mix() {
        let thirds: ClassWeights = [
            (AssetClass::Equity, 1.0 / 3.0),
            (AssetClass::Bonds, 1.0 / 3.0),
            (AssetClass::Cash, 1.0 / 3.0),
        ]
        .into_iter()
        .collect();
        let empty = held(0, 0);
        let r = cash_flow_rebalance(
            &empty,
            &thirds,
            Money::new(100, Currency::GBP),
            Bands::STANDARD,
        )
        .unwrap();
        assert_eq!(sum(&r.flows), Money::new(100, Currency::GBP));
        assert!(
            thirds
                .keys()
                .all(|c| r.flows[c].minor == 33 || r.flows[c].minor == 34)
        );
        assert_eq!(r.flows[&AssetClass::Gold].minor, 0, "no target, no buy");
        assert!(
            cash_flow_rebalance(
                &empty,
                &thirds,
                Money::new(1, Currency::INR),
                Bands::STANDARD
            )
            .is_err()
        );
        assert_eq!(apportion(&[0.5, 0.5, 0.0], 1), [1, 0, 0]);
        assert_eq!(apportion(&[-0.5, -0.5, 0.0], -1), [0, -1, 0]);
    }
}
