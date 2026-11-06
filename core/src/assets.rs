//! The asset classes the planner allocates across, and the dated
//! capital-market assumptions it uses (`core/catalogue/assumptions.json`).
//!
//! The assumptions are generic, synthetic, long-run illustrative figures,
//! not market data and not a forecast. Like the rules catalogue they carry
//! the day they were set (`asOf`) and where they came from (`source`).
//! Every screen that shows a result built on them must carry
//! [`MODEL_DISCLOSURE`].

use std::collections::BTreeMap;

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::store::{Error, Result};

const BUILTIN: &str = include_str!("../catalogue/assumptions.json");

pub const MODEL_DISCLOSURE: &str = "This is a model using illustrative assumptions, not financial advice or \
     a guarantee. Past and simulated results do not predict future returns.";

/// An asset class. The order here is the order of every vector and matrix
/// the engines use ([`AssetClass::index`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetClass {
    Equity,
    Bonds,
    Property,
    Gold,
    Cash,
}

/// Every class, in index order.
pub const ALL: [AssetClass; 5] = [
    AssetClass::Equity,
    AssetClass::Bonds,
    AssetClass::Property,
    AssetClass::Gold,
    AssetClass::Cash,
];

/// The growth classes: what the horizon cap limits.
pub const GROWTH: [AssetClass; 3] = [AssetClass::Equity, AssetClass::Property, AssetClass::Gold];

/// A fraction of the portfolio per class. A missing class counts as zero.
pub type ClassWeights = BTreeMap<AssetClass, f64>;

impl AssetClass {
    pub fn index(&self) -> usize {
        *self as usize
    }

    pub fn label(&self) -> &'static str {
        match self {
            AssetClass::Equity => "Equities",
            AssetClass::Bonds => "Bonds",
            AssetClass::Property => "Property / REITs",
            AssetClass::Gold => "Gold / commodities",
            AssetClass::Cash => "Cash",
        }
    }

    /// The stored name ("equity", "bonds"...).
    pub fn as_str(&self) -> &'static str {
        match self {
            AssetClass::Equity => "equity",
            AssetClass::Bonds => "bonds",
            AssetClass::Property => "property",
            AssetClass::Gold => "gold",
            AssetClass::Cash => "cash",
        }
    }

    /// A class by its stored name only.
    pub fn parse(s: &str) -> Option<AssetClass> {
        ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// A class as a file may name it: the stored name or a common alias, in
    /// any case ("Stocks", "fixed income", "REIT").
    pub fn from_alias(s: &str) -> Option<AssetClass> {
        let k = s.trim().to_lowercase();
        AssetClass::parse(&k).or(match k.as_str() {
            "equities" | "stock" | "stocks" => Some(AssetClass::Equity),
            "bond" | "fixed income" => Some(AssetClass::Bonds),
            "reit" | "reits" | "real estate" => Some(AssetClass::Property),
            "commodity" | "commodities" => Some(AssetClass::Gold),
            _ => None,
        })
    }
}

/// Weights for every class, missing ones as zero.
pub fn full(w: &ClassWeights) -> ClassWeights {
    ALL.iter()
        .map(|c| (*c, w.get(c).copied().unwrap_or(0.0)))
        .collect()
}

/// The sum of a class map's weights.
pub fn total(w: &ClassWeights) -> f64 {
    w.values().sum()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassAssumption {
    pub class: AssetClass,
    /// Annual expected (arithmetic) return, illustrative.
    pub expected_return: f64,
    /// Annual standard deviation, illustrative.
    pub volatility: f64,
}

/// The inputs the optimiser runs on, with the day they were set and where
/// they came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assumptions {
    pub as_of: Date,
    pub source: String,
    pub note: String,
    /// One per class, in [`ALL`] order.
    pub classes: Vec<ClassAssumption>,
    /// Correlations, indexed by [`AssetClass::index`].
    pub correlations: Vec<Vec<f64>>,
}

impl Assumptions {
    pub fn builtin() -> Assumptions {
        Assumptions::parse(BUILTIN).expect("the built-in assumptions are valid")
    }

    /// Read and check a set of assumptions: one entry per class in order,
    /// finite figures, volatility above zero, and a symmetric correlation
    /// matrix with ones on the diagonal and every value in [-1, 1].
    pub fn parse(json: &str) -> Result<Assumptions> {
        let a: Assumptions = serde_json::from_str(json)?;
        let bad = |why: &str| Err(Error::Invalid(format!("assumptions: {why}")));
        if a.source.trim().is_empty() {
            return bad("a source is needed");
        }
        if a.classes.len() != ALL.len() || a.classes.iter().zip(ALL).any(|(x, c)| x.class != c) {
            return bad("one entry per asset class, in order");
        }
        if a.classes.iter().any(|c| {
            !c.expected_return.is_finite() || !c.volatility.is_finite() || c.volatility <= 0.0
        }) {
            return bad("returns and volatilities must be finite, volatility above zero");
        }
        let n = ALL.len();
        if a.correlations.len() != n || a.correlations.iter().any(|r| r.len() != n) {
            return bad("the correlation matrix must be square, one row per class");
        }
        for i in 0..n {
            for j in 0..n {
                let v = a.correlations[i][j];
                if !(-1.0..=1.0).contains(&v) || (v - a.correlations[j][i]).abs() > 1e-12 {
                    return bad("correlations must be symmetric and within -1 to 1");
                }
            }
            if (a.correlations[i][i] - 1.0).abs() > 1e-12 {
                return bad("each class correlates 1 with itself");
            }
        }
        Ok(a)
    }

    /// Expected returns, in index order.
    pub fn mu(&self) -> Vec<f64> {
        self.classes.iter().map(|c| c.expected_return).collect()
    }

    /// Volatilities, in index order.
    pub fn vols(&self) -> Vec<f64> {
        self.classes.iter().map(|c| c.volatility).collect()
    }

    pub fn of(&self, class: AssetClass) -> &ClassAssumption {
        &self.classes[class.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_assumptions_are_dated_sourced_and_valid() {
        let a = Assumptions::builtin();
        assert_eq!(a.as_of, Date::constant(2026, 11, 5));
        assert!(a.source.contains("Synthetic"));
        assert_eq!(a.of(AssetClass::Equity).expected_return, 0.070);
        assert_eq!(a.of(AssetClass::Cash).volatility, 0.005);
        let mut broken = serde_json::to_value(&a).unwrap();
        broken["correlations"][0][1] = serde_json::json!(0.5);
        assert!(
            Assumptions::parse(&broken.to_string()).is_err(),
            "asymmetric"
        );
        let mut broken = serde_json::to_value(&a).unwrap();
        broken["source"] = serde_json::json!(" ");
        assert!(
            Assumptions::parse(&broken.to_string()).is_err(),
            "no source"
        );
    }

    #[test]
    fn classes_parse_by_name_and_alias() {
        for c in ALL {
            assert_eq!(AssetClass::parse(c.as_str()), Some(c));
            assert_eq!(ALL[c.index()], c);
        }
        assert_eq!(AssetClass::from_alias(" Stocks "), Some(AssetClass::Equity));
        assert_eq!(
            AssetClass::from_alias("fixed income"),
            Some(AssetClass::Bonds)
        );
        assert_eq!(AssetClass::from_alias("REIT"), Some(AssetClass::Property));
        assert_eq!(
            AssetClass::from_alias("commodities"),
            Some(AssetClass::Gold)
        );
        assert_eq!(AssetClass::from_alias("crypto"), None);
        assert_eq!(AssetClass::parse("Equity"), None, "stored names are exact");
    }
}
