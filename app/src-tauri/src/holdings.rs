//! Accounts and holdings as the Wealth screen asks for them, and the
//! holdings import (pick a file, see what it would change, then confirm).
//! Every rule lives in the core; these commands only pass the time in, pick
//! the base currency and note writes in the activity log.

use samriddhi_core::agents::base_currency;
use samriddhi_core::assets::AssetClass;
use samriddhi_core::holdings_import::{HoldingsImportPreview, ParsedHoldings, parse_holdings};
use samriddhi_core::money::{Currency, Money};
use samriddhi_core::store::{self, Action, Store};
use samriddhi_core::wealth::{Account, Holding, Totals, Wrapper};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::data::{BEFORE_IMPORT, read_picked};
use crate::{App, CommandResult, err, now};

/// Before-import backups kept (shared with the full import's prefix).
const KEEP_BEFORE_IMPORT: usize = 5;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    account: Account,
    holdings: Vec<Holding>,
    /// What the holdings in the base currency add up to.
    counted: Money,
    /// Holdings in other currencies, left out of `counted` until exchange
    /// rates arrive.
    not_counted: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WealthView {
    base: Currency,
    accounts: Vec<AccountView>,
    /// Every holding added up in the base currency, with the ones left out.
    totals: Totals,
    /// The currencies left out of the totals, each named once, in order.
    left_out: Vec<Currency>,
}

/// The whole Wealth screen from what is stored.
pub fn wealth_view_of(s: &Store) -> store::Result<WealthView> {
    let base = base_currency(&s.profile()?);
    let holdings = s.holdings()?;
    let totals = samriddhi_core::wealth::totals(&holdings, base)?;
    let mut accounts = Vec::new();
    for account in s.accounts()? {
        let own: Vec<Holding> = holdings
            .iter()
            .filter(|h| h.account_id == account.id)
            .cloned()
            .collect();
        let counted: Vec<Money> = own
            .iter()
            .map(|h| h.amount)
            .filter(|m| m.currency == base)
            .collect();
        accounts.push(AccountView {
            counted: Money::sum(base, &counted)?,
            not_counted: own.len() - counted.len(),
            holdings: own,
            account,
        });
    }
    let mut left_out: Vec<Currency> = totals
        .not_counted_no_fx
        .iter()
        .map(|h| h.amount.currency)
        .collect();
    left_out.sort();
    left_out.dedup();
    Ok(WealthView {
        base,
        accounts,
        totals,
        left_out,
    })
}

fn noted<T>(s: &Store, what: &str, v: T) -> store::Result<T> {
    s.audit("local", Action::Write, what, now())?;
    Ok(v)
}

#[tauri::command]
pub fn wealth_view(state: State<App>) -> CommandResult<WealthView> {
    state.with(|s| wealth_view_of(s))
}

#[tauri::command]
pub fn account_add(state: State<App>, name: String, wrapper: Wrapper) -> CommandResult<Account> {
    state.with(|s| {
        let a = s.add_account(&name, wrapper, now())?;
        noted(s, "accounts", a)
    })
}

#[tauri::command]
pub fn account_update(
    state: State<App>,
    id: String,
    name: String,
    wrapper: Wrapper,
) -> CommandResult<()> {
    state.with(|s| {
        s.update_account(&id, &name, wrapper, now())?;
        noted(s, "accounts", ())
    })
}

/// Removes the account and every holding in it.
#[tauri::command]
pub fn account_delete(state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| {
        s.delete_account(&id)?;
        noted(s, "accounts", ())
    })
}

#[tauri::command]
pub fn holding_add(
    state: State<App>,
    account_id: String,
    name: String,
    asset_class: AssetClass,
    amount: Money,
) -> CommandResult<Holding> {
    state.with(|s| {
        let h = s.add_holding(&account_id, &name, asset_class, amount, now())?;
        noted(s, "holdings", h)
    })
}

#[tauri::command]
pub fn holding_update(state: State<App>, holding: Holding) -> CommandResult<()> {
    state.with(|s| {
        s.update_holding(&holding, now())?;
        noted(s, "holdings", ())
    })
}

#[tauri::command]
pub fn holding_delete(state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| {
        s.delete_holding(&id)?;
        noted(s, "holdings", ())
    })
}

/// What a picked holdings file holds and what importing it would change.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingsImportView {
    /// The file's name, without its folder.
    file: String,
    /// Rows without a currency are read in this one.
    base: Currency,
    accounts: usize,
    holdings: usize,
    preview: HoldingsImportPreview,
}

/// Read `bytes` and say what importing them would change. Writes nothing.
pub fn import_view_of(
    s: &Store,
    bytes: &[u8],
    file: &str,
) -> store::Result<(HoldingsImportView, ParsedHoldings)> {
    let base = base_currency(&s.profile()?);
    let parsed = parse_holdings(bytes, base)?;
    let view = HoldingsImportView {
        file: file.to_owned(),
        base,
        accounts: parsed.accounts.len(),
        holdings: parsed.holding_count(),
        preview: s.preview_holdings_import(&parsed)?,
    };
    Ok((view, parsed))
}

/// Let the user pick a holdings file (JSON or CSV), check it, and say what
/// importing it would change. Nothing changes until `holdings_import_apply`.
/// `None` when cancelled.
#[tauri::command]
pub async fn holdings_import_pick(
    app: AppHandle,
    state: State<'_, App>,
) -> CommandResult<Option<HoldingsImportView>> {
    let dialog = app
        .dialog()
        .file()
        .set_title("Choose a holdings file to import")
        .add_filter("Holdings (JSON or CSV)", &["json", "csv"]);
    let Some(path) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(err)?
    else {
        return Ok(None);
    };
    let file = path
        .to_string()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("holdings")
        .to_owned();
    let bytes =
        read_picked(&app, path).map_err(|e| format!("That file could not be read ({e})."))?;
    let (view, parsed) = state.with(|s| import_view_of(s, &bytes, &file))?;
    state.lock_inner()?.pending_holdings = Some(parsed);
    Ok(Some(view))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingsImportDone {
    /// The backup made of the data before the import.
    backup: String,
    done: HoldingsImportPreview,
}

/// Import the picked holdings file, after backing up the current data. Adds
/// or updates; never deletes.
#[tauri::command]
pub fn holdings_import_apply(state: State<App>) -> CommandResult<HoldingsImportDone> {
    let parsed = state
        .lock_inner()?
        .pending_holdings
        .take()
        .ok_or("Choose a holdings file first.")?;
    let backup = state.backup_as(BEFORE_IMPORT, KEEP_BEFORE_IMPORT)?;
    let done = state.with(|s| s.apply_holdings_import(&parsed, now()))?;
    Ok(HoldingsImportDone { backup, done })
}

#[tauri::command]
pub fn holdings_import_cancel(state: State<App>) -> CommandResult<()> {
    state.lock_inner()?.pending_holdings = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::memory().unwrap()
    }

    #[test]
    fn totals_count_the_base_currency_and_name_the_left_out_ones() {
        let s = store();
        let gbp = Currency::GBP;
        let isa = s.add_account("ISA", Wrapper::Isa, now()).unwrap();
        let nre = s
            .add_account("Savings abroad", Wrapper::Cash, now())
            .unwrap();
        s.add_holding(
            &isa.id,
            "World fund",
            AssetClass::Equity,
            Money::new(1_200_000, gbp),
            now(),
        )
        .unwrap();
        s.add_holding(
            &isa.id,
            "Gilts",
            AssetClass::Bonds,
            Money::new(300_000, gbp),
            now(),
        )
        .unwrap();
        for (name, cur) in [
            ("Deposit", Currency::INR),
            ("Second", Currency::INR),
            ("Dirham", Currency::AED),
        ] {
            s.add_holding(
                &nre.id,
                name,
                AssetClass::Cash,
                Money::new(500_000, cur),
                now(),
            )
            .unwrap();
        }
        let v = wealth_view_of(&s).unwrap();
        assert_eq!(v.base, gbp, "no answers yet: GBP");
        assert_eq!(v.totals.total, Money::new(1_500_000, gbp));
        assert_eq!(
            v.totals.by_class[&AssetClass::Equity],
            Money::new(1_200_000, gbp)
        );
        assert_eq!(v.totals.not_counted_no_fx.len(), 3);
        assert_eq!(
            v.left_out,
            [Currency::AED, Currency::INR],
            "each named once"
        );
        let by_name = |n: &str| v.accounts.iter().find(|a| a.account.name == n).unwrap();
        assert_eq!(by_name("ISA").counted, Money::new(1_500_000, gbp));
        assert_eq!(by_name("ISA").not_counted, 0);
        assert_eq!(by_name("Savings abroad").counted, Money::zero(gbp));
        assert_eq!(by_name("Savings abroad").not_counted, 3);
    }

    #[test]
    fn an_import_preview_counts_rows_and_writes_nothing() {
        let s = store();
        let csv = "account,wrapper,holding,assetClass,value,currency\n\
                   ISA,isa,World fund,equity,1200.50,GBP\n\
                   ISA,isa,Gilts,bonds,300,GBP\n\
                   Abroad,cash,Deposit,cash,5000,INR\n\
                   ISA,isa,Odd,spaceships,10,GBP\n";
        let (v, parsed) = import_view_of(&s, csv.as_bytes(), "h.csv").unwrap();
        assert_eq!((v.accounts, v.holdings), (2, 3));
        assert_eq!(v.preview.new_accounts, 2);
        assert_eq!(v.preview.new_holdings, 3);
        assert_eq!(v.preview.not_counted_no_fx, 1);
        assert_eq!(v.preview.problems.len(), 1);
        assert!(
            !v.preview.problems[0].contains("spaceships"),
            "never echoes the file"
        );
        assert!(s.accounts().unwrap().is_empty(), "a preview writes nothing");
        let mut s = s;
        s.apply_holdings_import(&parsed, now()).unwrap();
        let (again, _) = import_view_of(&s, csv.as_bytes(), "h.csv").unwrap();
        assert_eq!(
            again.preview.unchanged_holdings, 3,
            "stable ids: nothing new"
        );
    }

    #[test]
    fn a_file_that_is_not_holdings_is_refused() {
        let s = store();
        assert!(import_view_of(&s, b"", "e.csv").is_err());
        assert!(import_view_of(&s, b"{\"format\":\"something-else\"}", "e.json").is_err());
    }
}
