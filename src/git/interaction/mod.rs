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

/// How many commits the two branches may have between them.
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
    /// the budget ran out first and a smaller set may fail too).
    Together { commits: Vec<Change>, minimal: bool },
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
        return Err("Type the command to test with".to_owned());
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
    scratch.remove()?;
    let found = found.map_err(|halt| match halt {
        Halt::Stopped => "stopped".to_owned(),
        Halt::Error(e) => e,
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
        .map_err(|_| format!("{rev} isn't a commit"))?;
    }
    run(Some(repo), &["merge-base", "--end-of-options", a, b])
        .map(|s| s.trim().to_owned())
        .map_err(|_| format!("{a} and {b} have no commit in common"))
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
            return Err(format!(
                "{rev} has merge commits since it diverged: only branches without merges can be \
                 taken apart"
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
            return Err(format!(
                "{rev} has no commits of its own since the two diverged"
            ));
        }
    }
    if changes.len() > MAX_CHANGES {
        return Err(format!(
            "the two branches have {} commits between them; at most {MAX_CHANGES} can be taken \
             apart",
            changes.len()
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
        for s in [Side::A, Side::B] {
            if self.probe(&side(s), progress)? == Probe::Fail {
                return Ok(Found::FailsAlone(s));
            }
        }
        // What the checks above took counts too.
        let budget = self.spec.budget.saturating_sub(self.probes);
        let mut test = |subset: &[usize]| self.probe(subset, progress);
        let reduced = ddmin(all.len(), budget, &mut test)?;
        Ok(Found::Together {
            commits: reduced
                .failing
                .iter()
                .map(|&i| self.changes[i].clone())
                .collect(),
            minimal: reduced.minimal,
        })
    }

    /// Tests the commits `subset` applied on the base, running a failure again to be sure.
    fn probe(
        &mut self,
        subset: &[usize],
        progress: &(dyn Fn(SearchProgress) + Sync),
    ) -> Result<Probe, Halt> {
        let probe = self.probe_once(subset)?;
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
        git(&["checkout", "--detach", "--force", "-q", self.base], &[0])
            .map_err(|e| Halt::Error(format!("could not check out the base: {e}")))?;
        if !subset.is_empty() {
            let mut args = vec!["-c", "commit.gpgSign=false"];
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
