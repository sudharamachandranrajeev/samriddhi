//! The next-best-step engine, version 0. Plain ordered rules over a profile
//! snapshot and today's (UTC) date; no clock, no store, no randomness, so
//! the same facts always give the same steps. The rules, in order:
//!
//! 1. Cash cushion under three months of costs.
//! 2. High-interest debt.
//! 3. ISA allowance left before 5 April (amounts from the dated catalogue).
//! 4. Employer pension match unknown: ask.
//! 5. A fact the rules need is missing: ask one question, the first that
//!    has not been put off.
//!
//! "Later" hides a step for [`LATER_DAYS`]; "Not for me" hides it and keeps
//! the reason (later phases learn from it).

use jiff::civil::Date;
use jiff::{Timestamp, tz::TimeZone};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::catalogue::{Catalogue, Rule, TaxYear};
use crate::money::Money;
use crate::profile::{self, PensionMatch, Profile};
use crate::store::{Error, Result, Store, iso};

/// Days "Later" hides a step.
pub const LATER_DAYS: i32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effort {
    /// A few minutes: answer a question, check a payslip.
    Minutes,
    /// An hour or two: open an account, set up a payment.
    Hours,
    /// A habit kept up over months.
    Ongoing,
}

/// A question a step asks, for the profile fact it fills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    /// A profile fact key, or "countries", "goals" or "risk".
    pub fact: String,
}

/// What a step is. Only an action can be accepted; information and notes
/// can be put off or turned down like any other step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepKind {
    /// Something the user could do.
    Action,
    /// A figure or result to know about.
    Info,
    /// A rule of thumb or a caution.
    Note,
}

/// The producer of the rule-based steps, in the feed and the step log.
pub const RULES_ORIGIN: &str = "steps";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Stable, so "Later" and "Not for me" stick to it.
    pub id: String,
    /// Who suggested it: [`RULES_ORIGIN`] for these rules, or an agent's
    /// name ("rebalancing", "behaviour"...).
    pub origin: String,
    pub kind: StepKind,
    pub title: String,
    pub why: String,
    /// 1 (nothing can be lost) to 5 (could lose much of it).
    pub risk: u8,
    pub effort: Effort,
    /// Where the facts behind it come from.
    pub source: String,
    pub as_of: Date,
    pub ask: Option<Ask>,
    /// How far to trust it, in plain words. Every agent step has one.
    pub basis: Option<String>,
    /// The model disclosure, on every step built on the plan or forecast
    /// models ([`crate::assets::MODEL_DISCLOSURE`]).
    pub disclosure: Option<String>,
}

impl Step {
    fn new(id: impl Into<String>, rule: &Rule, title: String, why: String) -> Step {
        Step {
            id: id.into(),
            origin: RULES_ORIGIN.into(),
            kind: StepKind::Action,
            title,
            why,
            risk: 1,
            effort: Effort::Minutes,
            source: rule.source.clone(),
            as_of: rule.as_of,
            ask: None,
            basis: None,
            disclosure: None,
        }
    }

    fn effort(mut self, effort: Effort) -> Step {
        self.effort = effort;
        self
    }

    fn risk(mut self, risk: u8) -> Step {
        self.risk = risk;
        self
    }

    fn asking(mut self, fact: &str) -> Step {
        self.ask = Some(Ask { fact: fact.into() });
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DismissKind {
    Later,
    NotForMe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dismissal {
    pub step_id: String,
    pub kind: DismissKind,
    pub reason: Option<String>,
    /// The UTC day it was dismissed.
    pub on: Date,
}

/// Today in UTC, the day every rule is judged on.
pub fn today(now: Timestamp) -> Date {
    now.to_zoned(TimeZone::UTC).date()
}

/// Is this step hidden today? "Later" lasts [`LATER_DAYS`] from the day it
/// was pressed; "Not for me" lasts until the user takes it back.
pub fn hidden(step_id: &str, dismissed: &[Dismissal], today: Date) -> bool {
    dismissed.iter().any(|d| {
        d.step_id == step_id
            && match d.kind {
                DismissKind::NotForMe => true,
                DismissKind::Later => {
                    d.on.checked_add(jiff::Span::new().days(LATER_DAYS))
                        .is_ok_and(|until| today < until)
                }
            }
    })
}

/// The steps to show today, in rule order, without the hidden ones.
pub fn next_steps(p: &Profile, today: Date, cat: &Catalogue, dismissed: &[Dismissal]) -> Vec<Step> {
    let mut steps: Vec<Step> = [
        cushion(p, cat),
        high_interest_debt(p, cat),
        isa_allowance(p, today, cat),
        pension_match(p, cat),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !hidden(&s.id, dismissed, today))
    .collect();
    steps.extend(
        missing(p, today, cat)
            .into_iter()
            .find(|s| !hidden(&s.id, dismissed, today)),
    );
    steps
}

/// Months of costs the cash covers, to one decimal place, rounded down.
fn months_covered(cash: Money, costs: Money) -> Option<(i64, i64)> {
    if costs.currency != cash.currency || costs.minor <= 0 {
        return None;
    }
    let tenths = (cash.minor as i128 * 10) / costs.minor as i128;
    let tenths = i64::try_from(tenths).ok()?;
    Some((tenths / 10, tenths % 10))
}

fn cushion(p: &Profile, cat: &Catalogue) -> Option<Step> {
    let (rule, months) = cat.cushion()?;
    let (cash, costs) = (p.cash?, p.costs?);
    let (whole, tenth) = months_covered(cash, costs)?;
    let target = costs.checked_mul(months).ok()?;
    let short = target.checked_sub(cash).ok()?;
    if short.minor <= 0 {
        return None;
    }
    let why = format!(
        "Your cash covers about {whole}.{tenth} months of your costs. Keeping at least {months} \
         months in an easy-access account means a surprise bill or a gap in pay does not turn \
         into debt. You need about {} more.",
        short.format()
    );
    Some(
        Step::new(
            "cushion",
            rule,
            format!("Build a {months}-month safety cushion"),
            why,
        )
        .effort(Effort::Ongoing),
    )
}

fn high_interest_debt(p: &Profile, cat: &Catalogue) -> Option<Step> {
    let (rule, line) = cat.high_interest()?;
    let costly: Vec<&profile::Debt> = p
        .debts
        .as_ref()?
        .iter()
        .filter(|d| d.balance.minor > 0 && d.band.floor_percent().is_some_and(|f| f >= line))
        .collect();
    let worst = costly.iter().max_by_key(|d| d.band.floor_percent())?;
    let names: Vec<&str> = costly.iter().map(|d| d.label.as_str()).collect();
    let why = format!(
        "{} cost {line}% a year or more ({} at {}). Paying it off earns that rate back for \
         certain, which savings rarely match. Keep paying the minimum on everything, then put \
         anything extra on the highest rate first.",
        names.join(", "),
        worst.label,
        worst.band.words()
    );
    Some(
        Step::new(
            "high-interest-debt",
            rule,
            "Pay down your most expensive debt".into(),
            why,
        )
        .effort(Effort::Ongoing),
    )
}

/// The UK ISA rule applies to UK residents whose tax home is the UK.
fn uk_saver(p: &Profile) -> bool {
    p.tax_home() == Some("GB") && p.lives_in("GB")
}

fn isa_allowance(p: &Profile, today: Date, cat: &Catalogue) -> Option<Step> {
    if !uk_saver(p) {
        return None;
    }
    let year = TaxYear::of(today);
    let (rule, allowance) = cat.isa_allowance(year)?;
    let used = p.isa_used.filter(|u| u.tax_year == year).map(|u| u.amount);
    let left = match used {
        Some(u) => allowance.checked_sub(u).ok()?,
        None => allowance,
    };
    if left.minor <= 0 {
        return None;
    }
    let last = year.last_day();
    let days = (last - today).get_days();
    let mut why = format!(
        "In the {} tax year you can put up to {} into ISAs. Interest and growth inside an ISA \
         are free of UK tax, and allowance you do not use is lost on {}.",
        year.label(),
        allowance.format(),
        last.strftime("%-d %B %Y")
    );
    why.push_str(&match used {
        Some(_) => format!(" You have about {} left.", left.format()),
        None => {
            " Tell us roughly how much you have paid in since 6 April to see what is left.".into()
        }
    });
    match days {
        0 => why.push_str(" Today is the last day."),
        1..=30 => why.push_str(&format!(" {days} days to go.")),
        _ => {}
    }
    if let Some((_, cap, age)) = cat.cash_isa_cap(year) {
        why.push_str(&format!(
            " If you are under {age}, at most {} of it can go into cash ISAs.",
            cap.format()
        ));
    } else if let Some((cap_rule, cap, age)) = cat.cash_isa_cap(TaxYear(year.0 + 1)) {
        let from = match &cap_rule.kind {
            crate::catalogue::RuleKind::CashIsaCap { from, .. } => *from,
            _ => year.last_day(),
        };
        why.push_str(&format!(
            " From {}, if you are under {age}, at most {} a year can go into cash ISAs.",
            from.strftime("%-d %B %Y"),
            cap.format()
        ));
    }
    Some(
        Step::new(
            format!("isa-allowance-{}", year.0),
            rule,
            format!(
                "Use your ISA allowance before {}",
                last.strftime("%-d %B %Y")
            ),
            why,
        )
        .effort(Effort::Hours)
        .risk(2),
    )
}

fn pension_match(p: &Profile, cat: &Catalogue) -> Option<Step> {
    if p.tax_home() != Some("GB") || !matches!(p.pension_match, None | Some(PensionMatch::Unsure)) {
        return None;
    }
    let rule = cat.pension_match()?;
    Some(
        Step::new(
            "ask:pension.employerMatch",
            rule,
            "Does your employer add more to your pension when you pay in more?".into(),
            "Some employers pay in more than the legal minimum when you do. If yours does, \
             not using it is like turning down part of your pay. Your payslip, staff handbook \
             or HR team can tell you."
                .into(),
        )
        .asking(profile::PENSION_MATCH),
    )
}

/// Questions for the facts the rules still need, most useful first. Only one
/// is shown at a time: the first not put off.
fn missing(p: &Profile, today: Date, cat: &Catalogue) -> Vec<Step> {
    let mut out = Vec::new();
    let ask = |fact: &str, rule: Option<&Rule>, title: String, why: &str| {
        rule.map(|r| Step::new(format!("ask:{fact}"), r, title, why.into()).asking(fact))
    };
    let cushion = cat.cushion().map(|(r, _)| r);
    let debt = cat.high_interest().map(|(r, _)| r);
    if p.costs.is_none() {
        out.extend(ask(
            profile::COSTS,
            cushion,
            "Roughly how much do you spend in a month?".into(),
            profile::why(profile::COSTS).unwrap_or_default(),
        ));
    }
    match (p.cash, p.costs) {
        (None, _) => out.extend(ask(
            profile::CASH,
            cushion,
            "Roughly how much cash do you have in savings and current accounts?".into(),
            profile::why(profile::CASH).unwrap_or_default(),
        )),
        (Some(cash), Some(costs)) if cash.currency != costs.currency => out.extend(ask(
            profile::CASH,
            cushion,
            format!(
                "Roughly how much cash do you have, in {} like your costs?",
                costs.currency
            ),
            "Your cash and costs are in different currencies, so we cannot compare them yet.",
        )),
        _ => {}
    }
    match &p.debts {
        None => out.extend(ask(
            profile::DEBTS,
            debt,
            "Do you owe money on cards, loans or overdrafts? Rough amounts are fine.".into(),
            profile::why(profile::DEBTS).unwrap_or_default(),
        )),
        Some(list) => {
            if let Some(d) = list.iter().find(|d| d.band.floor_percent().is_none()) {
                out.extend(ask(
                    profile::DEBTS,
                    debt,
                    format!("Roughly what interest rate is your {} at?", d.label),
                    "The rate decides whether paying it off beats saving.",
                ));
            }
        }
    }
    if p.take_home.is_none() {
        out.extend(ask(
            profile::TAKE_HOME,
            cushion,
            "Roughly how much do you take home in a month, after tax?".into(),
            profile::why(profile::TAKE_HOME).unwrap_or_default(),
        ));
    }
    let isa = cat.isa_allowance(TaxYear::of(today)).map(|(r, _)| r);
    if p.countries.is_empty() {
        out.extend(ask(
            "countries",
            isa.or(cushion),
            "Which countries do you live in, pay tax in and keep money in?".into(),
            "Tax rules, accounts and allowances depend on the country; we only suggest what fits yours.",
        ));
    }
    let year = TaxYear::of(today);
    if uk_saver(p) && p.isa_used.is_none_or(|u| u.tax_year != year) {
        out.extend(ask(
            profile::ISA_USED,
            isa,
            format!(
                "How much have you paid into ISAs since {}?",
                year.first_day().strftime("%-d %B %Y")
            ),
            profile::why(profile::ISA_USED).unwrap_or_default(),
        ));
    }
    if p.goals.is_empty() {
        out.extend(ask(
            "goals",
            cat.rule("uk.savings-goals"),
            "What are you saving or working towards?".into(),
            "Steps are ordered by what matters to you.",
        ));
    }
    if p.risk.is_none() {
        out.extend(ask(
            "risk",
            cat.rule("uk.risk-and-capacity"),
            "Ten short questions about risk".into(),
            "Some steps can go down in value; your answers keep suggestions within what you are \
             comfortable with and could afford to lose.",
        ));
    }
    out
}

fn kind_str(k: DismissKind) -> &'static str {
    match k {
        DismissKind::Later => "later",
        DismissKind::NotForMe => "notForMe",
    }
}

impl Store {
    /// Hide a step for [`LATER_DAYS`] from today.
    pub fn step_later(&self, step_id: &str, now: Timestamp) -> Result<()> {
        self.dismiss(step_id, DismissKind::Later, None, now)
    }

    /// Hide a step for good, keeping the reason the user gave.
    pub fn step_not_for_me(&self, step_id: &str, reason: &str, now: Timestamp) -> Result<()> {
        let reason = reason.trim();
        if reason.chars().count() > 500 {
            return Err(Error::Invalid("keep the reason under 500 letters".into()));
        }
        let reason = (!reason.is_empty()).then_some(reason);
        self.dismiss(step_id, DismissKind::NotForMe, reason, now)
    }

    /// Show a dismissed step again.
    pub fn step_restore(&self, step_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sam_steps_dismissed WHERE step_id = ?",
            [step_id],
        )?;
        Ok(())
    }

    fn dismiss(
        &self,
        step_id: &str,
        kind: DismissKind,
        reason: Option<&str>,
        now: Timestamp,
    ) -> Result<()> {
        if step_id.is_empty() || step_id.len() > 100 {
            return Err(Error::Invalid("odd step id".into()));
        }
        self.conn.execute(
            "INSERT INTO sam_steps_dismissed (step_id, kind, reason, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(step_id) DO UPDATE SET kind = ?2, reason = ?3, at = ?4",
            params![step_id, kind_str(kind), reason, iso(now)],
        )?;
        Ok(())
    }

    pub fn dismissals(&self) -> Result<Vec<Dismissal>> {
        let mut st = self
            .conn
            .prepare("SELECT step_id, kind, reason, at FROM sam_steps_dismissed ORDER BY at")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (step_id, kind, reason, at) = row?;
            let kind = match kind.as_str() {
                "later" => DismissKind::Later,
                _ => DismissKind::NotForMe,
            };
            let at: Timestamp = at
                .parse()
                .map_err(|e| Error::Invalid(format!("odd date {at:?}: {e}")))?;
            Ok(Dismissal {
                step_id,
                kind,
                reason,
                on: today(at),
            })
        })
        .collect()
    }

    /// The steps for today from what is stored.
    pub fn next_steps(&self, now: Timestamp, cat: &Catalogue) -> Result<Vec<Step>> {
        Ok(next_steps(
            &self.profile()?,
            today(now),
            cat,
            &self.dismissals()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;
    use crate::profile::{CountryTie, Debt, Goal, GoalKind, IsaUsed, RateBand};
    use crate::risk::{Capacity, RiskProfile};
    use crate::store::tests::{NOW, at};
    use jiff::civil::date;

    fn gbp(pounds: i64) -> Money {
        Money::new(pounds * 100, Currency::GBP)
    }

    fn uk() -> Vec<CountryTie> {
        vec![CountryTie {
            code: "GB".into(),
            residence: true,
            tax_home: true,
            citizen: false,
            holds_money: true,
            ties: String::new(),
        }]
    }

    /// A UK saver who has answered everything and needs no step.
    fn complete() -> Profile {
        Profile {
            take_home: Some(gbp(2500)),
            costs: Some(gbp(1500)),
            cash: Some(gbp(6000)),
            debts: Some(vec![]),
            pension_match: Some(PensionMatch::Yes),
            isa_used: Some(IsaUsed {
                tax_year: TaxYear(2026),
                amount: gbp(20_000),
            }),
            countries: uk(),
            goals: vec![Goal {
                kind: GoalKind::BuyHome,
                target: None,
                target_date: None,
            }],
            risk: Some(RiskProfile {
                attitude: 3,
                capacity: Capacity::Medium,
                combined: 3,
                label: "Balanced",
                summary: String::new(),
            }),
        }
    }

    fn ids(steps: &[Step]) -> Vec<&str> {
        steps.iter().map(|s| s.id.as_str()).collect()
    }

    const DAY: Date = date(2026, 10, 30);

    fn run(p: &Profile, day: Date) -> Vec<Step> {
        next_steps(p, day, &Catalogue::builtin(), &[])
    }

    #[test]
    fn a_complete_and_comfortable_profile_needs_no_step() {
        assert_eq!(ids(&run(&complete(), DAY)), Vec::<&str>::new());
    }

    #[test]
    fn cushion_under_three_months_of_costs() {
        let mut p = complete();
        p.cash = Some(gbp(2250));
        let steps = run(&p, DAY);
        assert_eq!(ids(&steps), ["cushion"]);
        let s = &steps[0];
        assert!(s.why.contains("about 1.5 months"), "{}", s.why);
        assert!(s.why.contains("2,250.00 GBP more"), "{}", s.why);
        assert_eq!((s.risk, s.effort), (1, Effort::Ongoing));
        assert!(s.source.starts_with("https://www.moneyhelper.org.uk/"));
        assert_eq!(s.as_of, date(2026, 10, 30));
        // Exactly three months is enough.
        p.cash = Some(gbp(4500));
        assert!(run(&p, DAY).is_empty());
        // Costs of nothing need no cushion; another currency cannot be compared.
        p.cash = Some(gbp(10));
        p.costs = Some(gbp(0));
        assert!(run(&p, DAY).is_empty());
        p.costs = Some(Money::new(150000, Currency::INR));
        assert_eq!(ids(&run(&p, DAY)), ["ask:money.cashSavings"]);
        assert!(run(&p, DAY)[0].title.contains("in INR"));
    }

    #[test]
    fn high_interest_debt_names_the_costly_ones() {
        let mut p = complete();
        let debt = |label: &str, band| Debt {
            label: label.into(),
            balance: gbp(1000),
            band,
        };
        p.debts = Some(vec![
            debt("Mortgage", RateBand::Under5),
            debt("Car loan", RateBand::From5To10),
        ]);
        assert!(run(&p, DAY).is_empty(), "cheap debt is not a step");
        p.debts = Some(vec![
            debt("Store card", RateBand::From10To20),
            debt("Credit card", RateBand::Over20),
            debt("Mortgage", RateBand::Under5),
        ]);
        let steps = run(&p, DAY);
        assert_eq!(ids(&steps), ["high-interest-debt"]);
        assert!(
            steps[0].why.starts_with("Store card, Credit card cost 10%"),
            "{}",
            steps[0].why
        );
        assert!(steps[0].why.contains("Credit card at over 20% a year"));
        // Paid off: no step.
        p.debts = Some(vec![Debt {
            balance: gbp(0),
            ..debt("Credit card", RateBand::Over20)
        }]);
        assert!(run(&p, DAY).is_empty());
        // A rate nobody knows is asked about.
        p.debts = Some(vec![debt("Overdraft", RateBand::Unknown)]);
        let steps = run(&p, DAY);
        assert_eq!(ids(&steps), ["ask:money.debts"]);
        assert!(steps[0].title.contains("your Overdraft"));
    }

    #[test]
    fn isa_allowance_left_before_5_april() {
        let mut p = complete();
        p.isa_used = Some(IsaUsed {
            tax_year: TaxYear(2026),
            amount: gbp(8000),
        });
        let steps = run(&p, DAY);
        assert_eq!(ids(&steps), ["isa-allowance-2026"]);
        let s = &steps[0];
        assert_eq!(s.title, "Use your ISA allowance before 5 April 2027");
        assert!(s.why.contains("2026/27"));
        assert!(s.why.contains("up to 20,000.00 GBP"));
        assert!(s.why.contains("about 12,000.00 GBP left"));
        assert!(
            s.why
                .contains("From 6 April 2027, if you are under 65, at most 12,000.00 GBP"),
            "{}",
            s.why
        );
        assert_eq!((s.risk, s.effort), (2, Effort::Hours));
        assert_eq!(s.source, "https://www.gov.uk/individual-savings-accounts");
        // Not a UK resident with a UK tax home: no ISA step.
        p.countries[0].residence = false;
        assert!(run(&p, DAY).is_empty());
    }

    #[test]
    fn tax_year_boundary_on_5_and_6_april() {
        let mut p = complete();
        p.isa_used = Some(IsaUsed {
            tax_year: TaxYear(2026),
            amount: gbp(15_000),
        });
        // 5 April 2027 is still 2026/27, and its last day.
        let last = run(&p, date(2027, 4, 5));
        assert_eq!(ids(&last), ["isa-allowance-2026"]);
        assert!(last[0].why.contains("Today is the last day."));
        assert!(last[0].why.contains("5,000.00 GBP left"));
        // 6 April 2027: a new tax year. Last year's payments do not count, so
        // the full allowance shows and the app asks what was paid in.
        let first = run(&p, date(2027, 4, 6));
        assert_eq!(ids(&first), ["isa-allowance-2027", "ask:isa.used"]);
        assert_eq!(first[0].title, "Use your ISA allowance before 5 April 2028");
        assert!(first[0].why.contains("2027/28"));
        assert!(
            first[0]
                .why
                .contains("under 65, at most 12,000.00 GBP of it")
        );
        assert!(first[1].title.contains("since 6 April 2027"));
        // Ten days before the end, the countdown shows.
        let near = run(&p, date(2027, 3, 26));
        assert!(near[0].why.contains("10 days to go."), "{}", near[0].why);
        // A tax year the catalogue does not know gives no ISA step at all.
        p.isa_used = None;
        let unknown = run(&p, date(2028, 4, 6));
        assert!(
            !ids(&unknown).iter().any(|i| i.starts_with("isa-")),
            "{:?}",
            ids(&unknown)
        );
    }

    #[test]
    fn pension_match_unknown_is_asked() {
        let mut p = complete();
        p.pension_match = None;
        let steps = run(&p, DAY);
        assert_eq!(ids(&steps), ["ask:pension.employerMatch"]);
        assert_eq!(
            steps[0].ask,
            Some(Ask {
                fact: profile::PENSION_MATCH.into()
            })
        );
        p.pension_match = Some(PensionMatch::Unsure);
        assert_eq!(ids(&run(&p, DAY)), ["ask:pension.employerMatch"]);
        p.pension_match = Some(PensionMatch::NotEmployed);
        assert!(run(&p, DAY).is_empty());
        // Outside the UK the UK workplace rules do not apply.
        p.pension_match = None;
        p.countries = vec![CountryTie {
            code: "AE".into(),
            residence: true,
            tax_home: true,
            citizen: false,
            holds_money: true,
            ties: String::new(),
        }];
        assert!(run(&p, DAY).is_empty());
    }

    #[test]
    fn a_new_profile_gets_one_question_at_a_time() {
        let p = Profile::default();
        let steps = run(&p, DAY);
        assert_eq!(ids(&steps), ["ask:money.costsMonthly"]);
        assert_eq!(steps[0].effort, Effort::Minutes);
        // "Later" on that question brings up the next one.
        let later = |id: &str| Dismissal {
            step_id: id.into(),
            kind: DismissKind::Later,
            reason: None,
            on: DAY,
        };
        let cat = Catalogue::builtin();
        let next = next_steps(&p, DAY, &cat, &[later("ask:money.costsMonthly")]);
        assert_eq!(ids(&next), ["ask:money.cashSavings"]);
        // The order the questions come in.
        let mut asked = vec![];
        let mut put_off = vec![];
        loop {
            let s = next_steps(&p, DAY, &cat, &put_off);
            let Some(q) = s.last() else { break };
            asked.push(q.id.clone());
            put_off.push(later(&q.id));
        }
        assert_eq!(
            asked,
            [
                "ask:money.costsMonthly",
                "ask:money.cashSavings",
                "ask:money.debts",
                "ask:money.takeHomeMonthly",
                "ask:countries",
                "ask:goals",
                "ask:risk",
            ]
        );
        // Every step carries a source and a date.
        for s in missing(&p, DAY, &cat) {
            assert!(s.source.starts_with("https://"), "{}", s.id);
            assert!(!s.why.is_empty(), "{}", s.id);
        }
    }

    #[test]
    fn rules_come_in_the_plans_order() {
        let mut p = complete();
        p.cash = Some(gbp(100));
        p.debts = Some(vec![Debt {
            label: "Card".into(),
            balance: gbp(900),
            band: RateBand::Over20,
        }]);
        p.isa_used = None;
        p.pension_match = None;
        p.goals.clear();
        assert_eq!(
            ids(&run(&p, DAY)),
            [
                "cushion",
                "high-interest-debt",
                "isa-allowance-2026",
                "ask:pension.employerMatch",
                "ask:isa.used",
            ]
        );
    }

    #[test]
    fn later_hides_for_14_days_and_not_for_me_for_good() {
        let mut p = complete();
        p.cash = Some(gbp(100));
        let cat = Catalogue::builtin();
        let later = Dismissal {
            step_id: "cushion".into(),
            kind: DismissKind::Later,
            reason: None,
            on: DAY,
        };
        let shown = |day: Date, d: &[Dismissal]| ids(&next_steps(&p, day, &cat, d)).len();
        assert_eq!(shown(DAY, std::slice::from_ref(&later)), 0);
        assert_eq!(shown(date(2026, 11, 12), std::slice::from_ref(&later)), 0);
        assert_eq!(shown(date(2026, 11, 13), std::slice::from_ref(&later)), 1);
        let never = Dismissal {
            kind: DismissKind::NotForMe,
            ..later
        };
        assert_eq!(shown(date(2030, 1, 1), &[never]), 0);
    }

    #[test]
    fn dismissals_are_stored_with_reasons_and_can_be_taken_back() {
        let s = Store::memory().unwrap();
        let now = at(NOW);
        s.set_fact(
            profile::COSTS,
            &serde_json::to_value(gbp(1000)).unwrap(),
            profile::Source::Wizard,
            now,
        )
        .unwrap();
        s.set_fact(
            profile::CASH,
            &serde_json::to_value(gbp(100)).unwrap(),
            profile::Source::Wizard,
            now,
        )
        .unwrap();
        let cat = Catalogue::builtin();
        assert_eq!(s.next_steps(now, &cat).unwrap()[0].id, "cushion");
        s.step_not_for_me("cushion", "  I keep cash with family  ", now)
            .unwrap();
        let d = s.dismissals().unwrap();
        assert_eq!(d[0].reason.as_deref(), Some("I keep cash with family"));
        assert_eq!(
            (d[0].kind, d[0].on),
            (DismissKind::NotForMe, date(2026, 11, 4))
        );
        assert_ne!(s.next_steps(now, &cat).unwrap()[0].id, "cushion");
        s.step_restore("cushion").unwrap();
        s.step_later("cushion", now).unwrap();
        assert_eq!(s.dismissals().unwrap()[0].kind, DismissKind::Later);
        assert!(s.step_not_for_me("x", &"y".repeat(501), now).is_err());
        assert!(s.step_later("", now).is_err());
        // Dates are judged in UTC: late evening west of Greenwich is already tomorrow.
        assert_eq!(today(at("2026-04-05T23:30:00-05:00")), date(2026, 4, 6));
    }
}
