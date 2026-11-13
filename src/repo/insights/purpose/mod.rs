//! Review by purpose (#191): a pull request's changes grouped by what they are for (API, logic,
//! tests, configuration, generated files…) by explainable rules, which the reviewer can rename,
//! reorder, split, merge and move hunks between. Every hunk is in exactly one group and opens
//! in the pull request's own diff at its revisions, so comments and review state are where
//! they always are. The standard order stays a click away.
//!
//! An arrangement is saved with the base and head it was made at: once the pull request moves
//! (a force push, new commits) it is stale, said so, and only carried over when asked.

mod arrange;
mod panel;
mod propose;
mod rows;
mod rules;
mod store;
mod units;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use arrange::{Arranged, Edit, Saved};
use propose::Proposal;
use units::Units;

use super::InsightsAct;
use crate::repo::{Action, DiffKey, RepoTab};

/// The pull request read, at which revisions.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Subject {
    url: String,
    base: String,
    head: String,
}

/// What the tab keeps of the grouping.
#[derive(Default)]
pub(crate) struct State {
    /// The files of a pull request are shown by purpose, not in the standard order.
    on: bool,
    /// What was asked for last.
    asked: Option<Subject>,
    request: u64,
    units: Option<Result<Arc<Units>, String>>,
    saved: Option<Saved>,
    proposal: Option<Proposal>,
    arranged: Option<Result<Arranged, String>>,
    /// What the file folding rules knew when the proposal was made.
    generation: u64,
    /// Units selected (by key), to split off together.
    selected: BTreeSet<String>,
    /// What the last change made by hand said, if anything.
    notice: Option<(String, bool)>,
    /// The saves sent, and the last one written: an earlier one never overwrites a later one.
    sent: u64,
    written: Arc<Mutex<u64>>,
}

/// What the grouping asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// By purpose, or the standard order.
    Show(bool),
    /// Read the pull request shown.
    Load,
    /// Propose again (the file folding rules learned more).
    Refresh,
    Edit(Edit),
    Select(String, bool),
    /// Bring a stale arrangement to the revisions shown.
    CarryOver,
    /// Forget the arrangement: the rules' groups again.
    Reset,
    /// Scroll the diff of `key` to a line (old side or not, number) once it is open.
    Jump { key: DiffKey, at: (bool, u32) },
}

pub(super) fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::Purpose(a))
}

impl RepoTab {
    /// The pull request shown, and its files as read, when both are current.
    fn purpose_subject(&self) -> Option<(Subject, crate::pull_request::PrFiles)> {
        let view = self.pr_view.as_ref()?;
        let details = view.details.as_ref()?.as_ref().ok()?;
        let files = view.files.as_ref()?.as_ref().ok()?;
        (files.to == details.head_oid).then(|| {
            (
                Subject {
                    url: details.url.clone(),
                    base: files.from.clone(),
                    head: files.to.clone(),
                },
                files.clone(),
            )
        })
    }

    pub(super) fn purpose_act(&mut self, a: Act, ctx: &egui::Context) {
        let state = &mut self.review_purpose;
        match a {
            Act::Show(on) => state.on = on,
            Act::Load => self.purpose_load(ctx),
            Act::Refresh => self.purpose_refresh(ctx),
            Act::Select(key, on) => {
                if on {
                    state.selected.insert(key);
                } else {
                    state.selected.remove(&key);
                }
            }
            Act::Edit(edit) => {
                let (Some(Ok(units)), Some(Ok(arranged))) = (&state.units, &state.arranged) else {
                    return;
                };
                match arrange::edit(arranged, units, edit) {
                    Ok(saved) => {
                        state.selected.clear();
                        state.notice = None;
                        self.purpose_keep(Some(saved), ctx);
                    }
                    Err(e) => state.notice = Some((e, true)),
                }
            }
            Act::CarryOver => {
                let (Some(Ok(units)), Some(saved)) = (&state.units, &state.saved) else {
                    return;
                };
                let (saved, dropped) = arrange::carry_over(saved, units);
                state.notice = Some((
                    match dropped {
                        0 => "Your grouping is carried over: every change you placed is still there".to_owned(),
                        n => format!("Your grouping is carried over; {n} changes you placed changed since and are where the rules put them"),
                    },
                    false,
                ));
                self.purpose_keep(Some(saved), ctx);
            }
            Act::Reset => {
                state.notice = None;
                self.purpose_keep(None, ctx);
            }
            Act::Jump { key, at } => {
                let Some(view) = self.diff.as_mut().filter(|d| d.key == key) else {
                    return;
                };
                match &view.data {
                    Some(Ok(data)) => view.jump_line = crate::repo::state::line_at(&data.lines, at),
                    Some(Err(_)) => {}
                    None => view.pending_line = Some(at),
                }
            }
        }
    }

    /// Reads the units of the pull request shown, and the arrangement saved for it.
    fn purpose_load(&mut self, ctx: &egui::Context) {
        let Some((subject, files)) = self.purpose_subject() else {
            return;
        };
        let state = &mut self.review_purpose;
        state.request += 1;
        let request = state.request;
        state.asked = Some(subject.clone());
        state.units = None;
        state.arranged = None;
        state.selected.clear();
        state.notice = None;
        let repo = self.path.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                let units = units::read(&repo, &subject.base, &subject.head, &files.files)?;
                let saved = store::load(&repo, &subject.url);
                Ok((units, saved))
            },
            move |tab, result: Result<(Units, Result<Option<Saved>, String>), String>, ctx| {
                let state = &mut tab.review_purpose;
                if state.request != request {
                    return;
                }
                match result {
                    Ok((units, saved)) => {
                        state.units = Some(Ok(Arc::new(units)));
                        match saved {
                            Ok(saved) => state.saved = saved,
                            Err(e) => {
                                state.saved = None;
                                state.notice = Some((e, true));
                            }
                        }
                    }
                    Err(e) => state.units = Some(Err(e)),
                }
                tab.purpose_refresh(ctx);
            },
        );
    }

    /// Proposes the groups again, and applies the arrangement to them.
    fn purpose_refresh(&mut self, ctx: &egui::Context) {
        let settings = crate::settings::Settings::get(ctx);
        let rules = crate::repo::folding::Rules::new(true, &settings.fold_patterns);
        let generation = self.folding.generation;
        let Some(Ok(units)) = &self.review_purpose.units else {
            return;
        };
        let generated: HashMap<&str, String> = units
            .units
            .iter()
            .filter_map(|u| {
                let reason = self.folding.reason(&rules, &u.path)?;
                Some((u.path.as_str(), reason.label().to_owned()))
            })
            .collect();
        let proposal = propose::propose(units, &|p| generated.get(p).cloned());
        let arranged = arrange::arrange(&proposal, units, self.review_purpose.saved.as_ref());
        let checked = arrange::check(&arranged, units).map(|()| arranged);
        let state = &mut self.review_purpose;
        state.generation = generation;
        state.proposal = Some(proposal);
        state.arranged = Some(checked);
    }

    /// Takes `saved` as the arrangement, shows it and writes it down.
    fn purpose_keep(&mut self, saved: Option<Saved>, ctx: &egui::Context) {
        let Some(subject) = self.review_purpose.asked.clone() else {
            return;
        };
        self.review_purpose.saved = saved.clone();
        self.purpose_refresh(ctx);
        let state = &mut self.review_purpose;
        state.sent += 1;
        let (seq, written, repo) = (state.sent, state.written.clone(), self.path.clone());
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                let mut last = written.lock().unwrap_or_else(|e| e.into_inner());
                if *last > seq {
                    return Ok(());
                }
                store::save(&repo, &subject.url, saved.as_ref())?;
                *last = seq;
                Ok(())
            },
            |tab, result: Result<(), String>, _| {
                if let Err(e) = result {
                    tab.review_purpose.notice =
                        Some((format!("Your grouping couldn't be saved: {e}"), true));
                }
            },
        );
    }
}
