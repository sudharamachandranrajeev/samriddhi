//! The AI digest: the only view of the user's money that may ever leave the
//! device for an AI model, and only after the user has seen it and pressed
//! send. Rounded, derived and nameless: no account or holding names, no
//! ids, no exact amounts, no goal names (goals become "Goal 1", "Goal 2"...),
//! no dates. Ported from the samridhi prototype (`lib/ai/digest.dart`) with
//! the same rounding: totals and targets to two significant figures,
//! weights to whole percent (targets to the nearest 5%).

use jiff::Timestamp;
use serde_json::{Map, Value, json};

use crate::agents::base_currency;
use crate::assets::{ALL, Assumptions};
use crate::money::Money;
use crate::plan::{PlanResult, plan_for_profile};
use crate::risk::RiskProfile;
use crate::steps::today;
use crate::store::{Result, Store};
use crate::wealth::Totals;

/// What the narrator is told before it sees the digest. Its answer is shown
/// as text only; it can never make or trigger a step.
pub const NARRATOR_SYSTEM_PROMPT: &str = "You explain an investment plan in plain English to a \
     non-expert. Use only the figures in the JSON the user sends; never invent numbers. Do not \
     recommend specific securities, funds or products and do not give personalised financial \
     advice. Keep it under 180 words and end with one sentence saying this is an illustrative \
     model, not advice.";

/// Round to `sig` significant figures (123456 -> 120000 at two).
pub fn round_sig(v: f64, sig: i32) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return 0.0;
    }
    let scale = 10_f64.powi(sig - 1 - v.abs().log10().floor() as i32);
    (v * scale).round() / scale
}

/// A weight to the nearest 5%, as a whole percent.
fn to_5pct(w: f64) -> i64 {
    ((w * 20.0).round() * 5.0) as i64
}

fn whole_pct(w: f64) -> i64 {
    (w * 100.0).round() as i64
}

/// An amount in major units at two significant figures.
fn approx(m: Money) -> Value {
    let major = m.minor as f64 / 10_f64.powi(i32::from(m.currency.exponent()));
    let r = round_sig(major, 2);
    if r.fract() == 0.0 && r.abs() < 9.0e15 {
        json!(r as i64)
    } else {
        json!(r)
    }
}

fn by_class(f: impl Fn(crate::assets::AssetClass) -> i64) -> Value {
    let mut m = Map::new();
    for c in ALL {
        m.insert(c.label().into(), json!(f(c)));
    }
    Value::Object(m)
}

/// The digest of a plan and the holdings counted in it. Holdings in other
/// currencies (not counted until exchange rates arrive) are left out.
pub fn build_digest(
    risk: Option<&RiskProfile>,
    plan: &PlanResult,
    totals: Option<&Totals>,
) -> Value {
    let invested = totals.filter(|t| t.total.minor > 0);
    let goals: Vec<Value> = plan
        .per_goal
        .iter()
        .enumerate()
        .map(|(i, g)| {
            json!({
                "name": format!("Goal {}", i + 1),
                "horizon_years": g.goal.horizon_years.round() as i64,
                "horizon_assumed": g.goal.horizon_assumed,
                "target_approx": g.goal.target.map(approx),
                "target_currency": g.goal.target.map(|m| m.currency.code().to_owned()),
                "growth_assets_pct": to_5pct(g.allocation.growth_share()),
            })
        })
        .collect();
    json!({
        "risk_profile": risk.map(|r| r.label),
        "risk_score_rounded_to_10": risk.map(|r| {
            (crate::plan::risk_score_for_level(r.combined) / 10.0).round() as i64 * 10
        }),
        "tier_mode": plan.tier,
        "currency": invested.map(|t| t.base.code().to_owned()),
        "total_invested_approx": invested.map(|t| approx(t.total)).unwrap_or(json!(0)),
        "target_allocation_pct": by_class(|c| to_5pct(plan.blended.get(&c).copied().unwrap_or(0.0))),
        "current_allocation_pct": invested
            .map(|t| by_class(|c| whole_pct(t.weights.get(&c).copied().unwrap_or(0.0)))),
        "goals": goals,
        "note": "Figures are rounded model outputs using illustrative assumptions.",
    })
}

/// The digest as the exact text that would be sent.
pub fn digest_text(digest: &Value) -> String {
    serde_json::to_string_pretty(digest).unwrap_or_default()
}

impl Store {
    /// The digest of what is stored, or `None` until the risk questions are
    /// answered (there is no plan to explain before that).
    pub fn digest(&self, now: Timestamp) -> Result<Option<Value>> {
        let p = self.profile()?;
        let Some(plan) = plan_for_profile(&Assumptions::builtin(), &p, today(now), None) else {
            return Ok(None);
        };
        let totals = self.totals(base_currency(&p))?;
        Ok(Some(build_digest(p.risk.as_ref(), &plan, Some(&totals))))
    }
}

/// Ported from the prototype's `test/agents/agents_test.dart` ("digest
/// privacy" group and the narrator-sees-only-the-digest test).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{AssetClass, Assumptions};
    use crate::money::Currency;
    use crate::plan::{GoalInput, build_plan, risk_score_for_level};
    use crate::risk::{self, Capacity};
    use crate::wealth::{self, Holding};

    fn digest(equity: i64, bonds: i64) -> String {
        let r = RiskProfile {
            attitude: 2,
            capacity: Capacity::High,
            combined: 2,
            label: risk::label(2),
            summary: "Secret summary".into(),
        };
        let goal = GoalInput::new(
            "Secret house fund",
            Some(Money::new(12_345_600, Currency::GBP)),
            12.0,
        );
        let plan = build_plan(
            &Assumptions::builtin(),
            risk_score_for_level(2),
            &[goal],
            None,
        );
        let h = |id: &str, name: &str, c: AssetClass, minor: i64, cur: Currency| Holding {
            id: id.into(),
            account_id: "acct-0193f1d2-secret".into(),
            name: name.into(),
            asset_class: c,
            amount: Money::new(minor, cur),
        };
        let holdings = vec![
            h(
                "hold-1a2b",
                "Secret Bank ISA world fund",
                AssetClass::Equity,
                equity,
                Currency::GBP,
            ),
            h(
                "hold-3c4d",
                "Secret gilt ladder",
                AssetClass::Bonds,
                bonds,
                Currency::GBP,
            ),
            h(
                "hold-5e6f",
                "Secret rupee deposit",
                AssetClass::Cash,
                987_654_321,
                Currency::INR,
            ),
        ];
        let t = wealth::totals(&holdings, Currency::GBP).unwrap();
        digest_text(&build_digest(Some(&r), &plan, Some(&t)))
    }

    #[test]
    fn rounded_anonymous_no_names_ids_or_exact_amounts() {
        // 123,456.78 and 78,901.23 pounds.
        let text = digest(12_345_678, 7_890_123);
        for secret in [
            "Secret", "house", "hold-", "acct-", "0193f1d2", "123456", "12345678", "78901",
            "7890123", "987654", "9876543", "summary",
        ] {
            assert!(!text.contains(secret), "{secret} leaked:\n{text}");
        }
        assert!(text.contains("\"Goal 1\""));
        // 202,357 pounds to two significant figures.
        assert!(text.contains("\"total_invested_approx\": 200000"), "{text}");
        // The goal's 123,456 pounds becomes 120,000.
        assert!(text.contains("\"target_approx\": 120000"), "{text}");
        assert!(text.contains("\"risk_profile\": \"Cautious\""));
        // Level 2 is score 25, rounded to 30.
        assert!(text.contains("\"risk_score_rounded_to_10\": 30"));
        // The rupee holding is not counted, and its currency does not show.
        assert!(!text.contains("INR"));
    }

    #[test]
    fn weights_are_whole_percent_and_targets_steps_of_five() {
        let v: Value = serde_json::from_str(&digest(6_000_000, 4_000_000)).unwrap();
        let cur = &v["current_allocation_pct"];
        assert_eq!(cur["Equities"], 60);
        assert_eq!(cur["Bonds"], 40);
        let target = v["target_allocation_pct"].as_object().unwrap();
        assert_eq!(target.len(), 5);
        for (k, pct) in target {
            assert_eq!(pct.as_i64().unwrap() % 5, 0, "{k}");
        }
        assert_eq!(v["goals"][0]["growth_assets_pct"].as_i64().unwrap() % 5, 0);
    }

    #[test]
    fn nothing_invested_gives_no_current_mix() {
        let v: Value = serde_json::from_str(&digest(0, 0)).unwrap();
        assert_eq!(v["total_invested_approx"], 0);
        assert!(v["current_allocation_pct"].is_null());
        assert!(v["currency"].is_null());
    }

    #[test]
    fn the_stored_digest_needs_a_plan_and_never_names_accounts() {
        use crate::risk::{Answer, QUESTIONS};
        use crate::store::tests::{NOW, at};
        use crate::wealth::Wrapper;
        let mut s = Store::memory().unwrap();
        assert_eq!(s.digest(at(NOW)).unwrap(), None);
        let answers: Vec<Answer> = QUESTIONS
            .iter()
            .map(|q| Answer {
                question_id: q.id.into(),
                option_id: q.options[0].id.into(),
            })
            .collect();
        s.save_risk_answers(&answers, at(NOW)).unwrap();
        let acc = s.add_account("Secret Bank", Wrapper::Isa, at(NOW)).unwrap();
        let h = s
            .add_holding(
                &acc.id,
                "Secret fund",
                AssetClass::Equity,
                Money::new(4_321_987, Currency::GBP),
                at(NOW),
            )
            .unwrap();
        let text = digest_text(&s.digest(at(NOW)).unwrap().unwrap());
        for secret in ["Secret", acc.id.as_str(), h.id.as_str(), "43219", "4321987"] {
            assert!(!text.contains(secret), "{secret} leaked:\n{text}");
        }
        assert!(text.contains("\"total_invested_approx\": 43000"), "{text}");
    }

    #[test]
    fn round_sig_matches_the_prototype() {
        assert_eq!(round_sig(123456.0, 2), 120000.0);
        assert_eq!(round_sig(0.0, 2), 0.0);
        assert_eq!(round_sig(987.0, 2), 990.0);
        assert_eq!(round_sig(-4567.0, 2), -4600.0);
        assert_eq!(round_sig(f64::NAN, 2), 0.0);
    }
}
