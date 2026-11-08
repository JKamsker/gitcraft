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
use self::memo::ToRecord;
use self::record::{Decision, TestRun};
use crate::git;
use crate::repo::RepoTab;

/// What the views ask of decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Read the decisions stored for the open editor's conflicts.
    Load,
    /// Bring the remote's decisions here (the newer of two for the same conflict wins), and
    /// with `true` send them all back to it.
    Sync(String, bool),
}

/// Decisions to store once their file is saved: taken as the save starts, so that what is
/// stored is what was saved, whatever the editor shows by the time it is written.
pub struct Pending {
    pub path: String,
    pub decisions: ToRecord,
    pub run_tests: bool,
    /// How many other files still had conflicts as it was saved (a test may fail for them).
    pub others_conflicted: usize,
}

/// What a test's command runs as: without the variables gitcraft gives git itself (no editor,
/// no prompts, no optional locks), so that it runs as it would in a terminal.
fn as_typed(command: &str) -> String {
    format!("unset GIT_EDITOR GIT_TERMINAL_PROMPT GIT_OPTIONAL_LOCKS; {command}")
}

/// How running `command` (a decision's test) went, in the repository's top folder.
/// Blocking: from the background.
pub fn run_test(repo: &std::path::Path, command: &str) -> Result<TestRun, String> {
    let alias = format!("alias.{}=!{}", git::USER_COMMAND_ALIAS, as_typed(command));
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
        Ok(r) if r.passed => (
            crate::i18n::message("repo-core-0479", &[("command", command.to_string().into())]),
            false,
        ),
        Ok(r) => {
            let last = r
                .output
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or(crate::i18n::text("repo-core-0480"));
            (
                crate::i18n::message(
                    "repo-core-0481",
                    &[
                        ("command", command.to_string().into()),
                        ("last", last.to_string().into()),
                    ],
                ),
                true,
            )
        }
        Err(e) => (
            crate::i18n::message(
                "repo-core-0482",
                &[
                    ("command", command.to_string().into()),
                    ("e", e.to_string().into()),
                ],
            ),
            true,
        ),
    }
}

/// Stores the decisions of `pending`, then, when asked, runs each different test once and
/// keeps how it went with every decision that has it (stored again). Stored first: a test
/// that never ends loses no note. Blocking: from the background. Says what happened.
fn record_and_test(repo: &std::path::Path, pending: Pending) -> Vec<(String, bool)> {
    let Pending {
        mut decisions,
        run_tests,
        others_conflicted,
        ..
    } = pending;
    let mut said = Vec::new();
    let stored = store_all(repo, &decisions, &mut said);
    if stored > 0 {
        said.push((
            match stored {
                1 => crate::i18n::text("repo-core-0483").to_owned(),
                n => crate::i18n::message("repo-core-0484", &[("n", (n).into())]),
            },
            false,
        ));
    }
    if !run_tests {
        return said;
    }
    let mut commands: Vec<String> = decisions
        .iter()
        .map(|(_, d)| d.test.clone())
        .filter(|t| !t.is_empty())
        .collect();
    commands.sort();
    commands.dedup();
    for command in commands {
        let run = run_test(repo, &command);
        let (mut text, bad) = test_message(&command, &run);
        if bad && others_conflicted > 0 {
            text.push_str(&crate::i18n::message(
                "repo-core-0485",
                &[("others-conflicted", (others_conflicted).into())],
            ));
        }
        said.push((text, bad));
        if let Ok(run) = run {
            for (_, d) in decisions.iter_mut().filter(|(_, d)| d.test == command) {
                d.last_test = Some(run.clone());
            }
        }
    }
    store_all(repo, &decisions, &mut said);
    said
}

/// Stores each of `decisions`; says what couldn't be. Returns how many were.
fn store_all(
    repo: &std::path::Path,
    decisions: &[(String, Decision)],
    said: &mut Vec<(String, bool)>,
) -> usize {
    let mut stored = 0;
    for (key, decision) in decisions {
        match store::save(repo, key, decision) {
            Ok(()) => stored += 1,
            Err(e) => said.push((e, true)),
        }
    }
    stored
}

impl RepoTab {
    pub(in crate::repo) fn decisions_act(&mut self, act: Act, ctx: &egui::Context) {
        match act {
            Act::Load => self.load_decisions(ctx),
            Act::Sync(remote, send) => {
                let repo = self.path.clone();
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-0486",
                        &[
                            (
                                "Fetching",
                                (if send {
                                    crate::i18n::text("repo-core-0487")
                                } else {
                                    crate::i18n::text("repo-core-0488")
                                })
                                .to_string()
                                .into(),
                            ),
                            ("remote", remote.to_string().into()),
                        ],
                    ),
                    false,
                )));
                self.then(
                    ctx,
                    move || store::sync(&repo, &remote, send),
                    |tab, said, _| {
                        tab.status = Some(crate::repo::Notice::plain(match said {
                            Ok(text) => (text, false),
                            Err(e) => (e, true),
                        }));
                    },
                );
            }
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
            {
                let keys = keys.clone();
                move || store::load(&repo, &keys)
            },
            move |tab, read, _| {
                // Still that editor, with those conflicts (Mergiraf may have solved some).
                let Some(ed) = tab.conflict.as_mut().filter(|ed| {
                    ed.generation == generation
                        && ed.path == path
                        && ed.memo.spots.iter().map(|s| &s.key).eq(keys.iter())
                }) else {
                    return;
                };
                match read {
                    Ok(mut found) => {
                        let found = keys.iter().map(|k| found.remove(k)).collect();
                        let applied = ed.take_offers(found);
                        if applied > 0 {
                            tab.status = Some(crate::repo::Notice::plain((
                                match applied {
                                    1 => crate::i18n::text("repo-core-0489").to_owned(),
                                    n => {
                                        crate::i18n::message("repo-core-0490", &[("n", (n).into())])
                                    }
                                },
                                false,
                            )));
                        }
                    }
                    Err(e) => {
                        // Not read: nothing offered, and not asked again for this editor.
                        ed.memo.offers = Some(Vec::new());
                        ed.memo.reading = false;
                        tab.status = Some(crate::repo::Notice::plain((e, true)));
                    }
                }
            },
        );
    }

    /// As the editor's file is about to be saved: what of its decisions is to be stored once
    /// it is (see [`Pending`]). `false`: they can't be told apart, and the save goes on
    /// without them (said in the toolbar).
    pub(in crate::repo) fn prepare_decisions(&mut self) {
        let Some(ed) = self.conflict.as_ref() else {
            return;
        };
        self.decisions_pending = match ed.to_record("", git::now()) {
            Ok(decisions) if decisions.is_empty() => None,
            Ok(decisions) => Some(Pending {
                path: ed.path.clone(),
                run_tests: ed.memo.run_tests && ed.has_tests(),
                others_conflicted: self
                    .work
                    .conflicted
                    .iter()
                    .filter(|f| f.path != ed.path)
                    .count(),
                decisions,
            }),
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((e, true)));
                None
            }
        };
    }

    /// Once `path` was saved: stores the decisions taken for it as the save started, and runs
    /// their tests when asked, in the background.
    pub(in crate::repo) fn record_decisions(&mut self, path: &str, ctx: &egui::Context) {
        let Some(pending) = self.decisions_pending.take_if(|p| p.path == path) else {
            return;
        };
        if pending.run_tests {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-0491").to_owned(),
                false,
            )));
        }
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                // Who commits here wrote the notes; a decision used again keeps its author.
                let by = git::identity(&repo).name;
                let mut pending = pending;
                for (_, d) in pending
                    .decisions
                    .iter_mut()
                    .filter(|(_, d)| d.by.is_empty())
                {
                    d.by.clone_from(&by);
                }
                record_and_test(&repo, pending)
            },
            |tab, said, _| {
                if said.is_empty() {
                    return;
                }
                let failed = said.iter().any(|(_, bad)| *bad);
                let text = said
                    .into_iter()
                    .map(|(t, _)| t)
                    .collect::<Vec<_>>()
                    .join(" · ");
                tab.status = Some(crate::repo::Notice::plain((text, failed)));
            },
        );
    }

    /// The palette's commands for decisions: bring the default remote's here, or send them
    /// there too.
    pub(in crate::repo) fn decision_palette_items(
        &self,
    ) -> Vec<(String, String, crate::repo::Action)> {
        let Some(remote) = self.default_remote() else {
            return Vec::new();
        };
        vec![
            (
                crate::i18n::message("repo-core-0492", &[("remote", remote.to_string().into())]),
                "conflicts".into(),
                crate::repo::Action::Decisions(Act::Sync(remote.clone(), true)),
            ),
            (
                crate::i18n::message("repo-core-0493", &[("remote", remote.to_string().into())]),
                "conflicts".into(),
                crate::repo::Action::Decisions(Act::Sync(remote, false)),
            ),
        ]
    }
}

#[cfg(test)]
mod tests;
