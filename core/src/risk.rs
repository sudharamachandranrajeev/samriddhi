//! The risk questionnaire, as data. Two parts are kept apart, as UK guidance
//! (FCA FG11/05) asks: *attitude* (how much ups and downs the person is
//! willing to live with, six questions) and *capacity for loss* (how big a
//! fall they could absorb without their life changing, four questions).
//! Scoring is plain arithmetic, so the same answers always give the same
//! result, and the result says in words how it was reached.

use std::collections::BTreeMap;

use jiff::Timestamp;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::store::{Error, Result, Store, iso};

/// Bump when a question or score changes; old answers are then asked again.
pub const VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Part {
    Attitude,
    Capacity,
}

#[derive(Debug, Clone, Serialize)]
pub struct Opt {
    pub id: &'static str,
    pub text: &'static str,
    /// Attitude: 1 (most careful) to 5. Capacity: 0 (low) to 2 (high).
    pub score: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct Question {
    pub id: &'static str,
    pub part: Part,
    pub text: &'static str,
    /// One sentence on why we ask, shown under the question.
    pub why: &'static str,
    pub options: &'static [Opt],
}

const fn o(id: &'static str, text: &'static str, score: u8) -> Opt {
    Opt { id, text, score }
}

pub const QUESTIONS: &[Question] = &[
    Question {
        id: "a1-fall",
        part: Part::Attitude,
        text: "Your investments drop by a fifth in one month. What would you most likely do?",
        why: "What you would do in a fall tells us more than how you feel on a good day.",
        options: &[
            o("sell-all", "Sell everything to stop the loss", 1),
            o("sell-some", "Sell some of it", 2),
            o("wait", "Do nothing and wait", 3),
            o("keep-adding", "Keep paying in as before", 4),
            o("buy-more", "Put more in while prices are low", 5),
        ],
    },
    Question {
        id: "a2-aim",
        part: Part::Attitude,
        text: "Which matters more to you?",
        why: "Safety and growth pull against each other; this shows which way you lean.",
        options: &[
            o(
                "never-lose",
                "Never losing money, even if it barely grows",
                1,
            ),
            o("mostly-safe", "Mostly safe, with a little growth", 2),
            o("balance", "A balance of safety and growth", 3),
            o("mostly-growth", "Mostly growth, accepting some falls", 4),
            o(
                "max-growth",
                "As much growth as possible, whatever the swings",
                5,
            ),
        ],
    },
    Question {
        id: "a3-choice",
        part: Part::Attitude,
        text: "You put money away for a year. Which range of results would you pick?",
        why: "Bigger possible gains come with bigger possible losses; we want the range you would accept.",
        options: &[
            o("none", "Lose nothing, gain up to 3%", 1),
            o("small", "Lose up to 5%, gain up to 10%", 2),
            o("medium", "Lose up to 10%, gain up to 20%", 3),
            o("large", "Lose up to 20%, gain up to 35%", 4),
            o("very-large", "Lose up to 35%, gain up to 60%", 5),
        ],
    },
    Question {
        id: "a4-feel",
        part: Part::Attitude,
        text: "How do you feel when you think about investing?",
        why: "Choices that keep you awake at night are not good choices, however they turn out.",
        options: &[
            o("anxious", "Anxious; I would rather not", 1),
            o("uneasy", "Uneasy, but I know I should", 2),
            o("fine", "Fine, as long as I understand it", 3),
            o("keen", "Keen", 4),
            o("thrilled", "Excited by the chance of big gains", 5),
        ],
    },
    Question {
        id: "a5-below",
        part: Part::Attitude,
        text: "How long could you watch an investment stay below what you paid before it worried you a lot?",
        why: "Most investments spend time below their price; patience is part of the risk you take.",
        options: &[
            o("days", "A few days", 1),
            o("months", "A few months", 2),
            o("year", "About a year", 3),
            o("years", "A few years", 4),
            o("no-worry", "It would not worry me", 5),
        ],
    },
    Question {
        id: "a6-friends",
        part: Part::Attitude,
        text: "How would people who know you describe you with money?",
        why: "Others often see our habits more clearly than we do.",
        options: &[
            o("very-careful", "Very careful", 1),
            o("careful", "Careful", 2),
            o("middle", "Somewhere in the middle", 3),
            o("bold", "Willing to take a chance", 4),
            o("very-bold", "A real risk-taker", 5),
        ],
    },
    Question {
        id: "c1-when",
        part: Part::Capacity,
        text: "When might you need the money you would invest?",
        why: "Money needed soon has no time to recover from a fall.",
        options: &[
            o("under-2", "Within 2 years", 0),
            o("2-to-5", "In 2 to 5 years", 1),
            o("over-5", "Not for more than 5 years", 2),
        ],
    },
    Question {
        id: "c2-hit",
        part: Part::Capacity,
        text: "If that money fell by a third, how would your everyday life change?",
        why: "A loss you can absorb is very different from one that changes how you live.",
        options: &[
            o("hard", "A lot: I would struggle with bills or plans", 0),
            o("tighter", "Things would be tighter for a while", 1),
            o("no-change", "Not at all", 2),
        ],
    },
    Question {
        id: "c3-cushion",
        part: Part::Capacity,
        text: "If your income stopped, how long could you live on your cash?",
        why: "A cash cushion means you never have to sell investments at a bad time.",
        options: &[
            o("under-3", "Less than 3 months", 0),
            o("3-to-6", "3 to 6 months", 1),
            o("over-6", "More than 6 months", 2),
        ],
    },
    Question {
        id: "c4-income",
        part: Part::Capacity,
        text: "How steady is your income?",
        why: "Steady income can refill savings after a fall; uncertain income cannot.",
        options: &[
            o("unsteady", "Uncertain, or none at the moment", 0),
            o("mostly", "Mostly steady", 1),
            o("steady", "Very steady, or from several sources", 2),
        ],
    },
];

/// The capacity answer that on its own means low capacity: money needed
/// within two years cannot wait for a fall to recover.
const NEEDED_SOON: (&str, &str) = ("c1-when", "under-2");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capacity {
    Low,
    Medium,
    High,
}

impl Capacity {
    /// The highest attitude level this capacity allows.
    fn cap(&self) -> u8 {
        match self {
            Capacity::Low => 2,
            Capacity::Medium => 3,
            Capacity::High => 5,
        }
    }

    fn words(&self) -> &'static str {
        match self {
            Capacity::Low => "a fall would hurt your plans or everyday life",
            Capacity::Medium => "you could take a moderate fall",
            Capacity::High => "you could take a large fall without your life changing",
        }
    }
}

/// One answer: a question id and the option picked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub question_id: String,
    pub option_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskProfile {
    /// 1 (very cautious) to 5 (very adventurous).
    pub attitude: u8,
    pub capacity: Capacity,
    /// The level suggestions keep to; see [`score`].
    pub combined: u8,
    pub label: &'static str,
    /// The result in plain words, with how it was reached.
    pub summary: String,
}

pub fn label(level: u8) -> &'static str {
    match level {
        1 => "Very cautious",
        2 => "Cautious",
        3 => "Balanced",
        4 => "Adventurous",
        _ => "Very adventurous",
    }
}

fn question(id: &str) -> Option<&'static Question> {
    QUESTIONS.iter().find(|q| q.id == id)
}

/// Check that every answer names a real question and one of its options,
/// and no question is answered twice.
pub fn validate(answers: &[Answer]) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for a in answers {
        let q = question(&a.question_id)
            .ok_or_else(|| Error::Invalid(format!("no question {:?}", a.question_id)))?;
        if !q.options.iter().any(|o| o.id == a.option_id) {
            return Err(Error::Invalid(format!(
                "{:?} is not an answer to {:?}",
                a.option_id, a.question_id
            )));
        }
        if !seen.insert(a.question_id.as_str()) {
            return Err(Error::Invalid(format!(
                "{:?} answered twice",
                a.question_id
            )));
        }
    }
    Ok(())
}

/// Score a full set of answers; `None` until every question is answered.
///
/// - Attitude: each answer scores 1 to 5; the six add up to 6..=30, cut into
///   five equal bands (6-10 is 1, 11-15 is 2, ... 26-30 is 5).
/// - Capacity: each answer scores 0 to 2; the four add up to 0..=8: 0-2 low,
///   3-5 medium, 6-8 high. Needing the money within two years makes it low
///   whatever the rest says.
/// - Combined: the lower of the two. Capacity is turned into the highest
///   attitude level it allows (low 2, medium 3, high 5) and the result is
///   the smaller of that and the attitude. Someone keen on risk who could
///   not absorb a loss should not be steered into one; someone who could
///   absorb a loss but does not want one should not be pushed either. A
///   suitable plan must fit both what you are willing and what you are able
///   to take, so the stricter one wins.
pub fn score(answers: &[Answer]) -> Option<RiskProfile> {
    validate(answers).ok()?;
    let picked: BTreeMap<&str, &str> = answers
        .iter()
        .map(|a| (a.question_id.as_str(), a.option_id.as_str()))
        .collect();
    let mut attitude_total = 0u32;
    let mut capacity_total = 0u32;
    for q in QUESTIONS {
        let opt = q.options.iter().find(|o| Some(&o.id) == picked.get(q.id))?;
        match q.part {
            Part::Attitude => attitude_total += opt.score as u32,
            Part::Capacity => capacity_total += opt.score as u32,
        }
    }
    let attitude = ((attitude_total.saturating_sub(6)) / 5 + 1).min(5) as u8;
    let mut capacity = match capacity_total {
        0..=2 => Capacity::Low,
        3..=5 => Capacity::Medium,
        _ => Capacity::High,
    };
    if picked.get(NEEDED_SOON.0) == Some(&NEEDED_SOON.1) {
        capacity = Capacity::Low;
    }
    let combined = attitude.min(capacity.cap());
    Some(RiskProfile {
        attitude,
        capacity,
        combined,
        label: label(combined),
        summary: summary(attitude, capacity, combined),
    })
}

fn summary(attitude: u8, capacity: Capacity, combined: u8) -> String {
    let feel = format!(
        "You said you are comfortable with {} risk ({attitude} of 5), and {}.",
        label(attitude).to_lowercase(),
        capacity.words()
    );
    let result = if combined < attitude {
        format!(
            " Because of what you could afford to lose, we keep suggestions at {} ({combined} of 5).",
            label(combined).to_lowercase()
        )
    } else {
        format!(
            " We keep suggestions at {} ({combined} of 5).",
            label(combined).to_lowercase()
        )
    };
    format!("{feel}{result} You can change your answers any time.")
}

impl Store {
    /// Keep the answers given (all or some), replacing earlier ones to the
    /// same questions.
    pub fn save_risk_answers(&mut self, answers: &[Answer], now: Timestamp) -> Result<()> {
        validate(answers)?;
        let tx = self.conn.transaction()?;
        for a in answers {
            tx.execute(
                "INSERT INTO sam_risk_answers (question_id, option_id, version, answered_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(question_id) DO UPDATE SET option_id = ?2, version = ?3, answered_at = ?4",
                params![a.question_id, a.option_id, VERSION, iso(now)],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Answers given to this version of the questions.
    pub fn risk_answers(&self) -> Result<Vec<Answer>> {
        let mut st = self.conn.prepare(
            "SELECT question_id, option_id FROM sam_risk_answers WHERE version = ? ORDER BY question_id",
        )?;
        let rows = st.query_map([VERSION], |r| {
            Ok(Answer {
                question_id: r.get(0)?,
                option_id: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The result, once every question has an answer.
    pub fn risk_profile(&self) -> Result<Option<RiskProfile>> {
        Ok(score(&self.risk_answers()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::{NOW, at};

    /// Pick the option at `index` for every question of `part`.
    fn all(part: Part, pick: impl Fn(&Question) -> usize) -> Vec<Answer> {
        QUESTIONS
            .iter()
            .filter(|q| q.part == part)
            .map(|q| Answer {
                question_id: q.id.into(),
                option_id: q.options[pick(q)].id.into(),
            })
            .collect()
    }

    fn answers(attitude: usize, capacity: usize) -> Vec<Answer> {
        let mut a = all(Part::Attitude, |_| attitude);
        a.extend(all(Part::Capacity, |_| capacity));
        a
    }

    #[test]
    fn the_questionnaire_has_six_plus_four_questions_each_with_a_reason() {
        let count = |p| QUESTIONS.iter().filter(|q| q.part == p).count();
        assert_eq!((count(Part::Attitude), count(Part::Capacity)), (6, 4));
        let mut ids: Vec<_> = QUESTIONS.iter().map(|q| q.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), QUESTIONS.len());
        for q in QUESTIONS {
            assert!(q.why.ends_with('.') && q.why.len() < 120, "{}", q.id);
            assert!(q.text.ends_with('?'), "{}", q.id);
            let scores: Vec<u8> = q.options.iter().map(|o| o.score).collect();
            let want: Vec<u8> = match q.part {
                Part::Attitude => vec![1, 2, 3, 4, 5],
                Part::Capacity => vec![0, 1, 2],
            };
            assert_eq!(scores, want, "{} scores rise one step at a time", q.id);
        }
    }

    #[test]
    fn scores_attitude_in_five_bands() {
        // Every option at the same index: totals 6, 12, 18, 24, 30.
        for (i, want) in [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)] {
            assert_eq!(score(&answers(i, 2)).unwrap().attitude, want);
        }
        // Band edges: 10 -> 1, 11 -> 2, 25 -> 4, 26 -> 5.
        let edge = |total_extra: usize| {
            let mut left = total_extra;
            let mut a = all(Part::Attitude, |_| 0);
            for ans in a.iter_mut() {
                let q = question(&ans.question_id).unwrap();
                let step = left.min(4);
                ans.option_id = q.options[step].id.into();
                left -= step;
            }
            a.extend(all(Part::Capacity, |_| 2));
            score(&a).unwrap().attitude
        };
        assert_eq!(edge(4), 1);
        assert_eq!(edge(5), 2);
        assert_eq!(edge(19), 4);
        assert_eq!(edge(20), 5);
    }

    #[test]
    fn capacity_bands_and_money_needed_soon() {
        assert_eq!(score(&answers(4, 0)).unwrap().capacity, Capacity::Low);
        assert_eq!(score(&answers(4, 1)).unwrap().capacity, Capacity::Medium);
        assert_eq!(score(&answers(4, 2)).unwrap().capacity, Capacity::High);
        // Everything high, but the money is needed within two years.
        let mut a = answers(4, 2);
        a.iter_mut()
            .find(|x| x.question_id == NEEDED_SOON.0)
            .unwrap()
            .option_id = NEEDED_SOON.1.into();
        assert_eq!(score(&a).unwrap().capacity, Capacity::Low);
    }

    #[test]
    fn the_lower_of_attitude_and_capacity_wins() {
        let keen_but_fragile = score(&answers(4, 0)).unwrap();
        assert_eq!(
            (keen_but_fragile.attitude, keen_but_fragile.combined),
            (5, 2)
        );
        assert_eq!(keen_but_fragile.label, "Cautious");
        assert!(
            keen_but_fragile
                .summary
                .contains("Because of what you could afford to lose"),
            "{}",
            keen_but_fragile.summary
        );
        let careful_but_able = score(&answers(0, 2)).unwrap();
        assert_eq!(
            (careful_but_able.attitude, careful_but_able.combined),
            (1, 1)
        );
        assert!(!careful_but_able.summary.contains("Because"));
        assert!(
            careful_but_able
                .summary
                .ends_with("You can change your answers any time.")
        );
        let middle = score(&answers(3, 1)).unwrap();
        assert_eq!((middle.attitude, middle.combined), (4, 3));
        // Same answers, same result.
        assert_eq!(score(&answers(3, 1)), Some(middle));
    }

    #[test]
    fn incomplete_or_wrong_answers_give_no_result() {
        let mut a = answers(2, 1);
        a.pop();
        assert_eq!(score(&a), None);
        let mut wrong = answers(2, 1);
        wrong[0].option_id = "made-up".into();
        assert!(validate(&wrong).is_err());
        assert_eq!(score(&wrong), None);
        let mut twice = answers(2, 1);
        twice.push(twice[0].clone());
        assert!(validate(&twice).is_err());
        assert!(
            validate(&[Answer {
                question_id: "q99".into(),
                option_id: "x".into()
            }])
            .is_err()
        );
    }

    #[test]
    fn answers_are_kept_and_replaced() {
        let mut s = Store::memory().unwrap();
        assert_eq!(s.risk_profile().unwrap(), None);
        s.save_risk_answers(&answers(0, 0), at(NOW)).unwrap();
        s.save_risk_answers(&answers(2, 2)[..3], at(NOW)).unwrap();
        assert_eq!(s.risk_answers().unwrap().len(), QUESTIONS.len());
        s.save_risk_answers(&answers(2, 2), at(NOW)).unwrap();
        assert_eq!(s.risk_profile().unwrap().unwrap().combined, 3);
        assert!(
            s.save_risk_answers(
                &[answers(0, 0)[0].clone(), answers(1, 0)[0].clone()],
                at(NOW)
            )
            .is_err()
        );
    }
}
