//! What the user has told Samriddhi about themselves: the wizard's answers,
//! and later the one-at-a-time questions. Each fact is kept with where it
//! came from (`source`) and why it was asked (`why`), so every screen can
//! say "you told us this, because...". Rough figures are fine: these are
//! monthly estimates, not a ledger.

use jiff::Timestamp;
use jiff::civil::Date;
use rusqlite::{OptionalExtension, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalogue::TaxYear;
use crate::money::{Currency, Money};
use crate::risk::RiskProfile;
use crate::store::{Error, Result, Store, iso};

/// Where a fact came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// Given in the first-run wizard.
    Wizard,
    /// Asked later, one question at a time.
    Asked,
    /// Worked out by the app from other facts.
    Derived,
}

impl Source {
    fn as_str(&self) -> &'static str {
        match self {
            Source::Wizard => "wizard",
            Source::Asked => "asked",
            Source::Derived => "derived",
        }
    }

    fn parse(s: &str) -> Result<Source> {
        match s {
            "wizard" => Ok(Source::Wizard),
            "asked" => Ok(Source::Asked),
            "derived" => Ok(Source::Derived),
            _ => Err(Error::Invalid(format!("odd source {s:?}"))),
        }
    }
}

pub const TAKE_HOME: &str = "money.takeHomeMonthly";
pub const COSTS: &str = "money.costsMonthly";
pub const CASH: &str = "money.cashSavings";
pub const DEBTS: &str = "money.debts";
pub const PENSION_MATCH: &str = "pension.employerMatch";
pub const ISA_USED: &str = "isa.used";

/// Every fact the profile may hold, with the reason given when it is asked.
pub const FACTS: &[(&str, &str)] = &[
    (
        TAKE_HOME,
        "With your costs, it shows how much you could put aside each month.",
    ),
    (
        COSTS,
        "Your costs set the size of the safety cushion you need.",
    ),
    (
        CASH,
        "Cash you can reach quickly is your cushion if income stops or a big bill arrives.",
    ),
    (
        DEBTS,
        "Expensive debt usually costs more than savings earn, so it changes what comes first.",
    ),
    (
        PENSION_MATCH,
        "Some employers pay more into your pension when you do; missing that is missing pay.",
    ),
    (
        ISA_USED,
        "The ISA allowance resets each April, so what you have used decides what is left.",
    ),
];

/// The reason a fact is asked.
pub fn why(key: &str) -> Option<&'static str> {
    FACTS.iter().find(|(k, _)| *k == key).map(|(_, w)| *w)
}

/// A rough yearly interest rate, as people know it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RateBand {
    Under5,
    From5To10,
    From10To20,
    Over20,
    Unknown,
}

impl RateBand {
    /// The lowest yearly rate in the band, in percent.
    pub fn floor_percent(&self) -> Option<u8> {
        match self {
            RateBand::Under5 => Some(0),
            RateBand::From5To10 => Some(5),
            RateBand::From10To20 => Some(10),
            RateBand::Over20 => Some(20),
            RateBand::Unknown => None,
        }
    }

    pub fn words(&self) -> &'static str {
        match self {
            RateBand::Under5 => "under 5% a year",
            RateBand::From5To10 => "5% to 10% a year",
            RateBand::From10To20 => "10% to 20% a year",
            RateBand::Over20 => "over 20% a year",
            RateBand::Unknown => "an unknown rate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Debt {
    /// The user's own name for it ("credit card").
    pub label: String,
    pub balance: Money,
    pub band: RateBand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PensionMatch {
    /// The employer adds more when the user pays in more.
    Yes,
    No,
    NotEmployed,
    Unsure,
}

/// ISA money paid in so far in one tax year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IsaUsed {
    pub tax_year: TaxYear,
    pub amount: Money,
}

/// How the user is tied to one country (ISO 3166 two-letter code).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryTie {
    pub code: String,
    pub residence: bool,
    pub tax_home: bool,
    pub citizen: bool,
    pub holds_money: bool,
    /// Anything else, in the user's words.
    #[serde(default)]
    pub ties: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoalKind {
    EmergencyCushion,
    ClearDebt,
    BuyHome,
    RetireEarly,
    ExtraIncome,
    FamilySupportAbroad,
    LearnInvesting,
}

impl GoalKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoalKind::EmergencyCushion => "emergencyCushion",
            GoalKind::ClearDebt => "clearDebt",
            GoalKind::BuyHome => "buyHome",
            GoalKind::RetireEarly => "retireEarly",
            GoalKind::ExtraIncome => "extraIncome",
            GoalKind::FamilySupportAbroad => "familySupportAbroad",
            GoalKind::LearnInvesting => "learnInvesting",
        }
    }
}

/// The wizard lets the user pick up to this many goals.
pub const MAX_GOALS: usize = 3;
const COUNTRIES_WHY: &str =
    "Tax rules, accounts and allowances depend on the country; we only suggest what fits yours.";
const GOALS_WHY: &str = "Steps are ordered by what matters to you.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub kind: GoalKind,
    #[serde(default)]
    pub target: Option<Money>,
    #[serde(default)]
    pub target_date: Option<Date>,
}

/// One stored fact, as the "why do you know this?" view shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub key: String,
    pub value: Value,
    pub source: Source,
    pub asked_at: String,
    pub why: String,
}

/// Everything the rules look at, read once. A missing fact is `None`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub take_home: Option<Money>,
    pub costs: Option<Money>,
    pub cash: Option<Money>,
    /// `Some(vec![])` means "no debts"; `None` means not asked yet.
    pub debts: Option<Vec<Debt>>,
    pub pension_match: Option<PensionMatch>,
    pub isa_used: Option<IsaUsed>,
    pub countries: Vec<CountryTie>,
    pub goals: Vec<Goal>,
    pub risk: Option<RiskProfile>,
}

impl Profile {
    pub fn tax_home(&self) -> Option<&str> {
        self.countries
            .iter()
            .find(|c| c.tax_home)
            .map(|c| c.code.as_str())
    }

    pub fn lives_in(&self, code: &str) -> bool {
        self.countries.iter().any(|c| c.residence && c.code == code)
    }

    /// Cash minus debts, when both are known and in one currency: the rough
    /// net worth the home screen shows ("rough, from your answers").
    pub fn rough_net_worth(&self) -> Option<Money> {
        let cash = self.cash?;
        let debts: Vec<Money> = self.debts.as_ref()?.iter().map(|d| d.balance).collect();
        cash.checked_sub(Money::sum(cash.currency, &debts).ok()?)
            .ok()
    }
}

/// Check a fact's value has the right shape for its key, and is sensible.
pub(crate) fn check(key: &str, value: &Value) -> Result<()> {
    fn parse<T: DeserializeOwned>(v: &Value) -> Result<T> {
        Ok(serde_json::from_value(v.clone())?)
    }
    let not_negative = |m: &Money, what: &str| {
        if m.is_negative() {
            Err(Error::Invalid(format!("{what} cannot be below zero")))
        } else {
            Ok(())
        }
    };
    match key {
        TAKE_HOME | COSTS | CASH => not_negative(&parse::<Money>(value)?, "an amount"),
        DEBTS => {
            for d in parse::<Vec<Debt>>(value)? {
                not_negative(&d.balance, "a debt")?;
                if d.label.trim().is_empty() || d.label.chars().count() > 80 {
                    return Err(Error::Invalid(
                        "a debt needs a short name (up to 80 letters)".into(),
                    ));
                }
            }
            Ok(())
        }
        PENSION_MATCH => parse::<PensionMatch>(value).map(drop),
        ISA_USED => {
            let used = parse::<IsaUsed>(value)?;
            not_negative(&used.amount, "ISA payments")?;
            if used.amount.currency != Currency::GBP {
                return Err(Error::Invalid("ISA payments are in pounds (GBP)".into()));
            }
            Ok(())
        }
        _ => Err(Error::Invalid(format!("{key} is not a profile fact"))),
    }
}

fn valid_code(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|b| b.is_ascii_uppercase())
}

impl Store {
    /// Keep a fact, with its reason from [`FACTS`]. Replaces an earlier one.
    pub fn set_fact(&self, key: &str, value: &Value, source: Source, now: Timestamp) -> Result<()> {
        check(key, value)?;
        let why = why(key).ok_or_else(|| Error::Invalid(format!("{key} is not a profile fact")))?;
        self.conn.execute(
            "INSERT INTO sam_profile (key, json, source, asked_at, why) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(key) DO UPDATE SET json = ?2, source = ?3, asked_at = ?4, why = ?5",
            params![
                key,
                serde_json::to_string(value)?,
                source.as_str(),
                iso(now),
                why
            ],
        )?;
        Ok(())
    }

    /// Forget a fact; the app will ask again when it needs it.
    pub fn clear_fact(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM sam_profile WHERE key = ?", [key])?;
        Ok(())
    }

    fn fact<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let j: Option<String> = self
            .conn
            .query_row("SELECT json FROM sam_profile WHERE key = ?", [key], |r| {
                r.get(0)
            })
            .optional()?;
        j.map(|j| Ok(serde_json::from_str(&j)?)).transpose()
    }

    /// Every stored fact with its source and reason, by key.
    pub fn facts(&self) -> Result<Vec<Fact>> {
        let mut st = self
            .conn
            .prepare("SELECT key, json, source, asked_at, why FROM sam_profile ORDER BY key")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (key, json, source, asked_at, why) = row?;
            Ok(Fact {
                key,
                value: serde_json::from_str(&json)?,
                source: Source::parse(&source)?,
                asked_at,
                why,
            })
        })
        .collect()
    }

    /// Replace the country list. At most one tax home; codes are ISO 3166
    /// two-letter codes in capitals ("GB", "IN", "AE").
    pub fn set_countries(
        &mut self,
        list: &[CountryTie],
        source: Source,
        now: Timestamp,
    ) -> Result<()> {
        if list.iter().filter(|c| c.tax_home).count() > 1 {
            return Err(Error::Invalid(
                "only one country can be your tax home".into(),
            ));
        }
        if let Some(c) = list.iter().find(|c| !valid_code(&c.code)) {
            return Err(Error::Invalid(format!(
                "{:?} is not a country code",
                c.code
            )));
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM sam_countries", [])?;
        for c in list {
            tx.execute(
                "INSERT INTO sam_countries (code, residence, tax_home, citizen, holds_money, ties, source, why, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    c.code, c.residence, c.tax_home, c.citizen, c.holds_money, c.ties.trim(),
                    source.as_str(), COUNTRIES_WHY, iso(now)
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn countries(&self) -> Result<Vec<CountryTie>> {
        let mut st = self.conn.prepare(
            "SELECT code, residence, tax_home, citizen, holds_money, ties FROM sam_countries
             ORDER BY tax_home DESC, residence DESC, code",
        )?;
        let rows = st.query_map([], |r| {
            Ok(CountryTie {
                code: r.get(0)?,
                residence: r.get(1)?,
                tax_home: r.get(2)?,
                citizen: r.get(3)?,
                holds_money: r.get(4)?,
                ties: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Replace the goals, most important first; up to [`MAX_GOALS`], each
    /// kind once, targets not below zero.
    pub fn set_goals(&mut self, goals: &[Goal], source: Source, now: Timestamp) -> Result<()> {
        if goals.len() > MAX_GOALS {
            return Err(Error::Invalid(format!("pick up to {MAX_GOALS} goals")));
        }
        for (i, g) in goals.iter().enumerate() {
            if goals[..i].iter().any(|h| h.kind == g.kind) {
                return Err(Error::Invalid("each goal once".into()));
            }
            if g.target.is_some_and(|m| m.is_negative()) {
                return Err(Error::Invalid("a target cannot be below zero".into()));
            }
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM sam_goals", [])?;
        for (i, g) in goals.iter().enumerate() {
            tx.execute(
                "INSERT INTO sam_goals (id, kind, rank, target_minor, target_currency, target_date, source, why, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    uuid::Uuid::now_v7().to_string(),
                    g.kind.as_str(),
                    i as i64 + 1,
                    g.target.map(|m| m.minor),
                    g.target.map(|m| m.currency.code().to_owned()),
                    g.target_date.map(|d| d.to_string()),
                    source.as_str(),
                    GOALS_WHY,
                    iso(now)
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn goals(&self) -> Result<Vec<Goal>> {
        let mut st = self.conn.prepare(
            "SELECT kind, target_minor, target_currency, target_date FROM sam_goals ORDER BY rank",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (kind, minor, currency, date) = row?;
            let target = match (minor, currency) {
                (Some(m), Some(c)) => Some(Money::new(m, Currency::new(&c)?)),
                _ => None,
            };
            let target_date = date
                .map(|d| d.parse::<Date>())
                .transpose()
                .map_err(|e| Error::Invalid(format!("odd goal date: {e}")))?;
            Ok(Goal {
                kind: serde_json::from_value(Value::String(kind))?,
                target,
                target_date,
            })
        })
        .collect()
    }

    /// Everything the rules need, in one read.
    pub fn profile(&self) -> Result<Profile> {
        Ok(Profile {
            take_home: self.fact(TAKE_HOME)?,
            costs: self.fact(COSTS)?,
            cash: self.fact(CASH)?,
            debts: self.fact(DEBTS)?,
            pension_match: self.fact(PENSION_MATCH)?,
            isa_used: self.fact(ISA_USED)?,
            countries: self.countries()?,
            goals: self.goals()?,
            risk: self.risk_profile()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::{NOW, at};
    use jiff::civil::date;
    use serde_json::json;

    fn gbp(minor: i64) -> Value {
        json!({ "minor": minor, "currency": "GBP" })
    }

    fn tie(code: &str, tax_home: bool) -> CountryTie {
        CountryTie {
            code: code.into(),
            residence: tax_home,
            tax_home,
            citizen: false,
            holds_money: true,
            ties: String::new(),
        }
    }

    #[test]
    fn facts_keep_their_source_and_reason() {
        let s = Store::memory().unwrap();
        s.set_fact(COSTS, &gbp(180000), Source::Wizard, at(NOW))
            .unwrap();
        s.set_fact(CASH, &gbp(250000), Source::Asked, at(NOW))
            .unwrap();
        s.set_fact(CASH, &gbp(260000), Source::Asked, at(NOW))
            .unwrap();
        let facts = s.facts().unwrap();
        assert_eq!(facts.len(), 2);
        let cash = facts.iter().find(|f| f.key == CASH).unwrap();
        assert_eq!(cash.value, gbp(260000));
        assert_eq!(cash.source, Source::Asked);
        assert_eq!(cash.asked_at, "2026-11-04T09:00:00Z");
        assert_eq!(cash.why, why(CASH).unwrap());
        let p = s.profile().unwrap();
        assert_eq!(p.costs, Some(Money::new(180000, Currency::GBP)));
        assert_eq!(p.take_home, None);
        s.clear_fact(CASH).unwrap();
        assert_eq!(s.profile().unwrap().cash, None);
    }

    #[test]
    fn refuses_unknown_keys_bad_shapes_and_odd_values() {
        let s = Store::memory().unwrap();
        let now = at(NOW);
        assert!(
            s.set_fact("money.lottery", &gbp(1), Source::Wizard, now)
                .is_err()
        );
        assert!(s.set_fact(CASH, &json!(12.5), Source::Wizard, now).is_err());
        assert!(s.set_fact(CASH, &gbp(-1), Source::Wizard, now).is_err());
        assert!(
            s.set_fact(PENSION_MATCH, &json!("maybe"), Source::Wizard, now)
                .is_err()
        );
        assert!(
            s.set_fact(
                ISA_USED,
                &json!({ "taxYear": 2026, "amount": { "minor": 100, "currency": "INR" } }),
                Source::Asked,
                now
            )
            .is_err()
        );
        let unnamed = json!([{ "label": " ", "balance": gbp(100), "band": "over20" }]);
        assert!(s.set_fact(DEBTS, &unnamed, Source::Wizard, now).is_err());
        assert!(s.facts().unwrap().is_empty());
        // Every fact has its reason, in one sentence.
        for (key, why) in FACTS {
            assert!(why.ends_with('.') && why.len() < 120, "{key}");
        }
    }

    #[test]
    fn debts_pension_and_isa_read_back_typed() {
        let s = Store::memory().unwrap();
        let now = at(NOW);
        let debts = json!([
            { "label": "Card", "balance": gbp(120000), "band": "over20" },
            { "label": "Car loan", "balance": gbp(500000), "band": "from5To10" }
        ]);
        s.set_fact(DEBTS, &debts, Source::Wizard, now).unwrap();
        s.set_fact(PENSION_MATCH, &json!("unsure"), Source::Wizard, now)
            .unwrap();
        s.set_fact(
            ISA_USED,
            &json!({ "taxYear": 2026, "amount": gbp(500000) }),
            Source::Asked,
            now,
        )
        .unwrap();
        let p = s.profile().unwrap();
        let d = p.debts.unwrap();
        assert_eq!(d[0].band, RateBand::Over20);
        assert_eq!(d[1].band.floor_percent(), Some(5));
        assert_eq!(p.pension_match, Some(PensionMatch::Unsure));
        assert_eq!(p.isa_used.unwrap().tax_year, TaxYear(2026));
    }

    #[test]
    fn countries_have_one_tax_home_at_most() {
        let mut s = Store::memory().unwrap();
        let now = at(NOW);
        let mut india = tie("IN", false);
        india.citizen = true;
        s.set_countries(
            &[india, tie("GB", true), tie("AE", false)],
            Source::Wizard,
            now,
        )
        .unwrap();
        let list = s.countries().unwrap();
        assert_eq!(list[0].code, "GB", "tax home first");
        let p = s.profile().unwrap();
        assert_eq!(p.tax_home(), Some("GB"));
        assert!(p.lives_in("GB") && !p.lives_in("IN"));
        assert!(
            s.set_countries(&[tie("GB", true), tie("IN", true)], Source::Wizard, now)
                .is_err()
        );
        assert!(
            s.set_countries(&[tie("gb", false)], Source::Wizard, now)
                .is_err()
        );
        assert!(
            s.set_countries(&[tie("GBR", false)], Source::Wizard, now)
                .is_err()
        );
        assert_eq!(
            s.countries().unwrap().len(),
            3,
            "a refused list changes nothing"
        );
    }

    #[test]
    fn goals_keep_their_order_and_targets() {
        let mut s = Store::memory().unwrap();
        let now = at(NOW);
        let goals = vec![
            Goal {
                kind: GoalKind::ClearDebt,
                target: None,
                target_date: Some(date(2027, 6, 30)),
            },
            Goal {
                kind: GoalKind::BuyHome,
                target: Some(Money::new(3_000_000, Currency::GBP)),
                target_date: None,
            },
        ];
        s.set_goals(&goals, Source::Wizard, now).unwrap();
        assert_eq!(s.goals().unwrap(), goals);
        let four = vec![
            Goal {
                kind: GoalKind::BuyHome,
                target: None,
                target_date: None,
            },
            Goal {
                kind: GoalKind::RetireEarly,
                target: None,
                target_date: None,
            },
            Goal {
                kind: GoalKind::ExtraIncome,
                target: None,
                target_date: None,
            },
            Goal {
                kind: GoalKind::LearnInvesting,
                target: None,
                target_date: None,
            },
        ];
        assert!(s.set_goals(&four, Source::Wizard, now).is_err());
        assert!(
            s.set_goals(&[goals[0].clone(), goals[0].clone()], Source::Wizard, now)
                .is_err()
        );
        let negative = Goal {
            kind: GoalKind::BuyHome,
            target: Some(Money::new(-1, Currency::GBP)),
            target_date: None,
        };
        assert!(s.set_goals(&[negative], Source::Wizard, now).is_err());
        assert_eq!(s.goals().unwrap().len(), 2);
    }

    #[test]
    fn rough_net_worth_is_cash_minus_debts_in_one_currency() {
        let mut p = Profile {
            cash: Some(Money::new(500000, Currency::GBP)),
            ..Profile::default()
        };
        assert_eq!(p.rough_net_worth(), None, "debts not asked yet");
        p.debts = Some(vec![]);
        assert_eq!(p.rough_net_worth(), Some(Money::new(500000, Currency::GBP)));
        p.debts = Some(vec![Debt {
            label: "Card".into(),
            balance: Money::new(700000, Currency::GBP),
            band: RateBand::Over20,
        }]);
        assert_eq!(
            p.rough_net_worth(),
            Some(Money::new(-200000, Currency::GBP))
        );
        p.debts.as_mut().unwrap()[0].balance = Money::new(1, Currency::INR);
        assert_eq!(p.rough_net_worth(), None, "no exchange rates yet");
    }
}
