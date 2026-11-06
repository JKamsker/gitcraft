//! Conflict decisions that remember why: resolving a conflict, a note on why and a command
//! that tests it can be kept with the resolution, in git. When the same conflict comes back
//! (its two sides the same, in a merge or a rebase), the decision is applied with its reason
//! shown, or, when the code around it changed since, only offered, as stale. Its test runs
//! once the file is saved, when asked, and how it went is kept with it.

mod memo;
pub mod record;
pub mod store;
pub(in crate::repo) mod view;

pub use self::memo::Memo;
use self::record::{Decision, TestRun};
use crate::git;
use crate::repo::RepoTab;

/// What the views ask of decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Read the decisions stored for the open editor's conflicts.
    Load,
    /// Send every decision to the remote, or bring the remote's here.
    Push(String),
    Fetch(String),
}

/// How running `command` (a decision's test) went, in the repository's top folder.
/// Blocking: from the background.
pub fn run_test(repo: &std::path::Path, command: &str) -> Result<TestRun, String> {
    let alias = format!("alias.{}=!{command}", git::USER_COMMAND_ALIAS);
    let out = git::run_captured(repo, &["-c", &alias, git::USER_COMMAND_ALIAS], None)?;
    let both = match (out.stdout.trim().is_empty(), out.stderr.trim().is_empty()) {
        (_, true) => out.stdout,
        (true, false) => out.stderr,
        (false, false) => format!("{}\n{}", out.stdout.trim_end(), out.stderr),
    };
    Ok(TestRun {
        passed: out.code == Some(0),
        at: git::now(),
        output: record::output_tail(&both),
    })
}

/// What a test's run says, for the toolbar.
fn test_message(command: &str, run: &Result<TestRun, String>) -> (String, bool) {
    match run {
        Ok(r) if r.passed => (format!("Test “{command}” passes"), false),
        Ok(r) => {
            let last = r.output.lines().last().unwrap_or("no output");
            (format!("Test “{command}” fails: {last}"), true)
        }
        Err(e) => (format!("Test “{command}” could not run: {e}"), true),
    }
}

/// Stores `decisions` and, when `run_tests`, runs each different test once, keeping how it
/// went with every decision that has it. Blocking: from the background. Says what happened.
fn record_and_test(
    repo: &std::path::Path,
    mut decisions: Vec<(String, Decision)>,
    run_tests: bool,
) -> Vec<(String, bool)> {
    let mut said = Vec::new();
    if run_tests {
        let mut commands: Vec<String> = decisions
            .iter()
            .map(|(_, d)| d.test.clone())
            .filter(|t| !t.is_empty())
            .collect();
        commands.dedup();
        commands.sort();
        commands.dedup();
        for command in commands {
            let run = run_test(repo, &command);
            said.push(test_message(&command, &run));
            if let Ok(run) = run {
                for (_, d) in decisions.iter_mut().filter(|(_, d)| d.test == command) {
                    d.last_test = Some(run.clone());
                }
            }
        }
    }
    let mut stored = 0;
    for (key, decision) in &decisions {
        match store::save(repo, key, decision) {
            Ok(()) => stored += 1,
            Err(e) => said.push((e, true)),
        }
    }
    if stored > 0 {
        said.insert(
            0,
            (
                match stored {
                    1 => "Kept the decision and its reason".to_owned(),
                    n => format!("Kept {n} decisions and their reasons"),
                },
                false,
            ),
        );
    }
    said
}

impl RepoTab {
    pub(in crate::repo) fn decisions_act(&mut self, act: Act, ctx: &egui::Context) {
        match act {
            Act::Load => self.load_decisions(ctx),
            Act::Push(remote) => self.run_ops(
                &format!("Send conflict decisions to {remote}"),
                store::push_ops(&remote),
                crate::repo::Refresh::Status,
                crate::repo::OpKind::plain(),
                ctx,
            ),
            Act::Fetch(remote) => self.run_ops(
                &format!("Fetch conflict decisions from {remote}"),
                store::fetch_ops(&remote),
                crate::repo::Refresh::Status,
                crate::repo::OpKind::plain(),
                ctx,
            ),
        }
    }

    /// Reads, in the background, the decisions stored for the open editor's conflicts.
    fn load_decisions(&mut self, ctx: &egui::Context) {
        let repo = self.path.clone();
        let Some(ed) = self.conflict.as_mut().filter(|ed| !ed.memo.reading) else {
            return;
        };
        ed.memo.reading = true;
        ed.memo.spots = ed.decision_spots();
        let keys: Vec<String> = ed.memo.spots.iter().map(|s| s.key.clone()).collect();
        let (generation, path) = (ed.generation, ed.path.clone());
        self.then(
            ctx,
            move || store::load(&repo, &keys).map(|found| (keys, found)),
            move |tab, read, _| {
                let Some(ed) = tab
                    .conflict
                    .as_mut()
                    .filter(|ed| ed.generation == generation && ed.path == path)
                else {
                    return;
                };
                match read {
                    Ok((keys, mut found)) => {
                        let found = keys.iter().map(|k| found.remove(k)).collect();
                        let applied = ed.take_offers(found);
                        if applied > 0 {
                            tab.status = Some((
                                match applied {
                                    1 => "Applied a decision you made before: check it, then save"
                                        .to_owned(),
                                    n => format!(
                                        "Applied {n} decisions you made before: check them, \
                                         then save"
                                    ),
                                },
                                false,
                            ));
                        }
                    }
                    Err(e) => {
                        // Not read: nothing offered, and not asked again for this editor.
                        ed.memo.offers = Some(Vec::new());
                        ed.memo.reading = false;
                        tab.status = Some((e, true));
                    }
                }
            },
        );
    }

    /// Once the editor's file is saved: stores the decisions written for it and runs their
    /// tests when asked, in the background.
    pub(in crate::repo) fn record_decisions(&mut self, path: &str, ctx: &egui::Context) {
        let Some(ed) = self.conflict.as_ref().filter(|ed| ed.path == path) else {
            return;
        };
        let decisions = match ed.to_record("", git::now()) {
            Ok(d) if d.is_empty() => return,
            Ok(d) => d,
            Err(e) => {
                self.status = Some((e, true));
                return;
            }
        };
        let run_tests = ed.memo.run_tests && ed.has_tests();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                // Who commits here: who made the decisions.
                let by = git::identity(&repo).name;
                let decisions = decisions
                    .into_iter()
                    .map(|(k, d)| {
                        (
                            k,
                            Decision {
                                by: by.clone(),
                                ..d
                            },
                        )
                    })
                    .collect();
                record_and_test(&repo, decisions, run_tests)
            },
            |tab, said, _| {
                // The worst news shown, the rest joined after it.
                if said.is_empty() {
                    return;
                }
                let failed = said.iter().any(|(_, bad)| *bad);
                let text = said
                    .into_iter()
                    .map(|(t, _)| t)
                    .collect::<Vec<_>>()
                    .join(" · ");
                tab.status = Some((text, failed));
            },
        );
    }

    /// The palette's commands for decisions: send them to the default remote, bring its own.
    pub(in crate::repo) fn decision_palette_items(
        &self,
    ) -> Vec<(String, String, crate::repo::Action)> {
        let Some(remote) = self.default_remote() else {
            return Vec::new();
        };
        vec![
            (
                format!("Send conflict decisions to {remote}"),
                "conflicts".into(),
                crate::repo::Action::Decisions(Act::Push(remote.clone())),
            ),
            (
                format!("Fetch conflict decisions from {remote}"),
                "conflicts".into(),
                crate::repo::Action::Decisions(Act::Fetch(remote)),
            ),
        ]
    }
}

#[cfg(test)]
mod tests;
