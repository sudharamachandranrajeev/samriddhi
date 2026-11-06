//! The log of what the user decided about suggested steps: accepted or
//! dismissed, by which producer, when. Accepting records intent only; the
//! app never acts on it. One row per decision, so a step accepted twice
//! counts twice (the behaviour nudge reads this).

use jiff::Timestamp;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::store::{Error, Result, Store, iso};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Accepted,
    Dismissed,
}

impl Decision {
    fn as_str(&self) -> &'static str {
        match self {
            Decision::Accepted => "accepted",
            Decision::Dismissed => "dismissed",
        }
    }

    fn parse(s: &str) -> Result<Decision> {
        match s {
            "accepted" => Ok(Decision::Accepted),
            "dismissed" => Ok(Decision::Dismissed),
            _ => Err(Error::Invalid(format!("odd decision {s:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepDecision {
    pub step_id: String,
    /// The rule set or agent that suggested it ("steps", "rebalancing"...).
    pub source: String,
    /// The step's title as it was shown.
    pub title: String,
    pub status: Decision,
    /// UTC ISO time.
    pub decided_at: String,
}

fn short(s: &str, max: usize, what: &str) -> Result<String> {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > max {
        return Err(Error::Invalid(format!("odd {what}")));
    }
    Ok(t.to_owned())
}

impl Store {
    /// Note a decision about a step.
    pub fn log_step_decision(
        &self,
        step_id: &str,
        source: &str,
        title: &str,
        status: Decision,
        now: Timestamp,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sam_step_log (step_id, source, title, status, decided_at) VALUES (?, ?, ?, ?, ?)",
            params![
                short(step_id, 100, "step id")?,
                short(source, 60, "step source")?,
                short(title, 200, "step title")?,
                status.as_str(),
                iso(now)
            ],
        )?;
        Ok(())
    }

    /// Decisions at or after `since`, oldest first.
    pub fn step_log_since(&self, since: Timestamp) -> Result<Vec<StepDecision>> {
        let mut st = self.conn.prepare(
            "SELECT step_id, source, title, status, decided_at FROM sam_step_log
             WHERE decided_at >= ? ORDER BY decided_at, seq",
        )?;
        let rows = st.query_map([iso(since)], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (step_id, source, title, status, decided_at) = row?;
            Ok(StepDecision {
                step_id,
                source,
                title,
                status: Decision::parse(&status)?,
                decided_at,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::{NOW, at};
    use jiff::SignedDuration;

    #[test]
    fn decisions_are_appended_and_read_back_by_time() {
        let s = Store::memory().unwrap();
        let t0 = at(NOW);
        let day = SignedDuration::from_hours(24);
        s.log_step_decision(
            "rebalance",
            "rebalancing",
            "Top up bonds",
            Decision::Accepted,
            t0,
        )
        .unwrap();
        s.log_step_decision(
            "rebalance",
            "rebalancing",
            "Top up bonds",
            Decision::Accepted,
            t0 + day,
        )
        .unwrap();
        s.log_step_decision(
            "cushion",
            "steps",
            "Build a cushion",
            Decision::Dismissed,
            t0 + day * 2,
        )
        .unwrap();
        let all = s.step_log_since(t0).unwrap();
        assert_eq!(all.len(), 3, "the same step twice is two rows");
        assert_eq!(all[0].decided_at, "2026-11-04T09:00:00Z");
        assert_eq!(all[2].status, Decision::Dismissed);
        assert_eq!(s.step_log_since(t0 + day).unwrap().len(), 2);
        assert!(
            s.log_step_decision(" ", "steps", "x", Decision::Accepted, t0)
                .is_err()
        );
        assert!(
            s.log_step_decision("x", "steps", &"t".repeat(201), Decision::Accepted, t0)
                .is_err()
        );
    }
}
