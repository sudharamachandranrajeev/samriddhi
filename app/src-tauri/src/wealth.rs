//! The profile, risk questions and next steps, as the screens ask for them.
//! Every rule lives in the core; these commands only check who may call
//! them and pass the time in.

use samriddhi_core::catalogue::Catalogue;
use samriddhi_core::money::Money;
use samriddhi_core::profile::{CountryTie, Fact, Goal, Profile, Source};
use samriddhi_core::risk::{self, Answer, Question, RiskProfile};
use samriddhi_core::steps::{self, Step};
use samriddhi_core::store::Action;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::{App, CommandResult, now};

/// The screens give facts from the wizard or a later question; only the app
/// itself works facts out.
fn from_screen(source: Source) -> CommandResult<Source> {
    match source {
        Source::Derived => Err("Worked-out facts are set by the app, not a screen.".into()),
        s => Ok(s),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    profile: Profile,
    /// Each stored fact with where it came from and why it was asked.
    facts: Vec<Fact>,
    /// Cash minus debts, "rough, from your answers".
    rough_net_worth: Option<Money>,
}

#[tauri::command]
pub fn profile_get(state: State<App>) -> CommandResult<ProfileView> {
    state.with(|s| {
        let profile = s.profile()?;
        Ok(ProfileView {
            rough_net_worth: profile.rough_net_worth(),
            facts: s.facts()?,
            profile,
        })
    })
}

#[tauri::command]
pub fn set_fact(state: State<App>, key: String, value: Value, source: Source) -> CommandResult<()> {
    let source = from_screen(source)?;
    state.with(|s| {
        s.set_fact(&key, &value, source, now())?;
        s.audit("local", Action::Write, &format!("profile {key}"), now())
    })
}

#[tauri::command]
pub fn clear_fact(state: State<App>, key: String) -> CommandResult<()> {
    state.with(|s| s.clear_fact(&key))
}

#[tauri::command]
pub fn set_countries(
    state: State<App>,
    countries: Vec<CountryTie>,
    source: Source,
) -> CommandResult<()> {
    let source = from_screen(source)?;
    state.with(|s| s.set_countries(&countries, source, now()))
}

#[tauri::command]
pub fn set_goals(state: State<App>, goals: Vec<Goal>, source: Source) -> CommandResult<()> {
    let source = from_screen(source)?;
    state.with(|s| s.set_goals(&goals, source, now()))
}

/// The questionnaire as data; it needs no unlock (it holds no answers).
#[tauri::command]
pub fn risk_questions() -> &'static [Question] {
    risk::QUESTIONS
}

#[tauri::command]
pub fn save_risk_answers(
    state: State<App>,
    answers: Vec<Answer>,
) -> CommandResult<Option<RiskProfile>> {
    state.with(|s| {
        s.save_risk_answers(&answers, now())?;
        s.risk_profile()
    })
}

#[tauri::command]
pub fn risk_result(state: State<App>) -> CommandResult<Option<RiskProfile>> {
    state.with(|s| s.risk_profile())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepsView {
    steps: Vec<Step>,
    /// A rule is past its review date: say the facts may be out of date.
    review_due: bool,
}

#[tauri::command]
pub fn steps_list(state: State<App>) -> CommandResult<StepsView> {
    let cat = Catalogue::builtin();
    let t = now();
    state.with(|s| {
        Ok(StepsView {
            steps: s.next_steps(t, &cat)?,
            review_due: cat.needs_review(steps::today(t)),
        })
    })
}

#[tauri::command]
pub fn step_later(state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| s.step_later(&id, now()))
}

#[tauri::command]
pub fn step_not_for_me(state: State<App>, id: String, reason: String) -> CommandResult<()> {
    state.with(|s| s.step_not_for_me(&id, &reason, now()))
}

#[tauri::command]
pub fn step_restore(state: State<App>, id: String) -> CommandResult<()> {
    state.with(|s| s.step_restore(&id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_cannot_claim_a_fact_was_worked_out() {
        assert_eq!(from_screen(Source::Wizard), Ok(Source::Wizard));
        assert_eq!(from_screen(Source::Asked), Ok(Source::Asked));
        assert!(from_screen(Source::Derived).is_err());
    }
}
