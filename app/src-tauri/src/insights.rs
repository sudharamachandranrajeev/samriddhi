//! The plan, rebalancing and insights views. Every number comes from the
//! core's engines; these commands only gather what a screen shows, in the
//! base currency, with the labels each result must carry (synthetic data,
//! illustrative, the model disclosure). Nothing here trades or moves money.

use std::collections::BTreeMap;

use jiff::civil::Date;
use samriddhi_core::agents::{FORECAST_SEED, base_currency};
use samriddhi_core::assets::{
    ALL, AssetClass, Assumptions, ClassAssumption, ClassWeights, MODEL_DISCLOSURE,
};
use samriddhi_core::backtest::{RebalanceRule, backtest};
use samriddhi_core::direct_indexing::{simulate_direct_indexing, synthetic_universe};
use samriddhi_core::forecast::{self, Forecast};
use samriddhi_core::money::{Currency, Money};
use samriddhi_core::plan::{PlanResult, RISK_TIER_COUNT, max_growth_share};
use samriddhi_core::rebalance::{
    self, Bands, RebalancePlan, breaches_bands, cash_flow_rebalance, drift_rows,
};
use samriddhi_core::steps;
use samriddhi_core::store::{self, Error, Store};
use samriddhi_core::synthetic::{self, SAMPLE_DATA_LABEL};
use samriddhi_core::wealth::Totals;
use serde::Serialize;
use tauri::State;

use crate::{App, CommandResult, now};

/// The backtest and direct-indexing simulator start from this many whole
/// units of the base currency.
const START_MAJOR: i64 = 10_000;
/// The direct-indexing simulator's universe: the prototype's seed, 40
/// stocks over 60 months, harvesting at 10% below cost, tax at 20%.
const INDEX_SEED: u64 = 11;
const INDEX_STOCKS: usize = 40;
const INDEX_MONTHS: usize = 60;
const HARVEST_AT: f64 = 0.10;
const TAX_RATE: f64 = 0.20;

fn whole(major: i64, c: Currency) -> Money {
    Money::new(major * 10_i64.pow(u32::from(c.exponent())), c)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    /// `None` until the risk questions are answered (or a tier is chosen).
    plan: Option<PlanResult>,
    /// The growth cap each goal's horizon allows, in `plan.per_goal` order.
    /// Tier mode does not apply it; the screen says so.
    caps: Vec<f64>,
    tier: Option<i32>,
    tier_count: i32,
    /// The combined risk level 1-5, when answered.
    risk_level: Option<u8>,
    risk_label: Option<String>,
    /// The illustrative long-run figures the optimiser runs on.
    assumptions: Vec<ClassAssumption>,
    assumptions_note: String,
    disclosure: &'static str,
}

pub fn plan_view_of(s: &Store, today: Date) -> store::Result<PlanView> {
    let p = s.profile()?;
    let a = Assumptions::builtin();
    let plan = s.plan(today)?;
    Ok(PlanView {
        caps: plan.as_ref().map_or(vec![], |p| {
            p.per_goal
                .iter()
                .map(|g| max_growth_share(g.goal.horizon_years))
                .collect()
        }),
        tier: s.plan_tier()?,
        tier_count: RISK_TIER_COUNT,
        risk_level: p.risk.as_ref().map(|r| r.combined),
        risk_label: p.risk.map(|r| r.label.to_string()),
        plan,
        assumptions: a.classes,
        assumptions_note: a.note,
        disclosure: MODEL_DISCLOSURE,
    })
}

#[tauri::command]
pub fn plan_view(state: State<App>) -> CommandResult<PlanView> {
    let today = steps::today(now());
    state.with(|s| plan_view_of(s, today))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftView {
    asset_class: AssetClass,
    target: f64,
    current: f64,
    /// Current minus target.
    drift: f64,
    /// What the class holds now, in the base currency.
    value: Money,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebalanceView {
    base: Currency,
    /// The counted holdings, in the base currency.
    total: Money,
    /// The plan's blended target; `None` until there is a plan.
    has_plan: bool,
    rows: Vec<DriftView>,
    bands: Bands,
    /// Some class is outside its band now.
    breached: bool,
    /// Holdings in other currencies, left out until exchange rates arrive.
    left_out: usize,
    left_out_currencies: Vec<Currency>,
    /// Where a contribution (+) or withdrawal (-) would go, when one is given.
    cash_flow: Option<RebalancePlan>,
    /// The sell-based option, only while bands stay breached (after the cash
    /// flow, when there is one). A suggestion to consider, never an order.
    trades: Option<BTreeMap<AssetClass, Money>>,
    disclosure: &'static str,
}

pub fn rebalance_view_of(
    s: &Store,
    today: Date,
    cash_flow: Option<Money>,
) -> store::Result<RebalanceView> {
    let base = base_currency(&s.profile()?);
    let t: Totals = s.totals(base)?;
    let plan = s.plan(today)?;
    let bands = Bands::STANDARD;
    let mut left_out_currencies: Vec<Currency> = t
        .not_counted_no_fx
        .iter()
        .map(|h| h.amount.currency)
        .collect();
    left_out_currencies.sort();
    left_out_currencies.dedup();
    let mut v = RebalanceView {
        base,
        total: t.total,
        has_plan: plan.is_some(),
        rows: vec![],
        bands,
        breached: false,
        left_out: t.not_counted_no_fx.len(),
        left_out_currencies,
        cash_flow: None,
        trades: None,
        disclosure: MODEL_DISCLOSURE,
    };
    let Some(plan) = plan else {
        return Ok(v);
    };
    let target = &plan.blended;
    v.rows = drift_rows(target, &t.weights)
        .into_iter()
        .map(|r| DriftView {
            asset_class: r.asset_class,
            target: r.target,
            current: r.current,
            drift: r.drift(),
            value: t.by_class[&r.asset_class],
        })
        .collect();
    let held = t.total.minor > 0;
    v.breached = held && breaches_bands(target, &t.weights, bands);
    let mut still = v.breached;
    if let Some(cf) = cash_flow.filter(|m| m.minor != 0) {
        if cf.currency != base {
            return Err(Error::Invalid(format!(
                "enter the amount in {}, the currency your totals are in",
                base.code()
            )));
        }
        let p = cash_flow_rebalance(&t.by_class, target, cf, bands)?;
        still = p.still_breached;
        v.cash_flow = Some(p);
    }
    if still && held {
        v.trades = Some(rebalance::full_rebalance_trades(&t.by_class, target, base)?);
    }
    Ok(v)
}

/// The rebalancing card. `cash_flow` is a contribution (positive) or
/// withdrawal (negative) to spread; it is only worked out, never made.
#[tauri::command]
pub fn rebalance_view(state: State<App>, cash_flow: Option<Money>) -> CommandResult<RebalanceView> {
    let today = steps::today(now());
    state.with(|s| rebalance_view_of(s, today, cash_flow))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassForecast {
    asset_class: AssetClass,
    forecast: Forecast,
    /// The yearly returns it was fitted on (synthetic sample).
    history: Vec<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestView {
    rule: RebalanceRule,
    label: &'static str,
    /// The value at the start and at the end of each year.
    yearly: Vec<Money>,
    final_value: Money,
    cagr: f64,
    annual_vol: f64,
    max_drawdown: f64,
    rebalances: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectIndexView {
    stocks: usize,
    months: usize,
    harvest_threshold: f64,
    tax_rate: f64,
    index_values: Vec<Money>,
    portfolio_values: Vec<Money>,
    harvested_losses: Money,
    harvests: usize,
    tax_offset_value: Money,
    tracking_difference: f64,
    label: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsView {
    base: Currency,
    /// Next-year return per class (cash left out), on synthetic data.
    forecasts: Vec<ClassForecast>,
    /// The mix the backtest replays: the plan's, or a plain 60/40 until
    /// there is a plan.
    backtest_target: ClassWeights,
    target_from_plan: bool,
    start: Money,
    backtests: Vec<BacktestView>,
    direct_indexing: DirectIndexView,
    sample_label: &'static str,
    assumptions_as_of: Date,
    disclosure: &'static str,
}

/// A plain 60/40 until the risk questions are answered.
fn fallback_target() -> ClassWeights {
    ALL.iter()
        .map(|c| {
            let w = match c {
                AssetClass::Equity => 0.6,
                AssetClass::Bonds => 0.4,
                _ => 0.0,
            };
            (*c, w)
        })
        .collect()
}

pub fn insights_view_of(s: &Store, today: Date) -> store::Result<InsightsView> {
    let base = base_currency(&s.profile()?);
    let a = Assumptions::builtin();
    let data = synthetic::sample_monthly_returns(&a, FORECAST_SEED);
    let mut forecasts = Vec::new();
    for c in ALL.into_iter().filter(|c| *c != AssetClass::Cash) {
        let history = synthetic::annualise(&data[&c]);
        let f = forecast::forecast_ses(&history, 1, 0.80, forecast::MIN_HISTORY)?;
        forecasts.push(ClassForecast {
            asset_class: c,
            forecast: f,
            history,
        });
    }
    let plan = s.plan(today)?;
    let target_from_plan = plan.is_some();
    let target = plan.map_or_else(fallback_target, |p| p.blended);
    let start = whole(START_MAJOR, base);
    let mut backtests = Vec::new();
    for rule in [
        RebalanceRule::None,
        RebalanceRule::Annual,
        RebalanceRule::Threshold,
    ] {
        let r = backtest(&data, &target, rule, Bands::STANDARD, start)?;
        backtests.push(BacktestView {
            rule,
            label: rule.label(),
            yearly: r.values.iter().step_by(12).copied().collect(),
            final_value: r.final_value(),
            cagr: r.cagr,
            annual_vol: r.annual_vol,
            max_drawdown: r.max_drawdown,
            rebalances: r.rebalances,
        });
    }
    let u = synthetic_universe(INDEX_SEED, INDEX_STOCKS, INDEX_MONTHS);
    let d = simulate_direct_indexing(&u, start, HARVEST_AT, TAX_RATE)?;
    Ok(InsightsView {
        base,
        forecasts,
        backtest_target: target,
        target_from_plan,
        start,
        backtests,
        direct_indexing: DirectIndexView {
            stocks: u.stocks(),
            months: u.months(),
            harvest_threshold: HARVEST_AT,
            tax_rate: TAX_RATE,
            index_values: d.index_values,
            portfolio_values: d.portfolio_values,
            harvested_losses: d.harvested_losses,
            harvests: d.harvests,
            tax_offset_value: d.tax_offset_value,
            tracking_difference: d.tracking_difference,
            label: d.label,
        },
        sample_label: SAMPLE_DATA_LABEL,
        assumptions_as_of: a.as_of,
        disclosure: MODEL_DISCLOSURE,
    })
}

/// Forecast, backtest and the direct-indexing simulator, all on synthetic
/// data and labelled so.
#[tauri::command]
pub fn insights_view(state: State<App>) -> CommandResult<InsightsView> {
    let today = steps::today(now());
    state.with(|s| insights_view_of(s, today))
}

#[cfg(test)]
mod tests {
    use super::*;
    use samriddhi_core::plan::TIER_SETTING;
    use samriddhi_core::profile::{Goal, GoalKind, Source};
    use samriddhi_core::risk::{Answer, QUESTIONS};
    use samriddhi_core::wealth::Wrapper;

    const TODAY: Date = Date::constant(2026, 11, 6);

    fn gbp(major: i64) -> Money {
        whole(major, Currency::GBP)
    }

    fn answered() -> Store {
        let mut s = Store::memory().unwrap();
        let answers: Vec<Answer> = QUESTIONS
            .iter()
            .map(|q| Answer {
                question_id: q.id.into(),
                option_id: q.options[q.options.len() / 2].id.into(),
            })
            .collect();
        s.save_risk_answers(&answers, now()).unwrap();
        s
    }

    fn holding(s: &Store, class: AssetClass, amount: Money) {
        let a = match s.accounts().unwrap().first() {
            Some(a) => a.clone(),
            None => s.add_account("ISA", Wrapper::Isa, now()).unwrap(),
        };
        s.add_holding(&a.id, class.as_str(), class, amount, now())
            .unwrap();
    }

    #[test]
    fn no_plan_until_the_risk_questions_or_a_tier() {
        let s = Store::memory().unwrap();
        let v = plan_view_of(&s, TODAY).unwrap();
        assert!(v.plan.is_none());
        assert!(v.caps.is_empty());
        assert_eq!(v.tier_count, 7);
        assert_eq!(v.disclosure, MODEL_DISCLOSURE);
        s.set_setting(TIER_SETTING, &3).unwrap();
        let v = plan_view_of(&s, TODAY).unwrap();
        assert_eq!(v.tier, Some(3));
        assert_eq!(v.plan.unwrap().tier, Some(3));
    }

    #[test]
    fn goals_without_a_date_are_flagged_and_capped_by_their_assumed_horizon() {
        let mut s = answered();
        s.set_goals(
            &[
                Goal {
                    kind: GoalKind::EmergencyCushion,
                    target: Some(gbp(10_000)),
                    target_date: None,
                },
                Goal {
                    kind: GoalKind::RetireEarly,
                    target: Some(gbp(30_000)),
                    target_date: Some(Date::constant(2050, 1, 1)),
                },
            ],
            Source::Asked,
            now(),
        )
        .unwrap();
        let v = plan_view_of(&s, TODAY).unwrap();
        let plan = v.plan.unwrap();
        assert_eq!(plan.per_goal.len(), 2);
        assert!(plan.per_goal[0].goal.horizon_assumed);
        assert!(!plan.per_goal[1].goal.horizon_assumed);
        let h = plan.per_goal[1].goal.horizon_years;
        assert_eq!(v.caps, [max_growth_share(0.0), max_growth_share(h)]);
        assert!(plan.per_goal[0].allocation.growth_share() <= v.caps[0] + 1e-4);
        let sum: f64 = plan.blended.values().sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rebalancing_shows_drift_and_spreads_a_contribution_without_selling() {
        let s = answered();
        let empty = rebalance_view_of(&s, TODAY, None).unwrap();
        assert!(empty.has_plan);
        assert!(!empty.breached, "nothing held, nothing to breach");
        assert!(empty.trades.is_none());
        holding(&s, AssetClass::Equity, gbp(9_000));
        holding(&s, AssetClass::Cash, gbp(1_000));
        let v = rebalance_view_of(&s, TODAY, None).unwrap();
        assert_eq!(v.rows.len(), 5);
        assert_eq!(v.total, gbp(10_000));
        assert!(v.breached);
        let trades = v.trades.unwrap();
        let net: i64 = trades.values().map(|m| m.minor).sum();
        assert_eq!(net, 0, "the sell-based option nets to zero");
        let add = rebalance_view_of(&s, TODAY, Some(gbp(1_000))).unwrap();
        let flows = add.cash_flow.unwrap().flows;
        assert_eq!(
            flows.values().map(|m| m.minor).sum::<i64>(),
            gbp(1_000).minor
        );
        assert!(
            flows.values().all(|m| m.minor >= 0),
            "a contribution never sells"
        );
        assert!(rebalance_view_of(&s, TODAY, Some(whole(100, Currency::INR))).is_err());
    }

    #[test]
    fn other_currencies_are_left_out_of_rebalancing_and_named() {
        let s = answered();
        holding(&s, AssetClass::Equity, gbp(5_000));
        holding(&s, AssetClass::Cash, whole(90_000, Currency::INR));
        let v = rebalance_view_of(&s, TODAY, None).unwrap();
        assert_eq!(v.total, gbp(5_000));
        assert_eq!(v.left_out, 1);
        assert_eq!(v.left_out_currencies, [Currency::INR]);
    }

    #[test]
    fn insights_run_on_labelled_synthetic_data() {
        let s = Store::memory().unwrap();
        let v = insights_view_of(&s, TODAY).unwrap();
        assert_eq!(v.forecasts.len(), 4, "every class but cash");
        assert!(!v.target_from_plan);
        assert_eq!(v.backtest_target[&AssetClass::Equity], 0.6);
        assert_eq!(v.backtests.len(), 3);
        for b in &v.backtests {
            assert_eq!(b.yearly.len(), 21, "start plus 20 year ends");
            assert_eq!(b.yearly[0], gbp(START_MAJOR));
            assert_eq!(*b.yearly.last().unwrap(), b.final_value);
        }
        assert_eq!(v.backtests[0].rebalances, 0);
        assert_eq!(v.backtests[1].rebalances, 20);
        assert_eq!(v.direct_indexing.portfolio_values.len(), INDEX_MONTHS + 1);
        assert!(v.direct_indexing.label.contains("Synthetic"));
        assert!(v.sample_label.contains("Synthetic"));
        assert_eq!(v.disclosure, MODEL_DISCLOSURE);
        let with_plan = insights_view_of(&answered(), TODAY).unwrap();
        assert!(with_plan.target_from_plan);
    }
}
