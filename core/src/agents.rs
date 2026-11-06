//! Deterministic agents: small, pure producers of [`Step`]s that read the
//! plan, the holdings and the user's own step log, and the orchestrator
//! that runs them and merges what they say with the rule-based steps into
//! one feed. Ported from the samridhi prototype (`lib/agents/`).
//!
//! Safety boundary, kept in code: an agent is a pure function of its
//! context. No agent writes to the store, calls the network or acts on
//! anything; the strongest thing a user can do with a step is "Accept",
//! which only adds a row to the step log ([`Store::accept_step`]). Agent
//! steps go through the same "Later" and "Not for me" rules as the rule
//! steps. Every agent step says where its facts come from (`source`,
//! `as_of`) and how far to trust it (`basis`); steps built on the plan or
//! forecast models also carry [`MODEL_DISCLOSURE`].

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use jiff::civil::{Date, date};
use jiff::{SignedDuration, Timestamp};
use serde::Serialize;

use crate::assets::{ALL, AssetClass, Assumptions, MODEL_DISCLOSURE};
use crate::catalogue::Catalogue;
use crate::forecast::{self, Forecast};
use crate::money::{Currency, Money};
use crate::plan::{self, PlanResult};
use crate::profile::Profile;
use crate::rebalance::{self, Bands};
use crate::risk::RiskProfile;
use crate::step_log::{Decision, StepDecision};
use crate::steps::{self, Dismissal, Effort, LATER_DAYS, RULES_ORIGIN, Step, StepKind};
use crate::store::{Error, Result, Store};
use crate::synthetic;
use crate::wealth::{Totals, Wrapper};

/// Accepted rebalancing steps in the trailing year before the behaviour
/// nudge appears.
pub const BEHAVIOUR_THRESHOLD: usize = 3;
/// The trailing window the behaviour nudge counts over.
pub const BEHAVIOUR_WINDOW_DAYS: i64 = 365;
/// The step id of a suggested rebalance: the one the behaviour nudge counts.
pub const REBALANCE_STEP: &str = "rebalance:trades";
/// The seed of the synthetic sample the forecast agent reads (the same one
/// the forecast and backtest tests use).
pub const FORECAST_SEED: u64 = 7;
/// The day the general notes (wrapper rules, the behaviour figure) were
/// last checked against their sources.
const NOTES_AS_OF: Date = date(2026, 11, 5);

pub const RISK_PROFILE: &str = "risk-profile";
pub const PORTFOLIO: &str = "portfolio";
pub const FORECAST: &str = "forecast";
pub const REBALANCING: &str = "rebalancing";
pub const WRAPPER_RULES: &str = "wrapper-rules";
pub const BEHAVIOUR: &str = "behaviour";

/// Read-only inputs the agents reason over. Built by the core from the
/// store ([`Store::agent_context`]) or by tests; agents never see account
/// or holding names beyond what [`Totals`] holds, and never show them.
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub now: Timestamp,
    pub risk: Option<RiskProfile>,
    /// The plan, when the risk questions are answered.
    pub plan: Option<PlanResult>,
    /// Holdings added up in the base currency.
    pub totals: Option<Totals>,
    /// The kinds of account the user has.
    pub wrappers: BTreeSet<Wrapper>,
    /// The statistical equity outlook, on synthetic sample data.
    pub equity_forecast: Option<Forecast>,
    pub bands: Bands,
    /// When the user accepted a suggested rebalance (their own log).
    pub accepted_rebalances: Vec<Timestamp>,
}

impl AgentContext {
    /// An empty context at `now`: no answers, no plan, no holdings.
    pub fn empty(now: Timestamp) -> AgentContext {
        AgentContext {
            now,
            risk: None,
            plan: None,
            totals: None,
            wrappers: BTreeSet::new(),
            equity_forecast: None,
            bands: Bands::STANDARD,
            accepted_rebalances: Vec::new(),
        }
    }

    pub fn today(&self) -> Date {
        steps::today(self.now)
    }
}

/// A deterministic agent: a pure function of the context, with no I/O and
/// no AI. An error hides only this agent's steps.
pub trait Agent {
    /// Its name, which is also the origin of its steps.
    fn name(&self) -> &'static str;
    fn run(&self, ctx: &AgentContext) -> std::result::Result<Vec<Step>, String>;
}

struct Draft {
    id: &'static str,
    origin: &'static str,
    kind: StepKind,
    title: String,
    why: String,
    basis: String,
    source: String,
    as_of: Date,
}

impl Draft {
    fn step(self) -> Step {
        Step {
            id: self.id.into(),
            origin: self.origin.into(),
            kind: self.kind,
            title: self.title,
            why: self.why,
            risk: 1,
            effort: Effort::Minutes,
            source: self.source,
            as_of: self.as_of,
            ask: None,
            basis: Some(self.basis),
            disclosure: None,
        }
    }

    /// A step built on the plan or forecast models.
    fn modelled(self) -> Step {
        Step {
            disclosure: Some(MODEL_DISCLOSURE.into()),
            ..self.step()
        }
    }
}

fn pct(w: f64) -> i64 {
    (w * 100.0).round() as i64
}

fn pct1(w: f64) -> String {
    format!("{:.1}", w * 100.0)
}

/// What the risk questions came to, and what the plan makes of it.
pub struct RiskProfileAgent;

impl Agent for RiskProfileAgent {
    fn name(&self) -> &'static str {
        RISK_PROFILE
    }

    fn run(&self, ctx: &AgentContext) -> std::result::Result<Vec<Step>, String> {
        let Some(r) = &ctx.risk else {
            return Ok(vec![]);
        };
        let score = plan::risk_score_for_level(r.combined);
        Ok(vec![
            Draft {
                id: "risk:band",
                origin: RISK_PROFILE,
                kind: StepKind::Info,
                title: format!("{} risk profile", r.label),
                why: format!(
                    "Your answers put you at level {} of 5, which the plan reads as a score of \
                     {score:.0}/100. Goals you need within a few years are capped at less growth \
                     exposure whatever this level is.",
                    r.combined
                ),
                basis: "Rule-based from your answers; as reliable as those answers are.".into(),
                source: "Your answers to the risk questions".into(),
                as_of: ctx.today(),
            }
            .step(),
        ])
    }
}

/// The plan's blended target mix.
pub struct PortfolioAgent;

impl Agent for PortfolioAgent {
    fn name(&self) -> &'static str {
        PORTFOLIO
    }

    fn run(&self, ctx: &AgentContext) -> std::result::Result<Vec<Step>, String> {
        let Some(p) = &ctx.plan else {
            return Ok(vec![]);
        };
        let mix: Vec<String> = ALL
            .iter()
            .filter_map(|c| {
                let w = p.blended.get(c).copied().unwrap_or(0.0);
                (w >= 0.005).then(|| format!("{} {}%", c.label(), pct(w)))
            })
            .collect();
        let mut why = format!("{}.", mix.join(", "));
        if p.tier.is_some() {
            why.push_str(
                " Tier mode is on: every goal gets this same mix, so goals you need soon are \
                 not made safer.",
            );
        }
        Ok(vec![
            Draft {
                id: "portfolio:target",
                origin: PORTFOLIO,
                kind: StepKind::Info,
                title: "Target allocation".into(),
                why,
                basis: "Mean-variance optimiser on illustrative long-run assumptions, not \
                        market data. Treat it as a starting point."
                    .into(),
                source: p.assumptions_source.clone(),
                as_of: p.assumptions_as_of,
            }
            .modelled(),
        ])
    }
}

/// A statistical equity outlook with its band and track record.
pub struct ForecastAgent;

impl Agent for ForecastAgent {
    fn name(&self) -> &'static str {
        FORECAST
    }

    fn run(&self, ctx: &AgentContext) -> std::result::Result<Vec<Step>, String> {
        let Some(eq) = &ctx.equity_forecast else {
            return Ok(vec![]);
        };
        let (Some(mean), Some(lower), Some(upper)) =
            (eq.mean.first(), eq.lower.first(), eq.upper.first())
        else {
            return Err("the forecast has no first year".into());
        };
        let t = &eq.track;
        Ok(vec![
            Draft {
                id: "forecast:equity",
                origin: FORECAST,
                kind: StepKind::Info,
                title: "Equity outlook (statistical)".into(),
                why: format!(
                    "Smoothed annual return {}% ({}% band {}% to {}%). It does not change your \
                     target.",
                    pct1(*mean),
                    pct(eq.confidence),
                    pct1(*lower),
                    pct1(*upper)
                ),
                basis: format!(
                    "The band held in {} of {} past years on synthetic sample data. A simple \
                     model, not a prediction.",
                    t.hits, t.trials
                ),
                source: synthetic::SAMPLE_DATA_LABEL.into(),
                as_of: ctx
                    .plan
                    .as_ref()
                    .map_or(Assumptions::builtin().as_of, |p| p.assumptions_as_of),
            }
            .modelled(),
        ])
    }
}

/// Drift from the plan's target: within the bands, or the trades that would
/// bring it back. A suggestion for the user, never an order.
pub struct RebalancingAgent;

impl Agent for RebalancingAgent {
    fn name(&self) -> &'static str {
        REBALANCING
    }

    fn run(&self, ctx: &AgentContext) -> std::result::Result<Vec<Step>, String> {
        let (Some(p), Some(t)) = (&ctx.plan, &ctx.totals) else {
            return Ok(vec![]);
        };
        if t.total.minor <= 0 {
            return Ok(vec![]);
        }
        let mut basis = String::from("Exact arithmetic on the holdings you entered");
        if !t.not_counted_no_fx.is_empty() {
            basis.push_str(&format!(
                ", leaving out {} in other currencies until exchange rates arrive",
                t.not_counted_no_fx.len()
            ));
        }
        let source = format!(
            "Your holdings on this device, against the plan's target ({})",
            p.assumptions_source
        );
        if !rebalance::breaches_bands(&p.blended, &t.weights, ctx.bands) {
            return Ok(vec![
                Draft {
                    id: "rebalance:ok",
                    origin: REBALANCING,
                    kind: StepKind::Info,
                    title: "Within tolerance bands".into(),
                    why: format!(
                        "No asset class is more than {} percentage points from its target.",
                        pct1(ctx.bands.over.max(ctx.bands.under))
                    ),
                    basis: format!("{basis}."),
                    source,
                    as_of: ctx.today(),
                }
                .modelled(),
            ]);
        }
        let trades = rebalance::full_rebalance_trades(&t.by_class, &p.blended, t.base)
            .map_err(|e| e.to_string())?;
        let one = Money::new(10_i64.pow(u32::from(t.base.exponent())), t.base);
        let lines: Vec<String> = trades
            .iter()
            .filter(|(_, m)| m.minor.abs() >= one.minor)
            .map(|(c, m)| {
                let verb = if m.minor > 0 { "Buy" } else { "Sell" };
                format!(
                    "{verb} {} {}",
                    c.label(),
                    Money::new(m.minor.abs(), m.currency).format()
                )
            })
            .collect();
        let mut step = Draft {
            id: REBALANCE_STEP,
            origin: REBALANCING,
            kind: StepKind::Action,
            title: "Rebalance to target".into(),
            why: format!(
                "{}. Putting new money into the classes that are under target first can do most \
                 of this without selling (see Insights). Samriddhi never trades: this is for you \
                 to consider.",
                lines.join("; ")
            ),
            basis: format!(
                "{basis}; a threshold rule (a Vanguard-style 3% band), not a market view."
            ),
            source,
            as_of: ctx.today(),
        }
        .modelled();
        step.risk = 2;
        step.effort = Effort::Hours;
        Ok(vec![step])
    }
}

/// Tax-wrapper sanity: rebalance where selling costs no tax first.
pub struct WrapperRulesAgent;

impl Agent for WrapperRulesAgent {
    fn name(&self) -> &'static str {
        WRAPPER_RULES
    }

    fn run(&self, ctx: &AgentContext) -> std::result::Result<Vec<Step>, String> {
        let sheltered: Vec<&str> = [Wrapper::Isa, Wrapper::Pension]
            .iter()
            .filter(|w| ctx.wrappers.contains(w))
            .map(|w| match w {
                Wrapper::Isa => "ISA",
                _ => "pension",
            })
            .collect();
        if sheltered.is_empty() || !ctx.wrappers.contains(&Wrapper::Taxable) {
            return Ok(vec![]);
        }
        Ok(vec![
            Draft {
                id: "wrapper:location",
                origin: WRAPPER_RULES,
                kind: StepKind::Note,
                title: "Rebalance inside tax wrappers first".into(),
                why: format!(
                    "You hold both sheltered ({}) and taxable accounts. Selling inside an ISA or \
                     pension triggers no capital gains tax, so do any sells there before touching \
                     taxable accounts. Tax-loss harvesting only matters in taxable accounts.",
                    sheltered.join(", ")
                ),
                basis: "A general rule of thumb; rules vary by country and change. Check your \
                        own situation."
                    .into(),
                source: "General UK wrapper rules (ISA and pension sales are free of capital \
                         gains tax)"
                    .into(),
                as_of: NOTES_AS_OF,
            }
            .step(),
        ])
    }
}

/// The behavioural-gap nudge, from the user's own accepted rebalances. It
/// cites Betterment's published figure as general education only: it
/// describes one provider's clients, not a forecast for this user.
pub struct BehaviourAgent;

impl Agent for BehaviourAgent {
    fn name(&self) -> &'static str {
        BEHAVIOUR
    }

    fn run(&self, ctx: &AgentContext) -> std::result::Result<Vec<Step>, String> {
        let cutoff = ctx.now - SignedDuration::from_hours(24 * BEHAVIOUR_WINDOW_DAYS);
        let n = ctx
            .accepted_rebalances
            .iter()
            .filter(|t| **t > cutoff && **t <= ctx.now)
            .count();
        if n < BEHAVIOUR_THRESHOLD {
            return Ok(vec![]);
        }
        Ok(vec![
            Draft {
                id: "behaviour:changes",
                origin: BEHAVIOUR,
                kind: StepKind::Note,
                title: format!("You have changed course {n} times in 12 months"),
                why: format!(
                    "You accepted {n} rebalancing steps in the last 12 months. Betterment \
                     reported that its clients who made allocation changes trailed a simple hold \
                     of their own average allocation by about 22 basis points on average, and \
                     the gap grew with the number of changes. Rebalancing on a schedule, or only \
                     when a band is breached, avoids reacting to headlines."
                ),
                basis: "The count is from your own step log on this device. The 22 basis-point \
                        figure is one provider's published average over its clients: an \
                        educational reference, not a prediction for you."
                    .into(),
                source: "Your step log on this device; Betterment's published research on its \
                         clients' allocation changes"
                    .into(),
                as_of: NOTES_AS_OF,
            }
            .step(),
        ])
    }
}

/// Every agent, in the order the prototype ran them.
pub fn default_agents() -> Vec<Box<dyn Agent>> {
    vec![
        Box::new(RiskProfileAgent),
        Box::new(PortfolioAgent),
        Box::new(ForecastAgent),
        Box::new(RebalancingAgent),
        Box::new(WrapperRulesAgent),
        Box::new(BehaviourAgent),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    pub steps: Vec<Step>,
    /// "agent: what went wrong", one per agent that failed.
    pub errors: Vec<String>,
}

/// Run the agents in order. Each is isolated: one that fails (or panics)
/// is reported and the others still speak. Nothing is written anywhere.
pub fn run_agents(agents: &[Box<dyn Agent>], ctx: &AgentContext) -> AgentRun {
    let mut run = AgentRun {
        steps: vec![],
        errors: vec![],
    };
    for a in agents {
        match catch_unwind(AssertUnwindSafe(|| a.run(ctx))) {
            Ok(Ok(steps)) => run.steps.extend(steps),
            Ok(Err(e)) => run.errors.push(format!("{}: {e}", a.name())),
            Err(_) => run
                .errors
                .push(format!("{}: stopped unexpectedly", a.name())),
        }
    }
    run
}

/// Where a step goes in the feed: things to do from the rules first (they
/// are about safety: cushion, debt, allowances), then an agent's suggested
/// action, then cautions, then the one question the rules still need,
/// then plain information.
fn rank(s: &Step) -> u8 {
    match (s.origin == RULES_ORIGIN, s.kind, s.ask.is_some()) {
        (_, _, true) => 3,
        (true, StepKind::Action, false) => 0,
        (false, StepKind::Action, false) => 1,
        (_, StepKind::Note, false) => 2,
        (_, StepKind::Info, false) => 4,
    }
}

/// One feed from the rule steps and the agent steps. Steps the user put
/// off or turned down stay hidden by the same rules ([`steps::hidden`]);
/// steps accepted in the last [`LATER_DAYS`] days are hidden too, so one
/// decision is not counted twice. The order is [`rank`], keeping each
/// producer's own order inside a rank; a repeated id keeps the first.
pub fn merge_feed(
    rule_steps: Vec<Step>,
    agent_steps: Vec<Step>,
    dismissed: &[Dismissal],
    accepted_recently: &BTreeSet<String>,
    today: Date,
) -> Vec<Step> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<Step> = rule_steps
        .into_iter()
        .chain(agent_steps)
        .filter(|s| !steps::hidden(&s.id, dismissed, today))
        .filter(|s| !accepted_recently.contains(&s.id))
        .filter(|s| seen.insert(s.id.clone()))
        .collect();
    out.sort_by_key(rank);
    out
}

/// Can this step be accepted? Only an action that is not a question:
/// answering is how a question is taken up.
pub fn acceptable(s: &Step) -> bool {
    s.kind == StepKind::Action && s.ask.is_none()
}

/// The base currency the engines use until there is a setting for it
/// (Phase 3): the currency of the user's costs, cash or pay, else the tax
/// home's, else GBP.
pub fn base_currency(p: &Profile) -> Currency {
    if let Some(m) = p.costs.or(p.cash).or(p.take_home) {
        return m.currency;
    }
    match p.tax_home() {
        Some("IN") => Currency::INR,
        Some("AE") => Currency::AED,
        Some("US") => Currency::USD,
        _ => Currency::GBP,
    }
}

/// The equity outlook on the synthetic sample, as the forecast agent shows it.
pub fn equity_forecast(a: &Assumptions) -> Option<Forecast> {
    let data = synthetic::sample_monthly_returns(a, FORECAST_SEED);
    let yearly = synthetic::annualise(data.get(&AssetClass::Equity)?);
    forecast::forecast_ses(&yearly, 1, 0.80, forecast::MIN_HISTORY).ok()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feed {
    pub steps: Vec<Step>,
    /// Agents that could not run this time; the rest of the feed stands.
    pub errors: Vec<String>,
}

fn accepted_rebalances(log: &[StepDecision]) -> Vec<Timestamp> {
    log.iter()
        .filter(|d| {
            d.status == Decision::Accepted && d.source == REBALANCING && d.step_id == REBALANCE_STEP
        })
        .filter_map(|d| d.decided_at.parse().ok())
        .collect()
}

impl Store {
    /// What the agents read, from what is stored, in `base` currency.
    pub fn agent_context(&self, now: Timestamp, base: Currency) -> Result<AgentContext> {
        let p = self.profile()?;
        let a = Assumptions::builtin();
        let since = now - SignedDuration::from_hours(24 * BEHAVIOUR_WINDOW_DAYS);
        let wrappers = self.accounts()?.iter().map(|a| a.wrapper).collect();
        Ok(AgentContext {
            now,
            plan: plan::plan_for_profile(&a, &p, steps::today(now), self.plan_tier()?),
            risk: p.risk,
            totals: Some(self.totals(base)?),
            wrappers,
            equity_forecast: equity_forecast(&a),
            bands: Bands::STANDARD,
            accepted_rebalances: accepted_rebalances(&self.step_log_since(since)?),
        })
    }

    /// Today's feed: the rule steps and every agent's steps, merged
    /// ([`merge_feed`]). Reading it writes nothing.
    pub fn step_feed(&self, now: Timestamp, cat: &Catalogue) -> Result<Feed> {
        let p = self.profile()?;
        let today = steps::today(now);
        let dismissed = self.dismissals()?;
        let ctx = self.agent_context(now, base_currency(&p))?;
        let run = run_agents(&default_agents(), &ctx);
        let since = now - SignedDuration::from_hours(24 * i64::from(LATER_DAYS));
        let accepted: BTreeSet<String> = self
            .step_log_since(since)?
            .into_iter()
            .filter(|d| d.status == Decision::Accepted)
            .map(|d| d.step_id)
            .collect();
        Ok(Feed {
            steps: merge_feed(
                steps::next_steps(&p, today, cat, &dismissed),
                run.steps,
                &dismissed,
                &accepted,
                today,
            ),
            errors: run.errors,
        })
    }

    fn feed_step(&self, id: &str, now: Timestamp, cat: &Catalogue) -> Result<Option<Step>> {
        Ok(self
            .step_feed(now, cat)?
            .steps
            .into_iter()
            .find(|s| s.id == id))
    }

    /// "Accept": note in the step log that the user means to do it. That is
    /// all it does; nothing acts on it. Only an action in today's feed can
    /// be accepted.
    pub fn accept_step(&self, id: &str, now: Timestamp, cat: &Catalogue) -> Result<Step> {
        let step = self
            .feed_step(id, now, cat)?
            .ok_or_else(|| Error::Invalid("that step is not in your list now".into()))?;
        if !acceptable(&step) {
            return Err(Error::Invalid(
                "only a suggested action can be accepted; this one can be put off".into(),
            ));
        }
        self.log_step_decision(&step.id, &step.origin, &step.title, Decision::Accepted, now)?;
        Ok(step)
    }

    /// Note a "Later" or "Not for me" in the step log, when the step is in
    /// today's feed. The dismissal itself is stored as before.
    pub fn log_dismissal(&self, id: &str, now: Timestamp, cat: &Catalogue) -> Result<()> {
        if let Some(step) = self.feed_step(id, now, cat)? {
            self.log_step_decision(
                &step.id,
                &step.origin,
                &step.title,
                Decision::Dismissed,
                now,
            )?;
        }
        Ok(())
    }
}

/// Ported from the prototype's `test/agents/agents_test.dart` ("agents" and
/// "orchestrator" groups, and the decision-log test). Amounts there were
/// doubles; here they are exact GBP.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{GoalInput, build_plan};
    use crate::risk::{self, Answer, Capacity, QUESTIONS};
    use crate::store::tests::{NOW, at};
    use crate::wealth::{self, Holding};

    const GBP: Currency = Currency::GBP;

    fn risk_level(level: u8) -> RiskProfile {
        RiskProfile {
            attitude: level,
            capacity: Capacity::High,
            combined: level,
            label: risk::label(level),
            summary: String::new(),
        }
    }

    fn holdings(values: &[(AssetClass, i64)]) -> Vec<Holding> {
        values
            .iter()
            .enumerate()
            .map(|(i, (c, minor))| Holding {
                id: format!("h{i}"),
                account_id: "a".into(),
                name: format!("Secret holding {i}"),
                asset_class: *c,
                amount: Money::new(*minor, GBP),
            })
            .collect()
    }

    /// The prototype's context: a "Secret house fund" of 123,456 in 12 years,
    /// 60,000 in equities and 40,000 in bonds.
    fn ctx_with(values: &[(AssetClass, i64)], accepted: Vec<Timestamp>) -> AgentContext {
        let a = Assumptions::builtin();
        let goal = GoalInput::new("Secret house fund", Some(Money::new(12_345_600, GBP)), 12.0);
        let r = risk_level(2);
        AgentContext {
            now: at(NOW),
            plan: Some(build_plan(
                &a,
                plan::risk_score_for_level(r.combined),
                &[goal],
                None,
            )),
            risk: Some(r),
            totals: Some(wealth::totals(&holdings(values), GBP).unwrap()),
            wrappers: BTreeSet::new(),
            equity_forecast: equity_forecast(&a),
            bands: Bands::STANDARD,
            accepted_rebalances: accepted,
        }
    }

    fn ctx() -> AgentContext {
        ctx_with(
            &[
                (AssetClass::Equity, 6_000_000),
                (AssetClass::Bonds, 4_000_000),
            ],
            vec![],
        )
    }

    fn day(s: &str) -> Timestamp {
        at(&format!("{s}T12:00:00Z"))
    }

    #[test]
    fn every_agent_step_states_its_basis_source_and_date() {
        let run = run_agents(&default_agents(), &ctx());
        assert!(run.errors.is_empty(), "{:?}", run.errors);
        let ids: Vec<&str> = run.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "risk:band",
                "portfolio:target",
                "forecast:equity",
                REBALANCE_STEP
            ]
        );
        for s in &run.steps {
            assert!(
                !s.basis.as_deref().unwrap_or("").trim().is_empty(),
                "{}",
                s.id
            );
            assert!(!s.source.trim().is_empty(), "{}", s.id);
            assert_ne!(s.origin, RULES_ORIGIN);
        }
        for id in ["portfolio:target", "forecast:equity", REBALANCE_STEP] {
            let s = run.steps.iter().find(|s| s.id == id).unwrap();
            assert_eq!(s.disclosure.as_deref(), Some(MODEL_DISCLOSURE), "{id}");
        }
        let risk = &run.steps[0];
        assert_eq!(risk.disclosure, None);
        assert_eq!(risk.title, "Cautious risk profile");
        assert!(risk.why.contains("25/100"), "{}", risk.why);
    }

    #[test]
    fn behaviour_nudge_needs_three_accepted_rebalances_in_the_last_year() {
        let recent = vec![day("2026-02-01"), day("2026-06-01"), day("2026-09-01")];
        let two = ctx_with(&[], recent[..2].to_vec());
        assert!(BehaviourAgent.run(&two).unwrap().is_empty());
        let mut old = vec![day("2024-01-01")];
        old.extend_from_slice(&recent[..2]);
        assert!(BehaviourAgent.run(&ctx_with(&[], old)).unwrap().is_empty());
        let s = BehaviourAgent.run(&ctx_with(&[], recent)).unwrap();
        let s = &s[0];
        assert!(s.title.contains("3 times"), "{}", s.title);
        assert!(s.why.contains("22 basis points"));
        assert!(s.why.contains("Betterment"));
        let basis = s.basis.as_deref().unwrap();
        assert!(basis.contains("not a prediction"));
        assert!(basis.contains("one provider's published average"));
        assert!(s.source.contains("Betterment"));
        assert!(!acceptable(s), "a note is never something to accept");
    }

    #[test]
    fn rebalancing_on_target_is_information_and_drift_is_an_action() {
        let c = ctx();
        let target = &c.plan.as_ref().unwrap().blended;
        let on: Vec<(AssetClass, i64)> = ALL
            .iter()
            .map(|k| (*k, (target[k] * 10_000_000.0).round() as i64))
            .collect();
        let ok = RebalancingAgent.run(&ctx_with(&on, vec![])).unwrap();
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].id, "rebalance:ok");
        assert!(!acceptable(&ok[0]));

        let drifted = RebalancingAgent
            .run(&ctx_with(&[(AssetClass::Equity, 10_000_000)], vec![]))
            .unwrap();
        assert_eq!(drifted.len(), 1);
        assert!(acceptable(&drifted[0]));
        assert!(
            drifted[0].why.contains("Sell Equities"),
            "{}",
            drifted[0].why
        );
        assert!(drifted[0].why.contains("never trades"));
        assert_eq!(drifted[0].disclosure.as_deref(), Some(MODEL_DISCLOSURE));
    }

    #[test]
    fn no_holdings_means_no_rebalancing_step() {
        assert!(
            RebalancingAgent
                .run(&ctx_with(&[], vec![]))
                .unwrap()
                .is_empty()
        );
        let mut c = ctx();
        c.totals = None;
        assert!(RebalancingAgent.run(&c).unwrap().is_empty());
    }

    #[test]
    fn holdings_in_other_currencies_are_named_as_left_out() {
        let mut c = ctx_with(&[(AssetClass::Equity, 10_000_000)], vec![]);
        let mut hs = holdings(&[(AssetClass::Equity, 10_000_000)]);
        hs.push(Holding {
            amount: Money::new(500, Currency::INR),
            ..hs[0].clone()
        });
        c.totals = Some(wealth::totals(&hs, GBP).unwrap());
        let s = RebalancingAgent.run(&c).unwrap();
        assert!(
            s[0].basis
                .as_deref()
                .unwrap()
                .contains("leaving out 1 in other currencies"),
            "{:?}",
            s[0].basis
        );
    }

    #[test]
    fn wrapper_note_only_with_both_sheltered_and_taxable_accounts() {
        let with = |ws: &[Wrapper]| {
            let mut c = ctx();
            c.wrappers = ws.iter().copied().collect();
            WrapperRulesAgent.run(&c).unwrap()
        };
        assert!(with(&[Wrapper::Taxable]).is_empty());
        assert!(with(&[Wrapper::Isa]).is_empty());
        let s = with(&[Wrapper::Isa, Wrapper::Taxable]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, StepKind::Note);
        assert!(s[0].why.contains("(ISA)"));
    }

    #[test]
    fn nothing_known_means_no_agent_steps() {
        let run = run_agents(&default_agents(), &AgentContext::empty(at(NOW)));
        assert!(run.steps.is_empty());
        assert!(run.errors.is_empty());
    }

    struct Boom;
    impl Agent for Boom {
        fn name(&self) -> &'static str {
            "boom"
        }
        fn run(&self, _: &AgentContext) -> std::result::Result<Vec<Step>, String> {
            Err("kaput".into())
        }
    }

    struct Panics;
    impl Agent for Panics {
        fn name(&self) -> &'static str {
            "panics"
        }
        fn run(&self, _: &AgentContext) -> std::result::Result<Vec<Step>, String> {
            panic!("on purpose")
        }
    }

    #[test]
    fn a_failing_agent_does_not_hide_the_others() {
        let agents: Vec<Box<dyn Agent>> =
            vec![Box::new(Boom), Box::new(Panics), Box::new(RiskProfileAgent)];
        let run = run_agents(&agents, &ctx());
        assert_eq!(run.steps.len(), 1);
        assert_eq!(run.steps[0].id, "risk:band");
        assert_eq!(run.errors, ["boom: kaput", "panics: stopped unexpectedly"]);
    }

    fn step(id: &str, origin: &str, kind: StepKind, ask: bool) -> Step {
        Step {
            id: id.into(),
            origin: origin.into(),
            kind,
            title: id.into(),
            why: String::new(),
            risk: 1,
            effort: Effort::Minutes,
            source: "s".into(),
            as_of: date(2026, 11, 4),
            ask: ask.then(|| steps::Ask { fact: "f".into() }),
            basis: None,
            disclosure: None,
        }
    }

    #[test]
    fn the_feed_orders_rules_then_actions_then_notes_then_the_question_then_information() {
        let rules = vec![
            step("cushion", RULES_ORIGIN, StepKind::Action, false),
            step("ask:x", RULES_ORIGIN, StepKind::Action, true),
        ];
        let agents = vec![
            step("risk:band", RISK_PROFILE, StepKind::Info, false),
            step(REBALANCE_STEP, REBALANCING, StepKind::Action, false),
            step("wrapper:location", WRAPPER_RULES, StepKind::Note, false),
            step("cushion", REBALANCING, StepKind::Info, false),
        ];
        let today = date(2026, 11, 4);
        let feed = merge_feed(rules, agents, &[], &BTreeSet::new(), today);
        let ids: Vec<&str> = feed.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "cushion",
                REBALANCE_STEP,
                "wrapper:location",
                "ask:x",
                "risk:band"
            ]
        );
        assert_eq!(
            feed[0].origin, RULES_ORIGIN,
            "a repeated id keeps the first"
        );
    }

    #[test]
    fn agent_steps_follow_the_same_later_and_not_for_me_rules() {
        let today = date(2026, 11, 4);
        let agents = || {
            vec![
                step(REBALANCE_STEP, REBALANCING, StepKind::Action, false),
                step("wrapper:location", WRAPPER_RULES, StepKind::Note, false),
            ]
        };
        let dismissed = vec![
            Dismissal {
                step_id: REBALANCE_STEP.into(),
                kind: steps::DismissKind::Later,
                reason: None,
                on: today,
            },
            Dismissal {
                step_id: "wrapper:location".into(),
                kind: steps::DismissKind::NotForMe,
                reason: Some("no taxable account really".into()),
                on: today,
            },
        ];
        let none = BTreeSet::new();
        assert!(merge_feed(vec![], agents(), &dismissed, &none, today).is_empty());
        let later = today
            .checked_add(jiff::Span::new().days(LATER_DAYS))
            .unwrap();
        let back = merge_feed(vec![], agents(), &dismissed, &none, later);
        let ids: Vec<&str> = back.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, [REBALANCE_STEP], "Later ends; Not for me stays");
        let accepted: BTreeSet<String> = [REBALANCE_STEP.to_string()].into();
        assert!(merge_feed(vec![], agents(), &dismissed, &accepted, later).is_empty());
    }

    #[test]
    fn base_currency_follows_the_users_own_amounts() {
        let mut p = Profile::default();
        assert_eq!(base_currency(&p), GBP);
        p.countries = vec![crate::profile::CountryTie {
            code: "IN".into(),
            residence: true,
            tax_home: true,
            citizen: true,
            holds_money: true,
            ties: String::new(),
        }];
        assert_eq!(base_currency(&p), Currency::INR);
        p.cash = Some(Money::new(1, Currency::AED));
        assert_eq!(base_currency(&p), Currency::AED);
    }

    /// A store with the risk questions answered at "balanced", one ISA and
    /// one taxable account, all in equities: so a rebalance is suggested.
    fn store_with_plan() -> Store {
        let mut s = Store::memory().unwrap();
        let answers: Vec<Answer> = QUESTIONS
            .iter()
            .map(|q| Answer {
                question_id: q.id.into(),
                option_id: q.options[q.options.len() / 2].id.into(),
            })
            .collect();
        s.save_risk_answers(&answers, at(NOW)).unwrap();
        let isa = s.add_account("Secret ISA", Wrapper::Isa, at(NOW)).unwrap();
        let gia = s
            .add_account("Secret GIA", Wrapper::Taxable, at(NOW))
            .unwrap();
        for acc in [&isa, &gia] {
            s.add_holding(
                &acc.id,
                "Secret world fund",
                AssetClass::Equity,
                Money::new(5_000_000, GBP),
                at(NOW),
            )
            .unwrap();
        }
        s
    }

    #[test]
    fn the_stored_feed_holds_rule_and_agent_steps_and_reading_it_writes_nothing() {
        let s = store_with_plan();
        let cat = Catalogue::builtin();
        let feed = s.step_feed(at(NOW), &cat).unwrap();
        assert!(feed.errors.is_empty(), "{:?}", feed.errors);
        let origins: BTreeSet<&str> = feed.steps.iter().map(|s| s.origin.as_str()).collect();
        assert!(origins.contains(RULES_ORIGIN), "{origins:?}");
        for o in [
            RISK_PROFILE,
            PORTFOLIO,
            FORECAST,
            REBALANCING,
            WRAPPER_RULES,
        ] {
            assert!(origins.contains(o), "{o} missing from {origins:?}");
        }
        for st in &feed.steps {
            assert!(!st.why.contains("Secret") && !st.title.contains("Secret"));
        }
        assert!(
            s.step_log_since(at("2000-01-01T00:00:00Z"))
                .unwrap()
                .is_empty()
        );
        assert!(s.dismissals().unwrap().is_empty());
    }

    #[test]
    fn accept_only_logs_intent_and_three_accepts_bring_the_nudge() {
        let s = store_with_plan();
        let cat = Catalogue::builtin();
        let holdings_before = s.holdings().unwrap();
        let t0 = at(NOW);
        // A day past the "accepted" hiding, so each accept is a new decision.
        let fortnight = SignedDuration::from_hours(24 * (i64::from(LATER_DAYS) + 1));
        for i in 0..3 {
            let t = t0 + fortnight * i;
            let feed = s.step_feed(t, &cat).unwrap();
            assert!(
                !feed.steps.iter().any(|s| s.id == "behaviour:changes"),
                "no nudge after {i}"
            );
            let st = s.accept_step(REBALANCE_STEP, t, &cat).unwrap();
            assert_eq!(st.origin, REBALANCING);
            // Accepted steps stay out of the feed for a while.
            assert!(s.accept_step(REBALANCE_STEP, t, &cat).is_err());
        }
        assert_eq!(s.holdings().unwrap(), holdings_before, "accept never acts");
        let log = s.step_log_since(t0).unwrap();
        assert_eq!(log.len(), 3);
        assert!(log.iter().all(|d| d.status == Decision::Accepted));
        let feed = s.step_feed(t0 + fortnight * 3, &cat).unwrap();
        let nudge = feed
            .steps
            .iter()
            .find(|s| s.id == "behaviour:changes")
            .expect("nudge after three accepts");
        assert!(nudge.title.contains("3 times"));
        // A year after the first accept it counts two again, and goes.
        let later = t0 + SignedDuration::from_hours(24 * 366);
        assert!(
            !s.step_feed(later, &cat)
                .unwrap()
                .steps
                .iter()
                .any(|s| s.id == "behaviour:changes")
        );
    }

    #[test]
    fn information_notes_and_questions_cannot_be_accepted() {
        let s = store_with_plan();
        let cat = Catalogue::builtin();
        for id in ["risk:band", "wrapper:location", "portfolio:target"] {
            let e = s.accept_step(id, at(NOW), &cat).unwrap_err().to_string();
            assert!(e.contains("only a suggested action"), "{id}: {e}");
        }
        let ask = s
            .step_feed(at(NOW), &cat)
            .unwrap()
            .steps
            .into_iter()
            .find(|s| s.ask.is_some())
            .expect("a new profile has a question");
        assert!(s.accept_step(&ask.id, at(NOW), &cat).is_err());
        assert!(s.accept_step("no-such-step", at(NOW), &cat).is_err());
        assert!(s.step_log_since(at(NOW)).unwrap().is_empty());
    }

    #[test]
    fn dismissing_from_the_feed_is_logged_and_hides_the_agent_step() {
        let s = store_with_plan();
        let cat = Catalogue::builtin();
        s.log_dismissal("wrapper:location", at(NOW), &cat).unwrap();
        s.step_not_for_me("wrapper:location", "only a small taxable pot", at(NOW))
            .unwrap();
        s.log_dismissal("not-in-feed", at(NOW), &cat).unwrap();
        let log = s.step_log_since(at(NOW)).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(
            (log[0].source.as_str(), log[0].status),
            (WRAPPER_RULES, Decision::Dismissed)
        );
        let feed = s.step_feed(at(NOW), &cat).unwrap();
        assert!(!feed.steps.iter().any(|s| s.id == "wrapper:location"));
    }
}
