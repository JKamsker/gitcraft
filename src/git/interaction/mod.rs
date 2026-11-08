//! Finding the combination that breaks: two branches each pass on their own but fail
//! together. Subsets of their commits are applied on the commit they diverged from, in a
//! scratch worktree, and tested, reduced with `ddmin` until it can say which commits fail
//! together while leaving out any one of them passes. Tests have a budget, and a failure is
//! run again to tell a flaky one apart.

mod ddmin;
#[cfg(test)]
mod tests;

use std::path::Path;

pub use ddmin::{Halt, Probe, ddmin};

use super::testlane::{self, RunStop, Scratch, Verdict};
use crate::git::{run, run_ok_codes};

/// How many tests a search runs at most unless asked otherwise.
pub const DEFAULT_BUDGET: usize = 40;

/// How many commits the two branches may have together.
pub const MAX_CHANGES: usize = 200;

/// Which branch a commit is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// A commit of one of the branches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub commit: String,
    pub summary: String,
    pub side: Side,
}

/// No hooks for the checkouts and picks of a search: a commit hook refusing a picked commit
/// would make every subset unresolved. (A folder that isn't there: git finds no hook in it.)
const NO_HOOKS: [&str; 2] = ["-c", "core.hooksPath=.gitcraft-no-hooks"];

/// What to look for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionSpec {
    /// The two branches (or commits).
    pub a: String,
    pub b: String,
    pub command: String,
    /// The most tests to run looking.
    pub budget: usize,
    /// How many more times a failure is run before it is believed.
    pub reruns: usize,
}

/// What was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// These commits fail together; with any one left out, they pass (when `minimal`; else
    /// the budget ran out first and a smaller set may fail too), or, with `gaps`, some of those
    /// couldn't be told (they don't apply without the one left out).
    Together {
        commits: Vec<Change>,
        minimal: bool,
        gaps: bool,
    },
    /// The two pass together: there is nothing to find.
    PassTogether,
    /// A branch fails on its own: it isn't an interaction.
    FailsAlone(Side),
    /// The two branches' commits don't apply together.
    DontApply,
    /// The result of the two together doesn't repeat: the test is flaky.
    Flaky,
}

/// A search done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interaction {
    pub found: Found,
    /// The commit the branches diverged from.
    pub base: String,
    pub changes: Vec<Change>,
    /// Times the command ran, reruns included.
    pub tests: usize,
    /// Subsets whose result didn't repeat, by their commits.
    pub flaky: Vec<Vec<String>>,
}

/// How far a search got, told after each test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchProgress {
    /// Subsets tested so far, of at most `budget`.
    pub tests: usize,
    pub budget: usize,
    /// The commits in the subset just tested, and what it said.
    pub size: usize,
    pub probe: Probe,
}

/// Looks for the commits of `spec`'s branches that fail together, `progress` told after each
/// test, until done or `stop` stops it. For a background thread; the scratch worktree is
/// removed however it ends.
pub fn find_interaction(
    repo: &Path,
    spec: &InteractionSpec,
    stop: &RunStop,
    progress: &(dyn Fn(SearchProgress) + Sync),
) -> Result<Interaction, String> {
    if spec.command.trim().is_empty() {
        return Err(crate::i18n::text("backend-type-the-command-to-test-with-4b88f5").to_owned());
    }
    let base = merge_base(repo, &spec.a, &spec.b)?;
    let changes = changes_of(repo, &base, spec)?;
    let scratch = Scratch::add_empty(repo, "interaction", &base)?;
    let mut search = Search {
        repo,
        dir: &scratch.path,
        base: &base,
        changes: &changes,
        spec,
        stop,
        tests: 0,
        probes: 0,
        seen: Default::default(),
        flaky: Vec::new(),
        stored: testlane::test_command(repo, testlane::DEFAULT_TEST)
            .ok()
            .flatten()
            .as_deref()
            == Some(spec.command.trim()),
        fallback_identity: !testlane::has_committer(repo),
    };
    let found = search.run(progress);
    let (tests, flaky) = (search.tests, search.flaky);
    // A worktree that couldn't be removed doesn't make what was found any less true.
    let _ = scratch.remove();
    let found = found.map_err(|halt| match halt {
        Halt::Stopped => "stopped".to_owned(),
        Halt::Error(e) => e,
        Halt::OutOfBudget => {
            crate::i18n::text("backend-out-of-tests-before-the-checks-were-done-3c9d48").to_owned()
        }
    })?;
    Ok(Interaction {
        found,
        base,
        changes,
        tests,
        flaky,
    })
}

/// The commit `a` and `b` diverged from.
fn merge_base(repo: &Path, a: &str, b: &str) -> Result<String, String> {
    for rev in [a, b] {
        run(
            Some(repo),
            &[
                "rev-parse",
                "--verify",
                "-q",
                "--end-of-options",
                &format!("{rev}^{{commit}}"),
            ],
        )
        .map_err(|_| {
            crate::i18n::message(
                "backend-rev-isn-t-a-commit-4663f7",
                &[("rev", (rev).to_string().into())],
            )
        })?;
    }
    run(Some(repo), &["merge-base", "--end-of-options", a, b])
        .map(|s| s.trim().to_owned())
        .map_err(|_| {
            crate::i18n::message(
                "backend-a-and-b-have-no-commit-in-common-62f15a",
                &[("a", (a).to_string().into()), ("b", (b).to_string().into())],
            )
        })
}

/// The commits of each branch since `base`, oldest first: `a`'s, then `b`'s.
fn changes_of(repo: &Path, base: &str, spec: &InteractionSpec) -> Result<Vec<Change>, String> {
    let mut changes = Vec::new();
    for (rev, side) in [(&spec.a, Side::A), (&spec.b, Side::B)] {
        let range = format!("{base}..{rev}");
        let merges = run(
            Some(repo),
            &["rev-list", "--count", "--min-parents=2", &range],
        )?;
        if merges.trim() != "0" {
            return Err(crate::i18n::message(
                "backend-rev-has-merge-commits-since-it-diverged-only-branches-without-mer-012058",
                &[("rev", (rev).to_string().into())],
            ));
        }
        let out = run(
            Some(repo),
            &["log", "--reverse", "--topo-order", "--format=%H %s", &range],
        )?;
        let before = changes.len();
        changes.extend(out.lines().filter_map(|l| {
            let (commit, summary) = l.split_once(' ').unwrap_or((l, ""));
            (!commit.is_empty()).then(|| Change {
                commit: commit.to_owned(),
                summary: summary.to_owned(),
                side,
            })
        }));
        if changes.len() == before {
            return Err(crate::i18n::message(
                "backend-rev-has-no-commits-of-its-own-since-the-two-diverged-3fb761",
                &[("rev", (rev).to_string().into())],
            ));
        }
    }
    if changes.len() > MAX_CHANGES {
        return Err(crate::i18n::message(
            "backend-the-two-branches-have-commits-between-them-at-most-max-changes-ca-ed4e1b",
            &[
                ("count", (changes.len()).to_string().into()),
                ("max_changes", (MAX_CHANGES).to_string().into()),
            ],
        ));
    }
    Ok(changes)
}

/// What a search works with.
struct Search<'a> {
    repo: &'a Path,
    dir: &'a Path,
    base: &'a str,
    changes: &'a [Change],
    spec: &'a InteractionSpec,
    stop: &'a RunStop,
    tests: usize,
    /// Subsets tested (what the budget counts).
    probes: usize,
    /// What each subset tested said.
    seen: std::collections::HashMap<Vec<usize>, Probe>,
    flaky: Vec<Vec<String>>,
    /// The command is the repository's test command: results are shared with the lane.
    stored: bool,
    fallback_identity: bool,
}

impl Search<'_> {
    fn run(&mut self, progress: &(dyn Fn(SearchProgress) + Sync)) -> Result<Found, Halt> {
        let side = |s: Side| -> Vec<usize> {
            (0..self.changes.len())
                .filter(|&i| self.changes[i].side == s)
                .collect()
        };
        let all: Vec<usize> = (0..self.changes.len()).collect();
        let together = self.probe(&all, progress)?;
        match together {
            Probe::Pass => return Ok(Found::PassTogether),
            Probe::Unresolved if self.flaky.is_empty() => return Ok(Found::DontApply),
            Probe::Unresolved => return Ok(Found::Flaky),
            Probe::Fail => {}
        }
        let mut known = vec![(all.clone(), Probe::Fail)];
        for s in [Side::A, Side::B] {
            let alone = side(s);
            match self.probe(&alone, progress)? {
                Probe::Fail => return Ok(Found::FailsAlone(s)),
                // A branch's own commits always apply: it didn't repeat.
                Probe::Unresolved => return Ok(Found::Flaky),
                Probe::Pass => known.push((alone, Probe::Pass)),
            }
        }
        // The checks above count against the budget too, and their results are known.
        let mut test = |subset: &[usize]| self.probe_interaction(subset, progress);
        let reduced = ddmin(all.len(), usize::MAX, &known, &mut test)?;
        Ok(Found::Together {
            commits: reduced
                .failing
                .iter()
                .map(|&i| self.changes[i].clone())
                .collect(),
            minimal: reduced.minimal,
            gaps: reduced.gaps,
        })
    }

    /// Whether `subset` fails as an interaction: it fails, and each branch's part of it alone
    /// passes. One that fails because of one branch's commits alone (taken without the later
    /// commit that fixes them, say) can't be told: it is no interaction.
    fn probe_interaction(
        &mut self,
        subset: &[usize],
        progress: &(dyn Fn(SearchProgress) + Sync),
    ) -> Result<Probe, Halt> {
        let probe = self.probe(subset, progress)?;
        if probe != Probe::Fail {
            return Ok(probe);
        }
        for side in [Side::A, Side::B] {
            let part: Vec<usize> = subset
                .iter()
                .copied()
                .filter(|&i| self.changes[i].side == side)
                .collect();
            if part.len() == subset.len() || self.probe(&part, progress)? != Probe::Pass {
                return Ok(Probe::Unresolved);
            }
        }
        Ok(Probe::Fail)
    }

    /// Tests the commits `subset` applied on the base, running a failure again to be sure;
    /// each subset once, within the budget.
    fn probe(
        &mut self,
        subset: &[usize],
        progress: &(dyn Fn(SearchProgress) + Sync),
    ) -> Result<Probe, Halt> {
        if subset.is_empty() {
            return Ok(Probe::Pass);
        }
        if let Some(probe) = self.seen.get(subset) {
            return Ok(*probe);
        }
        if self.probes >= self.spec.budget {
            return Err(Halt::OutOfBudget);
        }
        let probe = self.probe_once(subset)?;
        self.seen.insert(subset.to_vec(), probe);
        self.probes += 1;
        progress(SearchProgress {
            tests: self.probes,
            budget: self.spec.budget,
            size: subset.len(),
            probe,
        });
        Ok(probe)
    }

    fn probe_once(&mut self, subset: &[usize]) -> Result<Probe, Halt> {
        if self.stop.stopped() {
            return Err(Halt::Stopped);
        }
        let Some(tree) = self.apply(subset)? else {
            return Ok(Probe::Unresolved);
        };
        if self.stored
            && let Ok(known) = testlane::read_results(self.repo, testlane::DEFAULT_TEST)
            && let Some(verdict) = known.get(&tree)
        {
            return Ok(match verdict {
                Verdict::Good => Probe::Pass,
                Verdict::Bad => Probe::Fail,
            });
        }
        let first = self.run_test()?;
        let mut verdict = first;
        if first == Verdict::Bad {
            for _ in 0..self.spec.reruns {
                if self.run_test()? == Verdict::Good {
                    self.flaky.push(
                        subset
                            .iter()
                            .map(|&i| self.changes[i].commit.clone())
                            .collect(),
                    );
                    return Ok(Probe::Unresolved);
                }
            }
            verdict = Verdict::Bad;
        }
        if self.stored {
            let _ = testlane::write_result(
                self.repo,
                testlane::DEFAULT_TEST,
                &tree,
                verdict,
                self.fallback_identity,
            );
        }
        Ok(match verdict {
            Verdict::Good => Probe::Pass,
            Verdict::Bad => Probe::Fail,
        })
    }

    /// Checks out the base with `subset`'s commits picked on it, in order: the tree it gives,
    /// or `None` when they don't apply.
    fn apply(&self, subset: &[usize]) -> Result<Option<String>, Halt> {
        let git = |args: &[&str], ok: &[i32]| run_ok_codes(Some(self.dir), args, ok);
        let _ = git(&["cherry-pick", "--abort"], &[0, 128]);
        git(
            &[
                NO_HOOKS[0],
                NO_HOOKS[1],
                "checkout",
                "--detach",
                "--force",
                "-q",
                self.base,
            ],
            &[0],
        )
        .map_err(|e| {
            Halt::Error(crate::i18n::message(
                "backend-could-not-check-out-the-base-e-be0404",
                &[("detail", (e).to_string().into())],
            ))
        })?;
        // What the last test left (its files that git doesn't ignore) would be in the way of a
        // commit adding them, or change what the next test sees; ignored ones (build output)
        // stay, as a build can reuse them.
        git(&["clean", "-ffdq"], &[0]).map_err(|e| {
            Halt::Error(crate::i18n::message(
                "backend-could-not-clean-the-scratch-worktree-e-1d3043",
                &[("detail", (e).to_string().into())],
            ))
        })?;
        if !subset.is_empty() {
            let mut args = vec![NO_HOOKS[0], NO_HOOKS[1], "-c", "commit.gpgSign=false"];
            if self.fallback_identity {
                args.extend([
                    "-c",
                    "user.name=gitcraft",
                    "-c",
                    "user.email=gitcraft@localhost",
                ]);
            }
            args.extend(["cherry-pick", "--allow-empty", "--keep-redundant-commits"]);
            args.extend(subset.iter().map(|&i| self.changes[i].commit.as_str()));
            if git(&args, &[0]).is_err() {
                let _ = git(&["cherry-pick", "--abort"], &[0, 128]);
                return Ok(None);
            }
        }
        git(&["rev-parse", "HEAD^{tree}"], &[0])
            .map(|t| Some(t.trim().to_owned()))
            .map_err(Halt::Error)
    }

    /// Runs the command once where things are checked out.
    fn run_test(&mut self) -> Result<Verdict, Halt> {
        self.tests += 1;
        let stopper = self.stop.command_stopper();
        let (ended, _) =
            testlane::run_command(self.dir, &self.spec.command, &stopper).map_err(Halt::Error)?;
        testlane::verdict_of(&ended).ok_or(Halt::Stopped)
    }
}
