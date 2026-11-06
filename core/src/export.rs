//! Full export and import. The export is everything the user has told the
//! app, readable without Samriddhi: one JSON file, and one CSV file per
//! table for spreadsheets. Importing reads the JSON back after showing what
//! it holds (a preview of counts), and replaces the profile in one
//! transaction: all of it or nothing.
//!
//! Two things never leave the device: the app-lock PIN hash (a short PIN is
//! quick to guess from its hash, and people reuse PINs) and this device's
//! own lock and recovery flags.

use jiff::Timestamp;
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Transaction, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::profile::GoalKind;
use crate::risk::{self, Answer};
use crate::store::{Action, Error, Result, Store, TABLES, iso};

pub const FORMAT: &str = "samriddhi-export";
pub const FORMAT_VERSION: i64 = 1;

/// Tables an import replaces. `sam_meta` describes this database, and the
/// audit log is this device's history: both are kept.
pub const IMPORT_TABLES: &[&str] = &[
    "sam_settings",
    "sam_profile",
    "sam_countries",
    "sam_goals",
    "sam_risk_answers",
    "sam_steps_dismissed",
];

/// Settings that stay on this device.
pub fn private_setting(key: &str) -> bool {
    key.starts_with("lock.") || key == "recovery.saved"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub format: String,
    pub version: i64,
    pub schema: i64,
    pub exported_at: String,
    /// Table name to a list of rows, each row column name to value.
    pub tables: Map<String, Value>,
}

impl Export {
    pub fn from_json(text: &str) -> Result<Export> {
        Ok(serde_json::from_str(text)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCount {
    pub table: String,
    /// Rows here now.
    pub current: i64,
    /// Rows the import would put in their place.
    pub incoming: i64,
}

/// What an import would do, shown before it does it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub exported_at: String,
    pub schema: i64,
    pub tables: Vec<TableCount>,
    /// Tables in the file that are kept as they are here.
    pub kept: Vec<String>,
}

/// One table: its column names and rows.
struct Dump {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

impl Store {
    fn dump(&self, table: &str) -> Result<Dump> {
        // Table names come from TABLES, never from outside.
        let mut st = self.conn.prepare(&format!("SELECT * FROM {table}"))?;
        let columns: Vec<String> = st.column_names().iter().map(|c| c.to_string()).collect();
        let n = columns.len();
        let mut rows = Vec::new();
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let mut row = Vec::with_capacity(n);
            for (i, column) in columns.iter().enumerate() {
                row.push(match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => Value::from(v),
                    ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Real(_) | ValueRef::Blob(_) => {
                        return Err(Error::Invalid(format!(
                            "{table}.{column} holds a value exports cannot carry"
                        )));
                    }
                });
            }
            let private = table == "sam_settings" && row[0].as_str().is_some_and(private_setting);
            if !private {
                rows.push(row);
            }
        }
        Ok(Dump { columns, rows })
    }

    /// Everything, as one value ready to write as JSON. Notes the export in
    /// the audit log.
    pub fn export_all(&self, now: Timestamp) -> Result<Export> {
        let mut tables = Map::new();
        for table in TABLES {
            let d = self.dump(table)?;
            let rows: Vec<Value> = d
                .rows
                .into_iter()
                .map(|row| Value::Object(d.columns.iter().cloned().zip(row).collect()))
                .collect();
            tables.insert((*table).to_owned(), Value::Array(rows));
        }
        self.audit("local", Action::Read, "export", now)?;
        Ok(Export {
            format: FORMAT.into(),
            version: FORMAT_VERSION,
            schema: self.schema_version()?,
            exported_at: iso(now),
            tables,
        })
    }

    /// One CSV file per table: (file name, contents).
    pub fn export_csv(&self, now: Timestamp) -> Result<Vec<(String, String)>> {
        let files = TABLES
            .iter()
            .map(|table| {
                let d = self.dump(table)?;
                let mut out = csv_line(d.columns.iter().map(|c| Value::String(c.clone())));
                for row in d.rows {
                    out.push_str(&csv_line(row.into_iter()));
                }
                Ok((format!("{table}.csv"), out))
            })
            .collect::<Result<_>>()?;
        self.audit("local", Action::Read, "export csv", now)?;
        Ok(files)
    }

    /// Check an export and say what importing it would change. Nothing is
    /// written.
    pub fn preview_import(&self, export: &Export) -> Result<ImportPreview> {
        if export.format != FORMAT {
            return Err(Error::Invalid("this is not a Samriddhi export".into()));
        }
        if export.version > FORMAT_VERSION || export.schema > self.schema_version()? {
            return Err(Error::Invalid(
                "this export was made by a newer Samriddhi; update the app first".into(),
            ));
        }
        let mut tables = Vec::new();
        let mut kept = Vec::new();
        for (name, rows) in &export.tables {
            if !TABLES.contains(&name.as_str()) {
                return Err(Error::Invalid(format!("unknown table {name:?}")));
            }
            let rows = rows
                .as_array()
                .ok_or_else(|| Error::Invalid(format!("{name} is not a list of rows")))?;
            if !IMPORT_TABLES.contains(&name.as_str()) {
                kept.push(name.clone());
                continue;
            }
            let columns = self.columns(name)?;
            for row in rows {
                let obj = row
                    .as_object()
                    .ok_or_else(|| Error::Invalid(format!("a row of {name} is not an object")))?;
                if let Some(c) = obj.keys().find(|c| !columns.contains(c)) {
                    return Err(Error::Invalid(format!("{name} has no column {c:?}")));
                }
                check_row(name, obj)?;
            }
            let current: i64 =
                self.conn
                    .query_row(&format!("SELECT count(*) FROM {name}"), [], |r| r.get(0))?;
            tables.push(TableCount {
                table: name.clone(),
                current,
                incoming: rows.len() as i64,
            });
        }
        Ok(ImportPreview {
            exported_at: export.exported_at.clone(),
            schema: export.schema,
            tables,
            kept,
        })
    }

    /// Replace the profile with the export's, in one transaction. Make a
    /// backup first: this cannot be undone.
    pub fn import(&mut self, export: &Export, now: Timestamp) -> Result<ImportPreview> {
        let preview = self.preview_import(export)?;
        let mut columns = Vec::new();
        for t in &preview.tables {
            columns.push((t.table.clone(), self.columns(&t.table)?));
        }
        let tx = self.conn.transaction()?;
        for (table, cols) in &columns {
            if table == "sam_settings" {
                // This device's PIN and lock stay as they are.
                tx.execute(
                    "DELETE FROM sam_settings WHERE key NOT LIKE 'lock.%' AND key != 'recovery.saved'",
                    [],
                )?;
            } else {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
            for row in export.tables[table].as_array().into_iter().flatten() {
                insert(&tx, table, cols, row)?;
            }
        }
        tx.execute(
            "INSERT INTO sam_audit (at, actor, action, what) VALUES (?, 'local', 'write', 'import')",
            [iso(now)],
        )?;
        tx.commit()?;
        Ok(preview)
    }

    fn columns(&self, table: &str) -> Result<Vec<String>> {
        let mut st = self
            .conn
            .prepare("SELECT name FROM pragma_table_info(?) ORDER BY cid")?;
        let rows = st.query_map([table], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// Check what the database's own constraints cannot: facts have the right
/// shape, answers fit the questions, goals are known kinds.
fn check_row(table: &str, row: &Map<String, Value>) -> Result<()> {
    let text = |c: &str| row.get(c).and_then(Value::as_str).unwrap_or_default();
    match table {
        "sam_profile" => {
            let value: Value = serde_json::from_str(text("json"))?;
            crate::profile::check(text("key"), &value)
        }
        "sam_risk_answers" => risk::validate(&[Answer {
            question_id: text("question_id").into(),
            option_id: text("option_id").into(),
        }]),
        "sam_goals" => serde_json::from_value::<GoalKind>(Value::String(text("kind").into()))
            .map(drop)
            .map_err(|_| Error::Invalid(format!("unknown goal {:?}", text("kind")))),
        _ => Ok(()),
    }
}

fn insert(tx: &Transaction, table: &str, columns: &[String], row: &Value) -> Result<()> {
    let obj = row
        .as_object()
        .ok_or_else(|| Error::Invalid(format!("a row of {table} is not an object")))?;
    if table == "sam_settings"
        && obj
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(private_setting)
    {
        return Ok(());
    }
    // Only columns the table has (checked in the preview), quoted.
    let names: Vec<&String> = columns.iter().filter(|c| obj.contains_key(*c)).collect();
    let values = names
        .iter()
        .map(|c| sql_value(&obj[c.as_str()]))
        .collect::<Result<Vec<_>>>()?;
    let list: Vec<String> = names.iter().map(|c| format!("\"{c}\"")).collect();
    let marks = vec!["?"; names.len()].join(", ");
    tx.execute(
        &format!("INSERT INTO {table} ({}) VALUES ({marks})", list.join(", ")),
        params_from_iter(values),
    )?;
    Ok(())
}

fn sql_value(v: &Value) -> Result<SqlValue> {
    Ok(match v {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Number(n) => SqlValue::Integer(
            n.as_i64()
                .ok_or_else(|| Error::Invalid(format!("{n} is not a whole number")))?,
        ),
        _ => return Err(Error::Invalid("a column holds a list or object".into())),
    })
}

/// One CSV line (RFC 4180, CRLF). Text that a spreadsheet would run as a
/// formula gets a leading apostrophe.
fn csv_line(fields: impl Iterator<Item = Value>) -> String {
    let cells: Vec<String> = fields
        .map(|v| match v {
            Value::Null => String::new(),
            Value::String(s) => {
                let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                    format!("'{s}")
                } else {
                    s
                };
                if s.contains([',', '"', '\n', '\r']) {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s
                }
            }
            other => other.to_string(),
        })
        .collect();
    format!("{}\r\n", cells.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::{Currency, Money};
    use crate::profile::{self, CountryTie, Goal, Source};
    use crate::store::tests::{NOW, at};
    use serde_json::json;

    /// A store with one of everything; synthetic data only.
    fn filled() -> Store {
        let mut s = Store::memory().unwrap();
        let now = at(NOW);
        let gbp = |m| serde_json::to_value(Money::new(m, Currency::GBP)).unwrap();
        s.set_fact(profile::COSTS, &gbp(150000), Source::Wizard, now)
            .unwrap();
        s.set_fact(profile::CASH, &gbp(90000), Source::Wizard, now)
            .unwrap();
        s.set_countries(
            &[CountryTie {
                code: "GB".into(),
                residence: true,
                tax_home: true,
                citizen: false,
                holds_money: true,
                ties: "Works here, \"mostly\"".into(),
            }],
            Source::Wizard,
            now,
        )
        .unwrap();
        s.set_goals(
            &[Goal {
                kind: profile::GoalKind::EmergencyCushion,
                target: None,
                target_date: None,
            }],
            Source::Wizard,
            now,
        )
        .unwrap();
        s.save_risk_answers(
            &[Answer {
                question_id: "a1-fall".into(),
                option_id: "wait".into(),
            }],
            now,
        )
        .unwrap();
        s.step_not_for_me("cushion", "=SUM(A1)", now).unwrap();
        s.set_setting("ui.theme", &"vana").unwrap();
        s.set_setting("lock.pin", &"$argon2id$fake").unwrap();
        s.set_setting("recovery.saved", &true).unwrap();
        s
    }

    #[test]
    fn exports_every_table_but_never_the_pin() {
        let s = filled();
        let e = s.export_all(at(NOW)).unwrap();
        assert_eq!((e.format.as_str(), e.version), (FORMAT, 1));
        assert_eq!(e.exported_at, "2026-11-04T09:00:00Z");
        let names: Vec<&str> = e.tables.keys().map(String::as_str).collect();
        assert_eq!(names, TABLES);
        let text = serde_json::to_string(&e).unwrap();
        assert!(!text.contains("argon2") && !text.contains("recovery.saved"));
        assert!(text.contains("ui.theme"));
        assert_eq!(e.tables["sam_profile"].as_array().unwrap().len(), 2);
        let row = &e.tables["sam_countries"][0];
        assert_eq!(row["code"], "GB");
        assert_eq!(row["tax_home"], 1);
        // The export is in the audit log.
        assert_eq!(s.audit_log(1).unwrap()[0].3, "export");
    }

    #[test]
    fn csv_has_a_header_quotes_and_no_formulas() {
        let s = filled();
        let files = s.export_csv(at(NOW)).unwrap();
        assert_eq!(files.len(), TABLES.len());
        let (name, body) = files
            .iter()
            .find(|(n, _)| n == "sam_countries.csv")
            .unwrap();
        assert_eq!(name, "sam_countries.csv");
        let mut lines = body.split("\r\n");
        assert_eq!(
            lines.next().unwrap(),
            "code,residence,tax_home,citizen,holds_money,ties,source,why,updated_at"
        );
        assert!(
            lines
                .next()
                .unwrap()
                .starts_with("GB,1,1,0,1,\"Works here, \"\"mostly\"\"\",wizard,")
        );
        let (_, dismissed) = files
            .iter()
            .find(|(n, _)| n == "sam_steps_dismissed.csv")
            .unwrap();
        assert!(dismissed.contains(",'=SUM(A1),"), "{dismissed}");
        let (_, settings) = files.iter().find(|(n, _)| n == "sam_settings.csv").unwrap();
        assert!(!settings.contains("lock.pin"));
        // An empty table still has its header.
        let empty = Store::memory().unwrap().export_csv(at(NOW)).unwrap();
        assert_eq!(empty[2].1, "key,json,source,asked_at,why\r\n");
    }

    #[test]
    fn import_previews_counts_then_replaces_everything() {
        let source = filled();
        let e = source.export_all(at(NOW)).unwrap();
        let json = serde_json::to_string_pretty(&e).unwrap();
        let mut target = Store::memory().unwrap();
        target.set_setting("lock.pin", &"$argon2id$mine").unwrap();
        target.set_setting("ui.theme", &"swarna").unwrap();
        let back = Export::from_json(&json).unwrap();
        let p = target.preview_import(&back).unwrap();
        let count = |t: &str| p.tables.iter().find(|c| c.table == t).unwrap().clone();
        assert_eq!(
            count("sam_profile"),
            TableCount {
                table: "sam_profile".into(),
                current: 0,
                incoming: 2
            }
        );
        assert_eq!(count("sam_settings").current, 2);
        assert_eq!(p.kept, ["sam_meta", "sam_audit"]);
        assert!(
            target.facts().unwrap().is_empty(),
            "a preview writes nothing"
        );

        target.import(&back, at(NOW)).unwrap();
        assert_eq!(target.profile().unwrap(), source.profile().unwrap());
        assert_eq!(target.dismissals().unwrap(), source.dismissals().unwrap());
        assert_eq!(
            target.setting::<String>("ui.theme").unwrap().as_deref(),
            Some("vana")
        );
        assert_eq!(
            target.setting::<String>("lock.pin").unwrap().as_deref(),
            Some("$argon2id$mine"),
            "this device keeps its own PIN"
        );
        assert_eq!(target.audit_log(1).unwrap()[0].3, "import");
        // Importing twice gives the same result, not doubles.
        target.import(&back, at(NOW)).unwrap();
        assert_eq!(target.countries().unwrap().len(), 1);
    }

    #[test]
    fn a_bad_export_is_refused_and_changes_nothing() {
        let source = filled();
        let good = source.export_all(at(NOW)).unwrap();
        let mut target = filled();
        let before = target.profile().unwrap();
        let refused = |e: &Export, target: &mut Store| {
            assert!(target.preview_import(e).is_err());
            assert!(target.import(e, at(NOW)).is_err());
        };
        let mut e = good.clone();
        e.format = "something-else".into();
        refused(&e, &mut target);
        let mut e = good.clone();
        e.schema = 99;
        refused(&e, &mut target);
        let mut e = good.clone();
        e.tables.insert("sqlite_master".into(), json!([]));
        refused(&e, &mut target);
        let mut e = good.clone();
        e.tables["sam_goals"][0]["kind\"; DROP TABLE sam_goals; --"] = json!(1);
        refused(&e, &mut target);
        let mut e = good.clone();
        e.tables["sam_profile"][0]["json"] = json!("{\"minor\":-5,\"currency\":\"GBP\"}");
        refused(&e, &mut target);
        let mut e = good.clone();
        e.tables["sam_goals"][0]["kind"] = json!("winTheLottery");
        refused(&e, &mut target);
        let mut e = good.clone();
        e.tables["sam_risk_answers"][0]["option_id"] = json!("panic");
        refused(&e, &mut target);
        // A row the database itself refuses (two tax homes) rolls back the lot.
        let mut e = good.clone();
        let mut second = e.tables["sam_countries"][0].clone();
        second["code"] = json!("IN");
        e.tables["sam_countries"]
            .as_array_mut()
            .unwrap()
            .push(second);
        assert!(target.preview_import(&e).is_ok());
        assert!(target.import(&e, at(NOW)).is_err());
        assert_eq!(target.profile().unwrap(), before);
        assert!(Export::from_json("not json").is_err());
    }
}
