use super::*;

/// What a run's thread leaves the tab to do with what it found.
type ToTab = super::super::messages::Continuation;

/// What a run's thread does: stores the command, lists the commits, tests them. `send` takes
/// what the tab is to do with what it found.
pub(super) fn run_in_background(
    path: &std::path::Path,
    start: &StartTests,
    command: &str,
    stored: Option<&str>,
    stop: &RunStop,
    send: &(impl Fn(ToTab) + Sync),
    id: u64,
) -> Result<RunSummary, String> {
    if stored.map(str::trim) != Some(command) {
        if start.forget && stored.is_some() {
            git::forget_results(path, git::DEFAULT_TEST)?;
        }
        git::set_test_command(path, git::DEFAULT_TEST, command)?;
    }
    let jobs = git::commits_to_test(path, &start.range)?;
    let trees: Vec<String> = jobs.iter().map(|j| j.tree.clone()).collect();
    send(Box::new(move |tab, _| tab.tests_listed(id, trees)));
    let plan = TestPlan {
        name: git::DEFAULT_TEST.to_owned(),
        command: command.to_owned(),
        workers: workers_for(path, start.workers),
        retest: start.retest,
    };
    let tell = |event: TestEvent| send(Box::new(move |tab, _| tab.tests_event(id, event)));
    git::run_tests(path, jobs, &plan, stop, &tell)
}

/// How many commits are tested at once in `repo` when `asked` are: on another machine, at
/// most [`REMOTE_WORKERS`], so that the shells kept open there stay free for the tab's reads.
pub(crate) fn workers_for(repo: &std::path::Path, asked: usize) -> usize {
    if crate::remote::is_remote(repo) {
        asked.min(REMOTE_WORKERS)
    } else {
        asked
    }
}

/// The most commits tested at once on another machine.
pub(crate) const REMOTE_WORKERS: usize = 4;

/// What the status line says when the run of `range` ended so, and whether it is bad news.
pub(crate) fn ended_message(range: &str, result: &Result<RunSummary, String>) -> (String, bool) {
    match result {
        Err(e) => (format!("Couldn't test {range}: {e}"), true),
        Ok(s) => {
            let tested = s.good + s.bad - s.known;
            let mut parts = vec![format!("{} passed", s.good)];
            if s.bad > 0 {
                parts.push(format!("{} failed", s.bad));
            }
            if !s.failed.is_empty() {
                parts.push(format!("{} couldn't be tested", s.failed.len()));
            }
            let noun = if tested == 1 { "commit" } else { "commits" };
            let mut head = if s.stopped {
                format!("Testing {range} stopped after {tested} {noun}")
            } else {
                format!("Tested {tested} {noun} of {range}")
            };
            if s.known > 0 {
                head.push_str(&format!(" ({} had a result already)", s.known));
            }
            let mut said = format!("{head}: {}", parts.join(", "));
            if let Some(e) = &s.cleanup {
                said.push_str(&format!(". Couldn't remove a scratch worktree: {e}"));
            }
            let bad = s.bad > 0 || !s.failed.is_empty() || s.cleanup.is_some();
            (said, bad)
        }
    }
}

/// How the dot of `state` reads on hover, for the tree `tree` and the test `command`.
pub(crate) fn dot_tip(state: DotState, command: Option<&str>, tree: &str, ran: bool) -> String {
    let cmd = command.map_or_else(
        || crate::i18n::text("repo-workflows-test_lane-dot-tip-ad6afd6e").to_owned(),
        |c| format!("`{c}`"),
    );
    let what = match state {
        DotState::Good => crate::i18n::message(
            "repo-workflows-test_lane-dot-tip-4d352018",
            &[("cmd", (cmd).clone().into())],
        ),
        DotState::Bad => crate::i18n::message(
            "repo-workflows-test_lane-dot-tip-bc50fea6",
            &[("cmd", (cmd).clone().into())],
        ),
        DotState::Running => crate::i18n::message(
            "repo-workflows-test_lane-dot-tip-992c6c78",
            &[("cmd", (cmd).clone().into())],
        ),
        DotState::Queued => {
            crate::i18n::text("repo-workflows-test_lane-dot-tip-5830cb10").to_owned()
        }
        DotState::Failed => {
            crate::i18n::text("repo-workflows-test_lane-dot-tip-4db748be").to_owned()
        }
    };
    let how = match state {
        DotState::Good | DotState::Bad if ran => {
            crate::i18n::text("repo-workflows-test_lane-dot-tip-c91f64ce")
        }
        DotState::Failed => crate::i18n::text("repo-workflows-test_lane-dot-tip-3ebdbb8b"),
        DotState::Good | DotState::Bad => {
            crate::i18n::text("repo-workflows-test_lane-dot-tip-10d4cd00")
        }
        _ => "",
    };
    crate::i18n::message(
        "repo-workflows-test_lane-dot-tip-f9f1c77d",
        &[
            ("what", (what).clone().into()),
            ("short-tree-1", (short(tree)).into()),
            ("how", (how).into()),
        ],
    )
}

/// Presentation of the supplied result; the stored status factory above remains raw.
pub(super) fn display_ended_message(
    range: &str,
    result: &Result<RunSummary, String>,
) -> (String, bool) {
    match result {
        Err(error) => (
            crate::i18n::message(
                "repo-workflows-test_lane-ended-message-b4c579e3",
                &[
                    ("range", range.into()),
                    (
                        "e",
                        crate::repo::activity::display_workflow_error(error).into(),
                    ),
                ],
            ),
            true,
        ),
        Ok(s) => {
            let tested = s.good + s.bad - s.known;
            let display = crate::i18n::message(
                "repo-workflows-test-lane-ended-display",
                &[
                    ("stopped", (if s.stopped { "yes" } else { "no" }).into()),
                    ("range", (range).into()),
                    ("count", (tested).into()),
                    ("known", (s.known).into()),
                    ("good", (s.good).into()),
                    ("bad", (s.bad).into()),
                    ("unavailable", (s.failed.len()).into()),
                    (
                        "cleanup",
                        (if s.cleanup.is_some() { "yes" } else { "no" }).into(),
                    ),
                    (
                        "detail",
                        (s.cleanup
                            .as_deref()
                            .map(crate::repo::activity::display_workflow_error)
                            .unwrap_or_default())
                        .into(),
                    ),
                ],
            );
            (
                display,
                s.bad > 0 || !s.failed.is_empty() || s.cleanup.is_some(),
            )
        }
    }
}
