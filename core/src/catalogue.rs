//! The dated rules catalogue (`core/catalogue/rules.json`). Tax and country
//! rules change, so every rule carries the primary source it came from, the
//! day it was checked (`asOf`) and the day it must be checked again
//! (`reviewBy`, two months on). The steps engine reads amounts and lines from
//! here, never from constants in code.

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::money::Money;

const BUILTIN: &str = include_str!("../catalogue/rules.json");
/// How long a rule may go unchecked.
pub const REVIEW_DAYS: i32 = 62;

/// A UK tax year, named by the year it starts: 2026 is 6 April 2026 to
/// 5 April 2027 ("2026/27").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaxYear(pub i16);

impl TaxYear {
    /// The tax year a (UTC) day falls in: 5 April is the last day, 6 April
    /// the first day of the next.
    pub fn of(day: Date) -> TaxYear {
        if (day.month(), day.day()) >= (4, 6) {
            TaxYear(day.year())
        } else {
            TaxYear(day.year() - 1)
        }
    }

    pub fn first_day(&self) -> Date {
        Date::constant(self.0, 4, 6)
    }

    pub fn last_day(&self) -> Date {
        Date::constant(self.0 + 1, 4, 5)
    }

    /// "2026/27".
    pub fn label(&self) -> String {
        format!("{}/{:02}", self.0, (self.0 + 1).rem_euclid(100))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub as_of: Date,
    pub review_by: Date,
    /// The primary source: a public page anyone can check.
    pub source: String,
    pub note: String,
    #[serde(flatten)]
    pub kind: RuleKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuleKind {
    /// Keep at least this many months of costs as cash.
    Cushion { months: i64 },
    /// Debt at or above this yearly rate counts as high-interest.
    HighInterestDebt { min_rate_percent: u8 },
    /// The overall ISA allowance for one tax year.
    IsaAllowance { tax_year: TaxYear, amount: Money },
    /// The most a person under `under_age` may pay into cash ISAs each tax
    /// year, from the tax year starting `from`.
    CashIsaCap {
        from: Date,
        under_age: u8,
        amount: Money,
    },
    /// Employers may pay more into a workplace pension than the minimum.
    PensionMatch,
    /// Guidance a question or step leans on, with no numbers of its own.
    Guide,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub as_of: Date,
    pub review_by: Date,
    pub rules: Vec<Rule>,
}

impl Catalogue {
    /// The catalogue shipped with this version. Its tests prove it parses.
    pub fn builtin() -> Catalogue {
        Catalogue::parse(BUILTIN).expect("the built-in rules catalogue is valid")
    }

    pub fn parse(json: &str) -> serde_json::Result<Catalogue> {
        serde_json::from_str(json)
    }

    pub fn rule(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// True once any rule is past its review date: the screens say the
    /// facts may be out of date.
    pub fn needs_review(&self, today: Date) -> bool {
        today > self.review_by || self.rules.iter().any(|r| today > r.review_by)
    }

    pub fn cushion(&self) -> Option<(&Rule, i64)> {
        self.rules.iter().find_map(|r| match r.kind {
            RuleKind::Cushion { months } => Some((r, months)),
            _ => None,
        })
    }

    pub fn high_interest(&self) -> Option<(&Rule, u8)> {
        self.rules.iter().find_map(|r| match r.kind {
            RuleKind::HighInterestDebt { min_rate_percent } => Some((r, min_rate_percent)),
            _ => None,
        })
    }

    /// The ISA allowance for `year`, if the catalogue knows it. A year it does
    /// not know gives `None`, never a guess.
    pub fn isa_allowance(&self, year: TaxYear) -> Option<(&Rule, Money)> {
        self.rules.iter().find_map(|r| match r.kind {
            RuleKind::IsaAllowance { tax_year, amount } if tax_year == year => Some((r, amount)),
            _ => None,
        })
    }

    /// The cash ISA cap in force during `year`, if any.
    pub fn cash_isa_cap(&self, year: TaxYear) -> Option<(&Rule, Money, u8)> {
        self.rules.iter().find_map(|r| match r.kind {
            RuleKind::CashIsaCap {
                from,
                under_age,
                amount,
            } if from <= year.first_day() => Some((r, amount, under_age)),
            _ => None,
        })
    }

    pub fn pension_match(&self) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|r| matches!(r.kind, RuleKind::PensionMatch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;
    use jiff::civil::date;

    #[test]
    fn tax_years_turn_over_on_6_april() {
        assert_eq!(TaxYear::of(date(2027, 4, 5)), TaxYear(2026));
        assert_eq!(TaxYear::of(date(2027, 4, 6)), TaxYear(2027));
        assert_eq!(TaxYear::of(date(2026, 10, 30)), TaxYear(2026));
        assert_eq!(TaxYear::of(date(2027, 1, 1)), TaxYear(2026));
        assert_eq!(TaxYear::of(date(2026, 12, 31)), TaxYear(2026));
        assert_eq!(TaxYear(2026).first_day(), date(2026, 4, 6));
        assert_eq!(TaxYear(2026).last_day(), date(2027, 4, 5));
        assert_eq!(TaxYear(2026).label(), "2026/27");
        assert_eq!(TaxYear(2099).label(), "2099/00");
    }

    #[test]
    fn every_rule_is_dated_sourced_and_due_for_review_within_two_months() {
        let c = Catalogue::builtin();
        assert_eq!(c.as_of, date(2026, 10, 30));
        let mut ids: Vec<&str> = c.rules.iter().map(|r| r.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.rules.len(), "rule ids are unique");
        for r in &c.rules {
            assert!(r.source.starts_with("https://"), "{}", r.id);
            assert!(!r.note.is_empty(), "{}", r.id);
            assert!(r.review_by > r.as_of, "{}", r.id);
            let days = (r.review_by - r.as_of).get_days();
            assert!(days <= REVIEW_DAYS, "{} waits {days} days", r.id);
        }
        assert!(
            c.review_by
                <= c.as_of
                    .checked_add(jiff::Span::new().days(REVIEW_DAYS))
                    .unwrap()
        );
    }

    #[test]
    fn finds_the_numbers_the_steps_use() {
        let c = Catalogue::builtin();
        assert_eq!(c.cushion().unwrap().1, 3);
        assert_eq!(c.high_interest().unwrap().1, 10);
        let twenty = Money::new(2_000_000, Currency::GBP);
        assert_eq!(c.isa_allowance(TaxYear(2026)).unwrap().1, twenty);
        assert_eq!(c.isa_allowance(TaxYear(2027)).unwrap().1, twenty);
        assert_eq!(c.isa_allowance(TaxYear(2028)), None, "no guessing");
        assert_eq!(c.cash_isa_cap(TaxYear(2026)), None);
        let (_, cap, age) = c.cash_isa_cap(TaxYear(2027)).unwrap();
        assert_eq!((cap, age), (Money::new(1_200_000, Currency::GBP), 65));
        assert!(c.pension_match().is_some());
        assert!(c.rule("uk.savings-goals").is_some());
        assert!(c.rule("uk.risk-and-capacity").is_some());
        assert!(!c.needs_review(date(2026, 12, 30)));
        assert!(c.needs_review(date(2026, 12, 31)));
    }

    #[test]
    fn refuses_a_rule_it_does_not_understand() {
        let bad = r#"{"asOf":"2026-10-30","reviewBy":"2026-12-30","rules":[
            {"id":"x","kind":"mystery","asOf":"2026-10-30","reviewBy":"2026-12-30","source":"https://example.com","note":"n"}]}"#;
        assert!(Catalogue::parse(bad).is_err());
    }
}
