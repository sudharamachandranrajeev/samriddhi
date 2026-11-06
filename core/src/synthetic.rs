//! Deterministic synthetic data, so the backtest, forecast and
//! direct-indexing views work offline. None of it is market data. Real
//! history may only come in later through an explicit, user-approved feed.

use std::collections::BTreeMap;

use crate::assets::{ALL, AssetClass, Assumptions};

pub const SAMPLE_DATA_LABEL: &str =
    "Synthetic sample data (seeded random, not real market history).";

/// Months of sample returns: 20 years.
pub const SAMPLE_MONTHS: usize = 240;

/// A small seeded generator (SplitMix64). The same seed always gives the
/// same numbers, on every device.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal (Box-Muller).
    pub fn normal(&mut self) -> f64 {
        let u1 = 1.0 - self.next_f64();
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Synthetic monthly returns per class for [`SAMPLE_MONTHS`] months, from
/// the assumptions with one common factor and a built-in 14-month equity
/// and property drawdown from month 100, so rebalancing behaviour shows.
pub fn sample_monthly_returns(a: &Assumptions, seed: u64) -> BTreeMap<AssetClass, Vec<f64>> {
    let mut rnd = Rng::new(seed);
    // Loading of each class on the common "market" factor.
    let load = |c: AssetClass| match c {
        AssetClass::Equity => 0.9,
        AssetClass::Bonds => 0.1,
        AssetClass::Property => 0.7,
        AssetClass::Gold => 0.05,
        AssetClass::Cash => 0.0,
    };
    let mut out: BTreeMap<AssetClass, Vec<f64>> = ALL.iter().map(|c| (*c, Vec::new())).collect();
    for m in 0..SAMPLE_MONTHS {
        let market = rnd.normal();
        let bear = (100..114).contains(&m);
        for c in ALL {
            let l: f64 = load(c);
            let z = l * market + (1.0 - l * l).sqrt() * rnd.normal();
            let shock = match (bear, c) {
                (true, AssetClass::Equity | AssetClass::Property) => -0.03,
                (true, AssetClass::Bonds) => 0.005,
                _ => 0.0,
            };
            let x = a.of(c);
            out.get_mut(&c)
                .expect("every class")
                .push(x.expected_return / 12.0 + x.volatility / 12f64.sqrt() * z + shock);
        }
    }
    out
}

/// Compounds monthly returns into whole-year returns (a part year at the end
/// is left out).
pub fn annualise(monthly: &[f64]) -> Vec<f64> {
    monthly
        .as_chunks::<12>()
        .0
        .iter()
        .map(|y| y.iter().fold(1.0, |a, r| a * (1.0 + r)) - 1.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `test/core/rebalance_test.dart` ("sample data is
    /// deterministic and 240 months").
    #[test]
    fn sample_data_is_deterministic_and_240_months() {
        let a = Assumptions::builtin();
        let data = sample_monthly_returns(&a, 7);
        assert_eq!(data[&AssetClass::Equity].len(), SAMPLE_MONTHS);
        assert_eq!(sample_monthly_returns(&a, 7), data);
        assert_ne!(sample_monthly_returns(&a, 8), data);
    }

    #[test]
    fn annualise_compounds_whole_years() {
        let y = annualise(&[0.01; 25]);
        assert_eq!(y.len(), 2);
        assert!((y[0] - (1.01f64.powi(12) - 1.0)).abs() < 1e-12);
    }
}
