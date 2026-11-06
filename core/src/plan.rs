//! The plan engine: a target allocation per goal from the user's risk level
//! and the goal's horizon (a glide path), blended into one overall target.
//!
//! The risk input is the combined level (1-5) from the risk questions,
//! mapped to a score of `(level - 1) * 25` (0 to 100). Growth assets are
//! capped by horizon whatever the score: the short-horizon safety cap. An
//! optional tier mode (1-7) gives every goal the same portfolio instead.
//! Everything here is a model on illustrative assumptions; screens carry
//! [`crate::assets::MODEL_DISCLOSURE`].

use jiff::civil::Date;
use serde::Serialize;

use crate::assets::{ALL, AssetClass, Assumptions, ClassWeights, GROWTH};
use crate::money::Money;
use crate::mvo::{self, Options};
use crate::profile::{Goal, GoalKind, Profile};

/// No single growth class above this share.
pub const MAX_SINGLE_CLASS: f64 = 0.70;
/// Risk tiers run 1 (most cautious) to 7 (most adventurous).
pub const RISK_TIER_COUNT: i32 = 7;
/// Days in an average year, for horizons from dates.
const YEAR_DAYS: f64 = 365.25;

/// Max growth exposure (equity + property + gold) allowed by horizon:
/// capacity for loss, independent of stated tolerance.
pub fn max_growth_share(horizon_years: f64) -> f64 {
    if horizon_years <= 2.0 {
        0.10
    } else if horizon_years <= 5.0 {
        0.40
    } else if horizon_years <= 10.0 {
        0.70
    } else {
        0.90
    }
}

/// Maps a 0..100 risk score to MVO risk aversion (lambda): 0 gives 30, 100
/// gives 1.5, geometric in between.
pub fn risk_aversion_for(score: f64) -> f64 {
    let t = score.clamp(0.0, 100.0) / 100.0;
    30.0 * (1.5_f64 / 30.0).powf(t)
}

/// The score the plan engine uses for a combined risk level 1-5.
pub fn risk_score_for_level(level: u8) -> f64 {
    f64::from(level.clamp(1, 5) - 1) * 25.0
}

/// The score a tier stands for; tiers are evenly spaced and clamped.
pub fn tier_risk_score(tier: i32) -> f64 {
    let t = tier.clamp(1, RISK_TIER_COUNT);
    100.0 * f64::from(t - 1) / f64::from(RISK_TIER_COUNT - 1)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetAllocation {
    /// Every class, summing to one.
    pub weights: ClassWeights,
    pub expected_return: f64,
    pub volatility: f64,
    pub risk_score: f64,
    pub horizon_years: f64,
}

impl TargetAllocation {
    pub fn growth_share(&self) -> f64 {
        GROWTH.iter().map(|c| self.weights[c]).sum()
    }
}

/// The target allocation for one goal from a risk score and its horizon.
/// Growth classes together stay within [`max_growth_share`]; no single
/// growth class goes above [`MAX_SINGLE_CLASS`]; within two years there is
/// no property at all.
pub fn target_allocation(a: &Assumptions, risk_score: f64, horizon_years: f64) -> TargetAllocation {
    let mu = a.mu();
    let cov = mvo::covariance_from(&a.vols(), &a.correlations);
    let cap = max_growth_share(horizon_years);
    let mut hi: Vec<f64> = ALL
        .iter()
        .map(|c| {
            if GROWTH.contains(c) {
                cap.min(MAX_SINGLE_CLASS)
            } else {
                1.0
            }
        })
        .collect();
    if cap <= 0.10 {
        hi[AssetClass::Property.index()] = 0.0;
    }
    let lambda = risk_aversion_for(risk_score);
    // Bonds and cash are never capped, so the bounds can always sum to one.
    let run = |hi: &[f64]| {
        let opts = Options {
            upper: Some(hi),
            ..Options::default()
        };
        mvo::optimise(&mu, &cov, lambda, &opts).expect("bonds and cash keep the bounds feasible")
    };
    let mut res = run(&hi);
    // Enforce the aggregate growth cap by tightening per-class bounds.
    for _ in 0..30 {
        let growth: f64 = GROWTH.iter().map(|c| res.weights[c.index()]).sum();
        if growth <= cap + 1e-6 {
            break;
        }
        let k = cap / growth;
        for c in GROWTH {
            hi[c.index()] = hi[c.index()].min(res.weights[c.index()] * k);
        }
        res = run(&hi);
    }
    TargetAllocation {
        weights: ALL.iter().map(|c| (*c, res.weights[c.index()])).collect(),
        expected_return: res.expected_return,
        volatility: res.volatility,
        risk_score,
        horizon_years,
    }
}

/// One goal as the plan engine sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalInput {
    pub name: String,
    /// What the goal needs; weights the blend when every goal has one in one
    /// currency.
    pub target: Option<Money>,
    pub horizon_years: f64,
    /// True when the goal had no date and the horizon is the default for its
    /// kind ([`default_horizon_years`]); screens should say so.
    pub horizon_assumed: bool,
}

impl GoalInput {
    pub fn new(name: &str, target: Option<Money>, horizon_years: f64) -> GoalInput {
        GoalInput {
            name: name.into(),
            target,
            horizon_years,
            horizon_assumed: false,
        }
    }
}

/// The horizon assumed for a goal with no date. Goals that are about safety
/// get short ones, so a missing date never adds risk.
pub fn default_horizon_years(kind: GoalKind) -> f64 {
    match kind {
        GoalKind::EmergencyCushion => 0.0,
        GoalKind::ClearDebt => 1.0,
        GoalKind::FamilySupportAbroad => 3.0,
        GoalKind::BuyHome => 5.0,
        GoalKind::ExtraIncome | GoalKind::LearnInvesting => 10.0,
        GoalKind::RetireEarly => 20.0,
    }
}

/// Years from `today` to `date`; zero once it has passed.
pub fn years_until(today: Date, date: Date) -> f64 {
    let days = today.until(date).map_or(0, |s| s.get_days());
    (f64::from(days) / YEAR_DAYS).max(0.0)
}

/// The profile's goals as plan inputs, horizons counted from `today`.
pub fn goal_inputs(goals: &[Goal], today: Date) -> Vec<GoalInput> {
    goals
        .iter()
        .map(|g| GoalInput {
            name: g.kind.as_str().into(),
            target: g.target,
            horizon_years: g
                .target_date
                .map_or(default_horizon_years(g.kind), |d| years_until(today, d)),
            horizon_assumed: g.target_date.is_none(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalPlan {
    pub goal: GoalInput,
    pub allocation: TargetAllocation,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanResult {
    pub per_goal: Vec<GoalPlan>,
    /// The per-goal targets blended into one (weights sum to one).
    pub blended: ClassWeights,
    /// The tier used, if tier mode was on.
    pub tier: Option<i32>,
    pub assumptions_as_of: Date,
    pub assumptions_source: String,
}

/// Per-goal targets and the blended overall target. With no goals, one
/// 20-year "General investing" goal is assumed.
///
/// The blend weights each goal by its target amount when every goal has one
/// in a single currency (and they add up to more than zero); otherwise every
/// goal counts the same.
///
/// In tier mode every goal gets the same portfolio, whatever its horizon:
/// the short-horizon cap is deliberately not applied, and screens must say
/// so.
pub fn build_plan(
    a: &Assumptions,
    risk_score: f64,
    goals: &[GoalInput],
    tier: Option<i32>,
) -> PlanResult {
    let fallback = [GoalInput::new("General investing", None, 20.0)];
    let inputs = if goals.is_empty() {
        &fallback[..]
    } else {
        goals
    };
    let per_goal: Vec<GoalPlan> = inputs
        .iter()
        .map(|g| GoalPlan {
            goal: g.clone(),
            allocation: match tier {
                None => target_allocation(a, risk_score, g.horizon_years),
                // A long horizon here only means "no glide-path cap".
                Some(t) => target_allocation(a, tier_risk_score(t), 30.0),
            },
        })
        .collect();
    let shares = blend_shares(inputs);
    let mut blended: ClassWeights = ALL.iter().map(|c| (*c, 0.0)).collect();
    for (p, share) in per_goal.iter().zip(shares) {
        for c in ALL {
            *blended.get_mut(&c).expect("every class") += share * p.allocation.weights[&c];
        }
    }
    PlanResult {
        per_goal,
        blended,
        tier: tier.map(|t| t.clamp(1, RISK_TIER_COUNT)),
        assumptions_as_of: a.as_of,
        assumptions_source: a.source.clone(),
    }
}

fn blend_shares(goals: &[GoalInput]) -> Vec<f64> {
    let even = vec![1.0 / goals.len() as f64; goals.len()];
    let Some(first) = goals.first().and_then(|g| g.target) else {
        return even;
    };
    let amounts: Option<Vec<i64>> = goals
        .iter()
        .map(|g| {
            g.target
                .filter(|m| m.currency == first.currency)
                .map(|m| m.minor)
        })
        .collect();
    match amounts {
        Some(v) => {
            let total: i128 = v.iter().map(|m| *m as i128).sum();
            if total <= 0 {
                even
            } else {
                v.iter().map(|m| *m as f64 / total as f64).collect()
            }
        }
        None => even,
    }
}

/// The plan for a stored profile. Needs the risk questions answered, unless
/// a tier is chosen. Horizons are counted from `today`.
pub fn plan_for_profile(
    a: &Assumptions,
    p: &Profile,
    today: Date,
    tier: Option<i32>,
) -> Option<PlanResult> {
    let score = match (&p.risk, tier) {
        (Some(r), _) => risk_score_for_level(r.combined),
        (None, Some(_)) => 0.0,
        (None, None) => return None,
    };
    Some(build_plan(a, score, &goal_inputs(&p.goals, today), tier))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    fn a() -> Assumptions {
        Assumptions::builtin()
    }

    fn gbp(major: i64) -> Option<Money> {
        Some(Money::new(major * 100, Currency::GBP))
    }

    fn sum(w: &ClassWeights) -> f64 {
        w.values().sum()
    }

    // --- Ported from test/core/engine_test.dart, "target allocation" ---

    #[test]
    fn weights_sum_to_one_for_any_score_and_horizon() {
        for s in [0.0, 30.0, 60.0, 100.0] {
            for h in [1.0, 4.0, 8.0, 20.0] {
                let t = target_allocation(&a(), s, h);
                assert!((sum(&t.weights) - 1.0).abs() < 1e-6);
                assert!(t.growth_share() <= max_growth_share(h) + 1e-4);
            }
        }
    }

    #[test]
    fn more_risk_appetite_gives_more_growth_on_a_long_horizon() {
        let lo = target_allocation(&a(), 10.0, 25.0);
        let hi = target_allocation(&a(), 90.0, 25.0);
        assert!(hi.growth_share() > lo.growth_share());
        assert!(hi.volatility > lo.volatility);
    }

    #[test]
    fn short_horizon_caps_growth_even_for_adventurous_investors() {
        let t = target_allocation(&a(), 100.0, 1.0);
        assert!(t.growth_share() <= 0.10 + 1e-4);
        assert!(t.weights[&AssetClass::Property].abs() < 1e-9);
    }

    #[test]
    fn no_single_growth_class_above_70_percent() {
        let t = target_allocation(&a(), 100.0, 30.0);
        assert!(t.weights[&AssetClass::Equity] <= 0.70 + 1e-6);
    }

    // --- Ported from test/core/plan_test.dart ---

    #[test]
    fn blended_plan_sums_to_one_and_weights_goals_by_amount() {
        let p = build_plan(
            &a(),
            70.0,
            &[
                GoalInput::new("Far", gbp(900), 25.0),
                GoalInput::new("Near", gbp(100), 1.0),
            ],
            None,
        );
        assert!((sum(&p.blended) - 1.0).abs() < 1e-6);
        assert_eq!(p.per_goal.len(), 2);
        // Dominated by the long-horizon goal, so meaningful equity.
        assert!(p.blended[&AssetClass::Equity] > 0.3);
    }

    #[test]
    fn no_goals_falls_back_to_a_default_long_horizon_goal() {
        let p = build_plan(&a(), 50.0, &[], None);
        assert_eq!(p.per_goal.len(), 1);
        assert_eq!(p.per_goal[0].goal.horizon_years, 20.0);
    }

    #[test]
    fn tier_scores_are_evenly_spaced_and_clamped() {
        assert_eq!(tier_risk_score(1), 0.0);
        assert_eq!(tier_risk_score(7), 100.0);
        assert_eq!(tier_risk_score(4), 50.0);
        assert_eq!(tier_risk_score(0), 0.0);
        assert_eq!(tier_risk_score(99), 100.0);
    }

    #[test]
    fn tier_mode_gives_every_goal_the_same_portfolio_and_higher_is_more_equity() {
        let goals = [
            GoalInput::new("a", gbp(100), 2.0),
            GoalInput::new("b", gbp(100), 30.0),
        ];
        let p = build_plan(&a(), 10.0, &goals, Some(6));
        assert_eq!(
            p.per_goal[0].allocation.weights,
            p.per_goal[1].allocation.weights
        );
        let lo = build_plan(&a(), 10.0, &goals, Some(1));
        let hi = build_plan(&a(), 10.0, &goals, Some(7));
        assert!(hi.blended[&AssetClass::Equity] > lo.blended[&AssetClass::Equity]);
        assert!((sum(&p.blended) - 1.0).abs() < 1e-9);
    }

    // --- Samriddhi-specific: the risk mapping and goal horizons ---

    #[test]
    fn risk_levels_map_to_scores_in_steps_of_25() {
        let got: Vec<f64> = (0..=6).map(risk_score_for_level).collect();
        assert_eq!(got, [0.0, 0.0, 25.0, 50.0, 75.0, 100.0, 100.0]);
    }

    #[test]
    fn horizons_come_from_goal_dates_with_safe_defaults() {
        let today = Date::constant(2026, 11, 6);
        let goals = [
            Goal {
                kind: GoalKind::BuyHome,
                target: gbp(30_000),
                target_date: Some(Date::constant(2036, 11, 6)),
            },
            Goal {
                kind: GoalKind::EmergencyCushion,
                target: None,
                target_date: None,
            },
            Goal {
                kind: GoalKind::RetireEarly,
                target: None,
                target_date: Some(Date::constant(2020, 1, 1)),
            },
        ];
        let g = goal_inputs(&goals, today);
        assert!((g[0].horizon_years - 10.0).abs() < 0.01);
        assert!(!g[0].horizon_assumed);
        assert_eq!(g[1].horizon_years, 0.0);
        assert!(g[1].horizon_assumed);
        assert_eq!(g[2].horizon_years, 0.0, "a passed date is now");
        // The cushion gets the safety cap; mixed targets blend evenly.
        let p = build_plan(&a(), 100.0, &g, None);
        assert!(p.per_goal[1].allocation.growth_share() <= 0.10 + 1e-4);
        assert!((sum(&p.blended) - 1.0).abs() < 1e-6);
        assert_eq!(p.assumptions_as_of, a().as_of);
    }

    #[test]
    fn blend_weights_by_amount_only_in_one_currency() {
        let inr = Some(Money::new(100, Currency::INR));
        assert_eq!(
            blend_shares(&[
                GoalInput::new("a", gbp(3), 1.0),
                GoalInput::new("b", gbp(1), 1.0)
            ]),
            [0.75, 0.25]
        );
        assert_eq!(
            blend_shares(&[
                GoalInput::new("a", gbp(3), 1.0),
                GoalInput::new("b", inr, 1.0)
            ]),
            [0.5, 0.5]
        );
        assert_eq!(
            blend_shares(&[
                GoalInput::new("a", gbp(3), 1.0),
                GoalInput::new("b", None, 1.0)
            ]),
            [0.5, 0.5]
        );
    }

    #[test]
    fn a_profile_needs_risk_answers_unless_a_tier_is_chosen() {
        let today = Date::constant(2026, 11, 6);
        let p = Profile::default();
        assert!(plan_for_profile(&a(), &p, today, None).is_none());
        let t = plan_for_profile(&a(), &p, today, Some(4)).unwrap();
        assert_eq!(t.tier, Some(4));
        assert_eq!(t.per_goal[0].allocation.risk_score, 50.0);
    }
}
