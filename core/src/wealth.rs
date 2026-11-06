//! Accounts and the holdings in them, entered by hand or imported from a
//! file. Every amount is exact [`Money`] in its own currency. Until exchange
//! rates arrive, a holding in another currency than the base one is listed
//! but not counted: [`totals`] returns it in `not_counted_no_fx`.

use std::collections::BTreeMap;

use jiff::Timestamp;
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::assets::{ALL, AssetClass, ClassWeights};
use crate::money::{Currency, Money};
use crate::store::{Error, Result, Store, iso};

/// Longest account or holding name.
pub const MAX_NAME: usize = 80;

/// The kind of account a holding sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Wrapper {
    Isa,
    Pension,
    Taxable,
    Cash,
}

impl Wrapper {
    pub fn as_str(&self) -> &'static str {
        match self {
            Wrapper::Isa => "isa",
            Wrapper::Pension => "pension",
            Wrapper::Taxable => "taxable",
            Wrapper::Cash => "cash",
        }
    }

    pub fn parse(s: &str) -> Option<Wrapper> {
        [
            Wrapper::Isa,
            Wrapper::Pension,
            Wrapper::Taxable,
            Wrapper::Cash,
        ]
        .into_iter()
        .find(|w| w.as_str() == s)
    }
}

/// Where an account came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountSource {
    Manual,
    Import,
}

impl AccountSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountSource::Manual => "manual",
            AccountSource::Import => "import",
        }
    }

    fn parse(s: &str) -> Result<AccountSource> {
        match s {
            "manual" => Ok(AccountSource::Manual),
            "import" => Ok(AccountSource::Import),
            _ => Err(Error::Invalid(format!("odd account source {s:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub wrapper: Wrapper,
    pub source: AccountSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub amount: Money,
}

/// What the holdings add up to in the base currency.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub base: Currency,
    pub total: Money,
    /// Every class, zero where nothing is held.
    pub by_class: BTreeMap<AssetClass, Money>,
    /// Fractions of `total` per class; all zero when the total is zero.
    pub weights: ClassWeights,
    /// Holdings in another currency: listed, not counted, until exchange
    /// rates arrive.
    pub not_counted_no_fx: Vec<Holding>,
}

/// Add up holdings in `base`. Holdings in any other currency go to
/// `not_counted_no_fx` instead of the totals.
pub fn totals(holdings: &[Holding], base: Currency) -> Result<Totals> {
    let mut by_class: BTreeMap<AssetClass, Money> =
        ALL.iter().map(|c| (*c, Money::zero(base))).collect();
    let mut not_counted_no_fx = Vec::new();
    for h in holdings {
        if h.amount.currency != base {
            not_counted_no_fx.push(h.clone());
            continue;
        }
        let slot = by_class.entry(h.asset_class).or_insert(Money::zero(base));
        *slot = slot.checked_add(h.amount)?;
    }
    let all: Vec<Money> = by_class.values().copied().collect();
    let total = Money::sum(base, &all)?;
    Ok(Totals {
        base,
        total,
        weights: to_weights(&by_class),
        by_class,
        not_counted_no_fx,
    })
}

/// Fractions of the total per class (all zero if the total is zero). The
/// amounts must share one currency, as [`totals`] makes them.
pub fn to_weights(values: &BTreeMap<AssetClass, Money>) -> ClassWeights {
    let total: i128 = values.values().map(|m| m.minor as i128).sum();
    ALL.iter()
        .map(|c| {
            let v = values.get(c).map_or(0, |m| m.minor) as f64;
            (*c, if total == 0 { 0.0 } else { v / total as f64 })
        })
        .collect()
}

fn check_name(name: &str, what: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > MAX_NAME {
        return Err(Error::Invalid(format!(
            "{what} needs a short name (up to {MAX_NAME} letters)"
        )));
    }
    Ok(n.to_owned())
}

fn check_amount(m: &Money) -> Result<()> {
    if m.is_negative() {
        return Err(Error::Invalid("a holding cannot be below zero".into()));
    }
    Ok(())
}

fn account_row(r: &Row) -> rusqlite::Result<(String, String, String, String)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

fn to_account((id, name, wrapper, source): (String, String, String, String)) -> Result<Account> {
    Ok(Account {
        id,
        name,
        wrapper: Wrapper::parse(&wrapper)
            .ok_or_else(|| Error::Invalid(format!("odd wrapper {wrapper:?}")))?,
        source: AccountSource::parse(&source)?,
    })
}

type HoldingRow = (String, String, String, String, i64, String);

fn holding_row(r: &Row) -> rusqlite::Result<HoldingRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

fn to_holding((id, account_id, name, class, minor, currency): HoldingRow) -> Result<Holding> {
    Ok(Holding {
        id,
        account_id,
        name,
        asset_class: AssetClass::parse(&class)
            .ok_or_else(|| Error::Invalid(format!("odd asset class {class:?}")))?,
        amount: Money::new(minor, Currency::new(&currency)?),
    })
}

const HOLDING_COLUMNS: &str = "id, account_id, name, asset_class, amount_minor, currency";

impl Store {
    /// A new account entered by hand.
    pub fn add_account(&self, name: &str, wrapper: Wrapper, now: Timestamp) -> Result<Account> {
        let a = Account {
            id: uuid::Uuid::now_v7().to_string(),
            name: check_name(name, "an account")?,
            wrapper,
            source: AccountSource::Manual,
        };
        self.conn.execute(
            "INSERT INTO sam_accounts (id, name, wrapper, source, updated_at) VALUES (?, ?, ?, ?, ?)",
            params![a.id, a.name, a.wrapper.as_str(), a.source.as_str(), iso(now)],
        )?;
        Ok(a)
    }

    /// Rename an account or change its wrapper.
    pub fn update_account(
        &self,
        id: &str,
        name: &str,
        wrapper: Wrapper,
        now: Timestamp,
    ) -> Result<()> {
        let name = check_name(name, "an account")?;
        let n = self.conn.execute(
            "UPDATE sam_accounts SET name = ?, wrapper = ?, updated_at = ? WHERE id = ?",
            params![name, wrapper.as_str(), iso(now), id],
        )?;
        if n == 0 {
            return Err(Error::Invalid("no such account".into()));
        }
        Ok(())
    }

    /// Remove an account and every holding in it, together.
    pub fn delete_account(&mut self, id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM sam_holdings WHERE account_id = ?", [id])?;
        tx.execute("DELETE FROM sam_accounts WHERE id = ?", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn account(&self, id: &str) -> Result<Option<Account>> {
        self.conn
            .query_row(
                "SELECT id, name, wrapper, source FROM sam_accounts WHERE id = ?",
                [id],
                account_row,
            )
            .optional()?
            .map(to_account)
            .transpose()
    }

    /// Every account, by name.
    pub fn accounts(&self) -> Result<Vec<Account>> {
        let mut st = self
            .conn
            .prepare("SELECT id, name, wrapper, source FROM sam_accounts ORDER BY name, id")?;
        let rows = st.query_map([], account_row)?;
        rows.map(|r| to_account(r?)).collect()
    }

    /// A new holding in an existing account.
    pub fn add_holding(
        &self,
        account_id: &str,
        name: &str,
        asset_class: AssetClass,
        amount: Money,
        now: Timestamp,
    ) -> Result<Holding> {
        check_amount(&amount)?;
        if self.account(account_id)?.is_none() {
            return Err(Error::Invalid("no such account".into()));
        }
        let h = Holding {
            id: uuid::Uuid::now_v7().to_string(),
            account_id: account_id.to_owned(),
            name: check_name(name, "a holding")?,
            asset_class,
            amount,
        };
        self.conn.execute(
            &format!("INSERT INTO sam_holdings ({HOLDING_COLUMNS}, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)"),
            params![
                h.id,
                h.account_id,
                h.name,
                h.asset_class.as_str(),
                h.amount.minor,
                h.amount.currency.code(),
                iso(now)
            ],
        )?;
        Ok(h)
    }

    /// Change a holding's name, class or amount (it stays in its account).
    pub fn update_holding(&self, h: &Holding, now: Timestamp) -> Result<()> {
        check_amount(&h.amount)?;
        let name = check_name(&h.name, "a holding")?;
        let n = self.conn.execute(
            "UPDATE sam_holdings SET name = ?, asset_class = ?, amount_minor = ?, currency = ?, updated_at = ?
             WHERE id = ? AND account_id = ?",
            params![
                name,
                h.asset_class.as_str(),
                h.amount.minor,
                h.amount.currency.code(),
                iso(now),
                h.id,
                h.account_id
            ],
        )?;
        if n == 0 {
            return Err(Error::Invalid("no such holding".into()));
        }
        Ok(())
    }

    pub fn delete_holding(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM sam_holdings WHERE id = ?", [id])?;
        Ok(())
    }

    /// Every holding, by account then name.
    pub fn holdings(&self) -> Result<Vec<Holding>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {HOLDING_COLUMNS} FROM sam_holdings ORDER BY account_id, name, id"
        ))?;
        let rows = st.query_map([], holding_row)?;
        rows.map(|r| to_holding(r?)).collect()
    }

    /// The holdings in one account, by name.
    pub fn holdings_of(&self, account_id: &str) -> Result<Vec<Holding>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {HOLDING_COLUMNS} FROM sam_holdings WHERE account_id = ? ORDER BY name, id"
        ))?;
        let rows = st.query_map([account_id], holding_row)?;
        rows.map(|r| to_holding(r?)).collect()
    }

    /// What the stored holdings add up to in `base`.
    pub fn totals(&self, base: Currency) -> Result<Totals> {
        totals(&self.holdings()?, base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::{NOW, at};

    fn gbp(minor: i64) -> Money {
        Money::new(minor, Currency::GBP)
    }

    fn h(class: AssetClass, amount: Money) -> Holding {
        Holding {
            id: "h".into(),
            account_id: "a".into(),
            name: "n".into(),
            asset_class: class,
            amount,
        }
    }

    /// Ported from `test/core/plan_test.dart` ("valueByClass/toWeights
    /// aggregate and normalise"), with exact money in place of doubles.
    #[test]
    fn value_by_class_and_weights_aggregate_and_normalise() {
        let t = totals(
            &[
                h(AssetClass::Equity, gbp(30_000)),
                h(AssetClass::Equity, gbp(10_000)),
                h(AssetClass::Bonds, gbp(60_000)),
            ],
            Currency::GBP,
        )
        .unwrap();
        assert_eq!(t.by_class[&AssetClass::Equity], gbp(40_000));
        assert_eq!(t.total, gbp(100_000));
        assert!((t.weights[&AssetClass::Equity] - 0.4).abs() < 1e-12);
        assert!((t.weights[&AssetClass::Bonds] - 0.6).abs() < 1e-12);
        let empty = totals(&[], Currency::GBP).unwrap();
        assert_eq!(empty.weights[&AssetClass::Cash], 0.0);
        assert_eq!(empty.total, gbp(0));
    }

    #[test]
    fn other_currencies_are_listed_but_not_counted() {
        let usd = h(AssetClass::Equity, Money::new(50_000, Currency::USD));
        let t = totals(
            &[h(AssetClass::Cash, gbp(100_000)), usd.clone()],
            Currency::GBP,
        )
        .unwrap();
        assert_eq!(t.total, gbp(100_000));
        assert_eq!(t.weights[&AssetClass::Cash], 1.0);
        assert_eq!(t.weights[&AssetClass::Equity], 0.0);
        assert_eq!(t.not_counted_no_fx, vec![usd]);
        // In an INR base the same list flips round.
        let t = totals(&[h(AssetClass::Cash, gbp(1))], Currency::INR).unwrap();
        assert_eq!(t.total, Money::zero(Currency::INR));
        assert_eq!(t.not_counted_no_fx.len(), 1);
    }

    #[test]
    fn accounts_and_holdings_round_trip() {
        let mut s = Store::memory().unwrap();
        let now = at(NOW);
        let a = s.add_account("  Everyday ISA ", Wrapper::Isa, now).unwrap();
        assert_eq!(a.name, "Everyday ISA");
        assert_eq!(a.source, AccountSource::Manual);
        let fund = s
            .add_holding(&a.id, "World fund", AssetClass::Equity, gbp(1_200_000), now)
            .unwrap();
        s.add_holding(&a.id, "Cash", AssetClass::Cash, gbp(50_000), now)
            .unwrap();
        assert_eq!(s.accounts().unwrap(), vec![a.clone()]);
        assert_eq!(s.holdings_of(&a.id).unwrap().len(), 2);

        let mut changed = fund.clone();
        changed.amount = gbp(1_300_000);
        changed.asset_class = AssetClass::Bonds;
        s.update_holding(&changed, now).unwrap();
        assert!(s.holdings().unwrap().contains(&changed));
        assert_eq!(
            s.totals(Currency::GBP).unwrap().total,
            gbp(1_350_000),
            "both holdings count"
        );

        s.update_account(&a.id, "Stocks ISA", Wrapper::Isa, now)
            .unwrap();
        assert_eq!(s.account(&a.id).unwrap().unwrap().name, "Stocks ISA");

        // Refused: a negative amount, an empty name, an unknown account.
        let mut neg = changed.clone();
        neg.amount = gbp(-1);
        assert!(s.update_holding(&neg, now).is_err());
        assert!(s.add_account(" ", Wrapper::Cash, now).is_err());
        assert!(
            s.add_holding("nope", "X", AssetClass::Cash, gbp(1), now)
                .is_err()
        );
        assert!(s.update_account("nope", "X", Wrapper::Cash, now).is_err());

        s.delete_holding(&fund.id).unwrap();
        assert_eq!(s.holdings().unwrap().len(), 1);
        s.delete_account(&a.id).unwrap();
        assert!(s.accounts().unwrap().is_empty());
        assert!(s.holdings().unwrap().is_empty(), "its holdings go with it");
    }
}
