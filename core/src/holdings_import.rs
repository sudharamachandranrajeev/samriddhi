//! Read-only import of holdings from a file the user chooses (for example an
//! export from another app). Samriddhi only reads the file; nothing is ever
//! written back to the source.
//!
//! JSON (`format` is `samriddhi-holdings-v1`; the prototype's
//! `samridhi-holdings-v1` is read too):
//! `{"format":"samriddhi-holdings-v1","accounts":[{"name":"ISA",
//! "wrapper":"isa","holdings":[{"name":"Global equity fund",
//! "assetClass":"equity","value":12000,"currency":"GBP"}]}]}`
//!
//! CSV with the header `account,wrapper,holding,assetClass,value` and an
//! optional `currency` column. A missing currency is the base currency.
//!
//! Checks are strict: at most 2 MB, known asset classes and wrappers, and
//! amounts that are finite, not below zero and exact in their currency.
//! Amounts are read from the file's own digits straight into minor units,
//! never through a float. Bad rows are skipped and reported, and no message
//! repeats a value from the file. Ids are stable (`imp:<slug>`), so a second
//! import of the same file updates rows instead of adding them; an import
//! never deletes anything. [`Store::preview_holdings_import`] says what
//! [`Store::apply_holdings_import`] would do, and writes nothing.

use std::collections::BTreeMap;

use jiff::Timestamp;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::value::RawValue;

use crate::assets::AssetClass;
use crate::money::{Currency, Money, MoneyError};
use crate::store::{Error, Result, Store, iso};
use crate::wealth::{MAX_NAME, Wrapper};

pub const HOLDINGS_FORMAT: &str = "samriddhi-holdings-v1";
/// The prototype's name for the same format.
pub const LEGACY_HOLDINGS_FORMAT: &str = "samridhi-holdings-v1";
pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024;
/// Largest amount accepted, in major units (a trillion).
pub const MAX_VALUE_MAJOR: i128 = 1_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedHolding {
    pub name: String,
    pub asset_class: AssetClass,
    pub value: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedAccount {
    pub name: String,
    pub wrapper: Wrapper,
    pub holdings: Vec<ImportedHolding>,
}

impl ImportedAccount {
    /// Stable id: importing the same file twice updates instead of
    /// duplicating.
    pub fn id(&self) -> String {
        format!("imp:{}", slug(&self.name))
    }

    pub fn holding_id(&self, h: &ImportedHolding) -> String {
        format!("{}:{}", self.id(), slug(&h.name))
    }

    /// What the holdings in `currency` add up to; others are left out.
    pub fn total(&self, currency: Currency) -> Result<Money> {
        let v: Vec<Money> = self
            .holdings
            .iter()
            .map(|h| h.value)
            .filter(|m| m.currency == currency)
            .collect();
        Ok(Money::sum(currency, &v)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedHoldings {
    /// The currency rows without one were given.
    pub base: Currency,
    pub accounts: Vec<ImportedAccount>,
    /// Rows skipped, with the reason. Never holds the file's values.
    pub problems: Vec<String>,
}

impl ParsedHoldings {
    pub fn holding_count(&self) -> usize {
        self.accounts.iter().map(|a| a.holdings.len()).sum()
    }
}

/// Lower case, runs of anything but letters and digits as one '-', no '-'
/// at either end.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

fn refuse(why: &str) -> Error {
    Error::Invalid(why.to_owned())
}

/// Read `bytes` as JSON or CSV (decided by the first non-space character).
/// A file that cannot be used at all is refused; bad rows are skipped and
/// listed in `problems`.
pub fn parse_holdings(bytes: &[u8], base: Currency) -> Result<ParsedHoldings> {
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(refuse("The file is too large to be a holdings export."));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| refuse("The file is not UTF-8 text."))?;
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.is_empty() {
        return Err(refuse("The file is empty."));
    }
    let mut b = Builder {
        base,
        accounts: Vec::new(),
        problems: Vec::new(),
    };
    if text.starts_with('{') {
        from_json(text, &mut b)?;
    } else {
        from_csv(text, &mut b)?;
    }
    b.done()
}

struct Builder {
    base: Currency,
    accounts: Vec<ImportedAccount>,
    problems: Vec<String>,
}

/// One row as read, before any check.
struct Row<'a> {
    account: &'a str,
    wrapper: &'a str,
    holding: &'a str,
    class: &'a str,
    /// The amount's own text, or `None` if it was missing or not a number.
    value: Option<&'a str>,
    currency: &'a str,
}

impl Builder {
    fn add(&mut self, at: &str, row: Row) {
        match self.check(row) {
            Ok(()) => {}
            Err(why) => self.problems.push(format!("{at}: {why}")),
        }
    }

    fn check(&mut self, row: Row) -> std::result::Result<(), &'static str> {
        let account = row.account.trim();
        let holding = row.holding.trim();
        if account.is_empty() || holding.is_empty() {
            return Err("missing account or holding name");
        }
        if account.chars().count() > MAX_NAME || holding.chars().count() > MAX_NAME {
            return Err("a name is longer than 80 letters");
        }
        if slug(account).is_empty() || slug(holding).is_empty() {
            return Err("a name needs at least one letter or digit");
        }
        let class = AssetClass::from_alias(row.class).ok_or("unknown asset class")?;
        let currency = match row.currency.trim() {
            "" => self.base,
            c => Currency::new(c).map_err(|_| "unknown currency")?,
        };
        let value = row
            .value
            .ok_or("value is not a sensible amount")
            .and_then(|v| exact_amount(v, currency))?;
        let w = row.wrapper.trim().to_lowercase();
        let wrapper = if w.is_empty() {
            Wrapper::Taxable
        } else {
            Wrapper::parse(&w).ok_or("unknown wrapper")?
        };
        let i = match self.accounts.iter().position(|a| a.name == account) {
            Some(i) => i,
            None => {
                self.accounts.push(ImportedAccount {
                    name: account.to_owned(),
                    wrapper,
                    holdings: Vec::new(),
                });
                self.accounts.len() - 1
            }
        };
        let a = &mut self.accounts[i];
        if a.holdings.iter().any(|h| slug(&h.name) == slug(holding)) {
            return Err("the same holding appears twice in one account");
        }
        a.holdings.push(ImportedHolding {
            name: holding.to_owned(),
            asset_class: class,
            value,
        });
        Ok(())
    }

    fn done(self) -> Result<ParsedHoldings> {
        if self.accounts.is_empty() {
            return Err(Error::Invalid(match self.problems.first() {
                None => "No holdings found.".into(),
                Some(p) => format!("No valid holdings. First problem: {p}"),
            }));
        }
        Ok(ParsedHoldings {
            base: self.base,
            accounts: self.accounts,
            problems: self.problems,
        })
    }
}

/// An amount from its own digits, exactly: "3000.5", "1,234.00", "1.2e3".
/// Trailing zeros after the point are dropped (they change nothing); any
/// other digit past the currency's minor unit is refused, never rounded.
fn exact_amount(text: &str, currency: Currency) -> std::result::Result<Money, &'static str> {
    let bad = "value is not a sensible amount";
    let t = text.trim();
    let plain = if t.contains(['e', 'E']) {
        expand_exponent(t).ok_or(bad)?
    } else {
        t.to_owned()
    };
    let plain = match plain.split_once('.') {
        Some((w, f)) => {
            let f = f.trim_end_matches('0');
            if f.is_empty() {
                w.to_owned()
            } else {
                format!("{w}.{f}")
            }
        }
        None => plain,
    };
    let m = Money::parse(&plain, currency).map_err(|e| match e {
        MoneyError::TooPrecise { .. } => "value has more digits after the point than its currency",
        _ => bad,
    })?;
    if m.is_negative() {
        return Err(bad);
    }
    let cap = MAX_VALUE_MAJOR * 10i128.pow(u32::from(currency.exponent()));
    if i128::from(m.minor) > cap {
        return Err(bad);
    }
    Ok(m)
}

/// "1.25e3" as "1250", exactly, by moving the point. `None` if it is not a
/// plain number with an exponent.
fn expand_exponent(t: &str) -> Option<String> {
    let (num, exp) = t.split_once(['e', 'E'])?;
    let exp: i32 = exp.strip_prefix('+').unwrap_or(exp).parse().ok()?;
    if exp.abs() > 40 {
        return None;
    }
    let (sign, num) = match num.strip_prefix('-') {
        Some(n) => ("-", n),
        None => ("", num),
    };
    let (whole, frac) = num.split_once('.').unwrap_or((num, ""));
    let ok = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !ok(whole) || !ok(frac) {
        return None;
    }
    let digits = format!("{whole}{frac}");
    let point = whole.len() as i32 + exp;
    let s = if point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}", "0".repeat(point as usize - digits.len()))
    } else {
        let (w, f) = digits.split_at(point as usize);
        format!("{w}.{f}")
    };
    Some(format!("{sign}{s}"))
}

type RawMap = BTreeMap<String, Box<RawValue>>;

/// A field as text: a string as itself, any other value as its JSON text,
/// missing or null as empty.
fn field(map: &RawMap, key: &str) -> String {
    match map.get(key).map(|r| r.get().trim()) {
        None | Some("null") => String::new(),
        Some(raw) => serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_owned()),
    }
}

/// An amount's own text: a JSON number's literal digits, or a string.
fn amount_text(map: &RawMap, key: &str) -> Option<String> {
    let raw = map.get(key)?.get().trim();
    if raw.starts_with('"') {
        serde_json::from_str::<String>(raw).ok()
    } else if raw.starts_with(|c: char| c == '-' || c.is_ascii_digit()) {
        Some(raw.to_owned())
    } else {
        None
    }
}

fn from_json(text: &str, b: &mut Builder) -> Result<()> {
    let root: Box<RawValue> = serde_json::from_str(text).map_err(|_| refuse("Not valid JSON."))?;
    let not_ours = || refuse("Not a samriddhi-holdings-v1 file.");
    let root: RawMap = serde_json::from_str(root.get()).map_err(|_| not_ours())?;
    let format = root
        .get("format")
        .and_then(|f| serde_json::from_str::<String>(f.get()).ok());
    if !matches!(
        format.as_deref(),
        Some(HOLDINGS_FORMAT | LEGACY_HOLDINGS_FORMAT)
    ) {
        return Err(not_ours());
    }
    let accounts: Vec<Box<RawValue>> = root
        .get("accounts")
        .and_then(|a| serde_json::from_str(a.get()).ok())
        .ok_or_else(|| refuse("No \"accounts\" list."))?;
    for (i, a) in accounts.iter().enumerate() {
        let account = serde_json::from_str::<RawMap>(a.get()).ok().and_then(|m| {
            let hs = serde_json::from_str::<Vec<Box<RawValue>>>(m.get("holdings")?.get()).ok()?;
            Some((m, hs))
        });
        let Some((a, hs)) = account else {
            b.problems.push(format!("account {}: malformed", i + 1));
            continue;
        };
        let (name, wrapper) = (field(&a, "name"), field(&a, "wrapper"));
        for (j, h) in hs.iter().enumerate() {
            let at = format!("account {}, holding {}", i + 1, j + 1);
            let Ok(h) = serde_json::from_str::<RawMap>(h.get()) else {
                b.problems.push(format!("{at}: malformed"));
                continue;
            };
            let value = amount_text(&h, "value");
            b.add(
                &at,
                Row {
                    account: &name,
                    wrapper: &wrapper,
                    holding: &field(&h, "name"),
                    class: &field(&h, "assetClass"),
                    value: value.as_deref(),
                    currency: &field(&h, "currency"),
                },
            );
        }
    }
    Ok(())
}

/// One CSV line: commas split fields, double quotes wrap them, "" is a
/// quote inside quotes.
fn csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' && chars.peek() == Some(&'"') {
                cur.push('"');
                chars.next();
            } else if ch == '"' {
                quoted = false;
            } else {
                cur.push(ch);
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == ',' {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    out.push(cur);
    out
}

fn from_csv(text: &str, b: &mut Builder) -> Result<()> {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split(['\n', '\r']).collect();
    let head: Vec<String> = csv_line(lines[0])
        .iter()
        .map(|s| s.trim().to_lowercase())
        .collect();
    let col = |n: &str| head.iter().position(|h| h == n);
    let need = ["account", "wrapper", "holding", "assetclass", "value"];
    let idx: Option<Vec<usize>> = need.iter().map(|n| col(n)).collect();
    let Some(idx) = idx else {
        return Err(refuse(
            "CSV header must be: account,wrapper,holding,assetClass,value (and optionally currency)",
        ));
    };
    let cur = col("currency");
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let f = csv_line(line);
        let at = |k: usize| f.get(k).map_or("", String::as_str);
        let value = at(idx[4]);
        b.add(
            &format!("row {}", i + 1),
            Row {
                account: at(idx[0]),
                wrapper: at(idx[1]),
                holding: at(idx[2]),
                class: at(idx[3]),
                value: (!value.trim().is_empty()).then_some(value),
                currency: cur.map_or("", at),
            },
        );
    }
    Ok(())
}

/// What an import would do. Nothing is ever deleted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingsImportPreview {
    pub new_accounts: usize,
    pub updated_accounts: usize,
    pub unchanged_accounts: usize,
    pub new_holdings: usize,
    pub updated_holdings: usize,
    pub unchanged_holdings: usize,
    /// Holdings not in the base currency: kept, but not counted in totals
    /// until exchange rates arrive.
    pub not_counted_no_fx: usize,
    /// Rows the file had that will be skipped, with the reason.
    pub problems: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    New,
    Updated,
    Unchanged,
}

impl Store {
    fn import_changes(&self, p: &ParsedHoldings) -> Result<Vec<(Change, Vec<Change>)>> {
        let mut out = Vec::new();
        for a in &p.accounts {
            let have: Option<(String, String, String)> = self
                .conn
                .query_row(
                    "SELECT name, wrapper, source FROM sam_accounts WHERE id = ?",
                    [a.id()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let ac = match have {
                None => Change::New,
                Some((n, w, s)) if n == a.name && w == a.wrapper.as_str() && s == "import" => {
                    Change::Unchanged
                }
                Some(_) => Change::Updated,
            };
            let mut hs = Vec::new();
            for h in &a.holdings {
                let have: Option<(String, String, i64, String)> = self
                    .conn
                    .query_row(
                        "SELECT name, asset_class, amount_minor, currency FROM sam_holdings WHERE id = ?",
                        [a.holding_id(h)],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )
                    .optional()?;
                hs.push(match have {
                    None => Change::New,
                    Some((n, c, m, cur))
                        if n == h.name
                            && c == h.asset_class.as_str()
                            && m == h.value.minor
                            && cur == h.value.currency.code() =>
                    {
                        Change::Unchanged
                    }
                    Some(_) => Change::Updated,
                });
            }
            out.push((ac, hs));
        }
        Ok(out)
    }

    /// Say what importing `p` would change. Nothing is written.
    pub fn preview_holdings_import(&self, p: &ParsedHoldings) -> Result<HoldingsImportPreview> {
        let changes = self.import_changes(p)?;
        let mut v = HoldingsImportPreview {
            problems: p.problems.clone(),
            not_counted_no_fx: p
                .accounts
                .iter()
                .flat_map(|a| &a.holdings)
                .filter(|h| h.value.currency != p.base)
                .count(),
            ..Default::default()
        };
        for (ac, hs) in &changes {
            match ac {
                Change::New => v.new_accounts += 1,
                Change::Updated => v.updated_accounts += 1,
                Change::Unchanged => v.unchanged_accounts += 1,
            }
            for h in hs {
                match h {
                    Change::New => v.new_holdings += 1,
                    Change::Updated => v.updated_holdings += 1,
                    Change::Unchanged => v.unchanged_holdings += 1,
                }
            }
        }
        Ok(v)
    }

    /// Add or update the accounts and holdings in `p`, all in one
    /// transaction, and note it in the audit log. Rows not in the file are
    /// left alone.
    pub fn apply_holdings_import(
        &mut self,
        p: &ParsedHoldings,
        now: Timestamp,
    ) -> Result<HoldingsImportPreview> {
        let preview = self.preview_holdings_import(p)?;
        let changes = self.import_changes(p)?;
        let at = iso(now);
        let tx = self.conn.transaction()?;
        for (a, (ac, hs)) in p.accounts.iter().zip(&changes) {
            if *ac != Change::Unchanged {
                tx.execute(
                    "INSERT INTO sam_accounts (id, name, wrapper, source, updated_at) VALUES (?1, ?2, ?3, 'import', ?4)
                     ON CONFLICT(id) DO UPDATE SET name = ?2, wrapper = ?3, source = 'import', updated_at = ?4",
                    params![a.id(), a.name, a.wrapper.as_str(), at],
                )?;
            }
            for (h, hc) in a.holdings.iter().zip(hs) {
                if *hc == Change::Unchanged {
                    continue;
                }
                tx.execute(
                    "INSERT INTO sam_holdings (id, account_id, name, asset_class, amount_minor, currency, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(id) DO UPDATE SET name = ?3, asset_class = ?4, amount_minor = ?5,
                         currency = ?6, updated_at = ?7",
                    params![
                        a.holding_id(h),
                        a.id(),
                        h.name,
                        h.asset_class.as_str(),
                        h.value.minor,
                        h.value.currency.code(),
                        at
                    ],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO sam_audit (at, actor, action, what) VALUES (?, 'local', 'write', 'holdings import')",
            [&at],
        )?;
        tx.commit()?;
        Ok(preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::{NOW, at};

    fn gbp(minor: i64) -> Money {
        Money::new(minor, Currency::GBP)
    }

    fn parse(s: &str) -> Result<ParsedHoldings> {
        parse_holdings(s.as_bytes(), Currency::GBP)
    }

    // --- Ported from test/core/holdings_import_test.dart ---

    #[test]
    fn csv_groups_by_account_maps_aliases_skips_bad_rows() {
        let r = parse(
            "account,wrapper,holding,assetClass,value\n\
             My ISA,isa,\"Global, equity\",stocks,12000\n\
             My ISA,isa,Gilts,bond,3000.5\n\
             Bank,cash,Savings,cash,500\n\
             Bank,cash,Weird,crypto,10\n\
             Bank,cash,Neg,cash,-5\n",
        )
        .unwrap();
        let names: Vec<&str> = r.accounts.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["My ISA", "Bank"]);
        let first = &r.accounts[0];
        assert_eq!(first.holdings[0].name, "Global, equity");
        assert_eq!(first.holdings[0].asset_class, AssetClass::Equity);
        // 15000.5 pounds, exactly.
        assert_eq!(first.total(Currency::GBP).unwrap(), gbp(1_500_050));
        assert_eq!(r.problems.len(), 2);
        assert!(!r.problems.join("").contains("crypto"));
        assert!(!r.problems.join("").contains("-5"));
    }

    #[test]
    fn json_parses_and_ids_are_stable() {
        let json = serde_json::json!({
            "format": HOLDINGS_FORMAT,
            "accounts": [{
                "name": "Pension",
                "wrapper": "pension",
                "holdings": [{ "name": "Index fund", "assetClass": "equity", "value": 100 }],
            }],
        })
        .to_string();
        let a = parse(&json).unwrap().accounts.remove(0);
        let a2 = parse(&json).unwrap().accounts.remove(0);
        assert_eq!(a.id(), a2.id());
        assert_eq!(a.id(), "imp:pension");
        assert_eq!(a.holding_id(&a.holdings[0]), "imp:pension:index-fund");
        assert_eq!(a.holdings[0].value, gbp(10_000));
    }

    #[test]
    fn unusable_files_are_refused_with_a_message() {
        for bad in [
            "",
            "{\"format\":\"other\",\"accounts\":[]}",
            "{not json",
            "a,b\n1,2",
            "account,wrapper,holding,assetClass,value\nX,isa,Y,nope,1",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(parse_holdings(&vec![b'A'; MAX_IMPORT_BYTES + 1], Currency::GBP).is_err());
    }

    // --- Samriddhi-specific: currency, exactness, no echo, preview/apply ---

    #[test]
    fn the_prototype_format_name_is_read_too() {
        let json = "{\"format\":\"samridhi-holdings-v1\",\"accounts\":[{\"name\":\"A\",\"holdings\":[{\"name\":\"B\",\"assetClass\":\"cash\",\"value\":1}]}]}";
        let r = parse(json).unwrap();
        assert_eq!(
            r.accounts[0].wrapper,
            Wrapper::Taxable,
            "no wrapper is taxable"
        );
    }

    #[test]
    fn amounts_are_exact_and_messages_never_echo_values() {
        let r = parse(
            "account,wrapper,holding,assetClass,value,currency\n\
             A,isa,One,cash,0.1\n\
             A,isa,Two,cash,\"1,234.50\"\n\
             A,isa,Three,cash,1.5e3\n\
             A,isa,Four,cash,12.30,usd\n\
             A,isa,Five,cash,1.005\n\
             A,isa,Six,cash,NaN\n\
             A,isa,Seven,cash,inf\n\
             A,isa,Eight,cash,1,ZZZ\n\
             A,isa,Nine,cash,2000000000000\n\
             A,secretwrapper,Ten,cash,1\n\
             A,isa,One,cash,5\n",
        )
        .unwrap();
        let v: Vec<Money> = r.accounts[0].holdings.iter().map(|h| h.value).collect();
        assert_eq!(
            v,
            [
                gbp(10),
                gbp(123_450),
                gbp(150_000),
                Money::new(1230, Currency::USD)
            ]
        );
        assert_eq!(r.problems.len(), 7, "{:?}", r.problems);
        let all = r.problems.join("\n");
        for raw in [
            "1.005",
            "NaN",
            "inf",
            "ZZZ",
            "2000000000000",
            "secretwrapper",
        ] {
            assert!(!all.contains(raw), "{raw} echoed in {all}");
        }
        assert!(all.contains("row 6: value has more digits"));
        assert!(all.contains("row 12: the same holding appears twice"));

        // JSON numbers keep their own digits; a float would not.
        let json = "{\"format\":\"samriddhi-holdings-v1\",\"accounts\":[{\"name\":\"A\",\"wrapper\":\"cash\",\"holdings\":[
            {\"name\":\"x\",\"assetClass\":\"cash\",\"value\":999999999999.99},
            {\"name\":\"y\",\"assetClass\":\"cash\",\"value\":\"0.30\"},
            {\"name\":\"z\",\"assetClass\":\"cash\",\"value\":true},
            {\"name\":\"w\",\"assetClass\":\"cash\",\"value\":-1e2},
            7]}, \"junk\"]}";
        let r = parse(json).unwrap();
        let v: Vec<Money> = r.accounts[0].holdings.iter().map(|h| h.value).collect();
        assert_eq!(v, [gbp(99_999_999_999_999), gbp(30)]);
        assert_eq!(
            r.problems,
            [
                "account 1, holding 3: value is not a sensible amount",
                "account 1, holding 4: value is not a sensible amount",
                "account 1, holding 5: malformed",
                "account 2: malformed",
            ]
        );
    }

    #[test]
    fn names_slug_unicode_and_bad_utf8_is_refused() {
        assert_eq!(slug("  Global, Equity -- Fund! "), "global-equity-fund");
        assert_eq!(slug("बचत खाता"), "बचत-खाता");
        assert!(parse_holdings(&[0xff, 0xfe, 0x00], Currency::GBP).is_err());
        let r = parse("account,wrapper,holding,assetClass,value\n!!!,isa,x,cash,1\nA,isa,x,cash,1")
            .unwrap();
        assert_eq!(
            r.problems,
            ["row 2: a name needs at least one letter or digit"]
        );
    }

    #[test]
    fn preview_counts_then_apply_updates_and_never_deletes() {
        let mut s = Store::memory().unwrap();
        let now = at(NOW);
        let manual = s.add_account("Mine", Wrapper::Cash, now).unwrap();
        s.add_holding(&manual.id, "Jar", AssetClass::Cash, gbp(100), now)
            .unwrap();
        let file = "account,wrapper,holding,assetClass,value,currency\n\
                    ISA,isa,World,equity,100\n\
                    ISA,isa,Gilts,bonds,50\n\
                    Abroad,cash,Savings,cash,10,INR\n";
        let p = parse(file).unwrap();
        let v = s.preview_holdings_import(&p).unwrap();
        assert_eq!((v.new_accounts, v.new_holdings), (2, 3));
        assert_eq!(v.not_counted_no_fx, 1);
        assert_eq!(s.holdings().unwrap().len(), 1, "a preview writes nothing");

        s.apply_holdings_import(&p, now).unwrap();
        assert_eq!(s.accounts().unwrap().len(), 3);
        assert_eq!(s.holdings().unwrap().len(), 4);
        assert_eq!(s.audit_log(1).unwrap()[0].3, "holdings import");
        let t = s.totals(Currency::GBP).unwrap();
        assert_eq!(t.total, gbp(15_100));
        assert_eq!(t.not_counted_no_fx.len(), 1);

        // The same file again changes nothing; a changed one updates in place.
        let again = s.preview_holdings_import(&p).unwrap();
        assert_eq!((again.new_holdings, again.updated_holdings), (0, 0));
        assert_eq!(again.unchanged_holdings, 3);
        let p2 =
            parse("account,wrapper,holding,assetClass,value\nISA,isa,World,equity,250\n").unwrap();
        let v2 = s.apply_holdings_import(&p2, now).unwrap();
        assert_eq!(
            (v2.updated_holdings, v2.unchanged_accounts, v2.new_holdings),
            (1, 1, 0)
        );
        let hs = s.holdings().unwrap();
        assert_eq!(hs.len(), 4, "rows missing from the file stay");
        let world = hs.iter().find(|h| h.id == "imp:isa:world").unwrap();
        assert_eq!(world.amount, gbp(25_000));
        assert!(hs.iter().any(|h| h.name == "Jar"), "manual rows stay");
    }
}
