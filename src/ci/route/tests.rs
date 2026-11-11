use super::*;

use super::fixtures::*;

/// The case this module exists for: `dev` tracks the fork, whose CI runs the builds.
#[test]
fn a_forks_branches_are_asked_of_the_fork() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        local: vec![branch("dev", "local1", "fork/dev")],
        remotes: vec![
            (
                "origin".to_owned(),
                vec![branch("origin/master", "up1", "")],
            ),
            ("fork".to_owned(), vec![branch("fork/dev", "fork1", "")]),
        ],
        head: "local1".to_owned(),
        current: Some("dev".to_owned()),
        ..Refs::default()
    };
    let plan = plan_of(&remotes, &refs, vec!["pr1"], vec!["local1", "old1"]);
    assert_eq!(shas_of(&plan, "me/repo"), ["local1", "old1", "fork1"]);
    assert_eq!(shas_of(&plan, "up/repo"), ["pr1", "local1", "old1", "up1"]);
    assert_eq!(
        plan[0].0.path, "me/repo",
        "the checked-out branch's project first"
    );
}

#[test]
fn without_an_upstream_everything_local_goes_to_the_main_project() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        local: vec![branch("topic", "t1", "")],
        head: "t1".to_owned(),
        current: Some("topic".to_owned()),
        ..Refs::default()
    };
    let plan = plan_of(&remotes, &refs, vec![], vec!["t0"]);
    assert_eq!(plan.len(), 1);
    assert_eq!(shas_of(&plan, "up/repo"), ["t1", "t0"]);
}

#[test]
fn a_detached_head_goes_to_the_main_project() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        local: vec![branch("dev", "d1", "fork/dev")],
        head: "h1".to_owned(),
        current: None,
        ..Refs::default()
    };
    let plan = plan_of(&remotes, &refs, vec![], vec![]);
    assert_eq!(shas_of(&plan, "up/repo"), ["h1"]);
    assert_eq!(shas_of(&plan, "me/repo"), ["d1"]);
}

/// A mirror on another host can't be asked through the main project's tool.
#[test]
fn a_remote_on_another_host_is_asked_as_the_main_project() {
    let remotes = vec![
        remote("origin", "https://github.com/up/repo.git"),
        remote("mirror", "https://gitlab.com/me/repo.git"),
    ];
    let refs = Refs {
        remotes: vec![("mirror".to_owned(), vec![branch("mirror/main", "m1", "")])],
        ..Refs::default()
    };
    let plan = plan_of(&remotes, &refs, vec![], vec![]);
    assert_eq!(plan.len(), 1);
    assert_eq!(shas_of(&plan, "up/repo"), ["m1"]);
}

#[test]
fn an_upstream_is_on_the_longest_remote_name_it_starts_with() {
    let remotes = vec![
        remote("my", "https://github.com/my/one.git"),
        remote("my/fork", "https://github.com/my/fork.git"),
        remote("myfork", "https://github.com/my/other.git"),
    ];
    assert_eq!(remote_of("my/fork/dev", &remotes), Some("my/fork"));
    assert_eq!(remote_of("my/dev", &remotes), Some("my"));
    assert_eq!(remote_of("myfork/dev", &remotes), Some("myfork"));
    assert_eq!(remote_of("dev", &remotes), None);
    assert_eq!(remote_of("", &remotes), None);
}

#[test]
fn a_commit_on_two_remotes_is_asked_of_both_once() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        remotes: vec![
            (
                "origin".to_owned(),
                vec![
                    branch("origin/main", "same", ""),
                    branch("origin/x", "same", ""),
                ],
            ),
            ("fork".to_owned(), vec![branch("fork/main", "same", "")]),
        ],
        ..Refs::default()
    };
    let plan = plan_of(&remotes, &refs, vec![], vec![]);
    assert_eq!(shas_of(&plan, "up/repo"), ["same"]);
    assert_eq!(shas_of(&plan, "me/repo"), ["same"]);
}

#[test]
fn no_more_than_a_few_projects_are_asked() {
    let remotes: Vec<Remote> = (0..8)
        .map(|i| {
            remote(
                &format!("r{i}"),
                &format!("https://github.com/o{i}/repo.git"),
            )
        })
        .chain([remote("origin", "https://github.com/up/repo.git")])
        .collect();
    let refs = Refs {
        remotes: (0..8)
            .map(|i| {
                (
                    format!("r{i}"),
                    vec![branch(&format!("r{i}/x"), &format!("s{i}"), "")],
                )
            })
            .collect(),
        ..Refs::default()
    };
    let plan = plan_of(&remotes, &refs, vec![], vec![]);
    assert_eq!(plan.len(), MAX_PROJECTS);
    // The remotes past the limit go to the main project.
    assert_eq!(shas_of(&plan, "up/repo"), ["s3", "s4", "s5", "s6", "s7"]);
}

#[test]
fn an_unborn_head_is_not_asked() {
    let remotes = fork_and_upstream();
    let refs = Refs::default();
    let plan = plan_of(&remotes, &refs, vec![], vec!["c1"]);
    assert_eq!(shas_of(&plan, "up/repo"), ["c1"]);
}

/// A triangular workflow keeps looking at the push fork after its tracking tip moves.
#[test]
fn push_remote_and_tracking_upstream_are_both_asked() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        local: vec![branch("dev", "local", "origin/main")],
        push_remotes: HashMap::from([("dev".to_owned(), "fork".to_owned())]),
        head: "local".to_owned(),
        current: Some("dev".to_owned()),
        remotes: vec![("fork".to_owned(), vec![branch("fork/dev", "newer", "")])],
        ..Refs::default()
    };
    let plan = plan_of(&remotes, &refs, vec![], vec!["old"]);
    assert_eq!(shas_of(&plan, "me/repo"), ["local", "old", "newer"]);
    assert_eq!(shas_of(&plan, "up/repo"), ["local", "old"]);
    assert_eq!(
        paths(&projects_for("local", &project("up/repo"), &remotes, &refs)),
        ["me/repo", "up/repo"]
    );
}

/// Recent shared history still shows checks that only upstream ran.
#[test]
fn recent_commits_are_asked_of_the_fork_and_upstream_within_the_pair_budget() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        local: vec![branch("dev", "head", "fork/dev")],
        head: "head".to_owned(),
        current: Some("dev".to_owned()),
        ..Refs::default()
    };
    let main = project("up/repo");
    let sources = Sources {
        main: &main,
        remotes: &remotes,
        refs: &refs,
        prs: vec![],
        recent: vec!["old", "older"],
    };
    let plan = plan(&sources, 3);
    assert_eq!(pairs(&plan).len(), 3);
    assert_eq!(shas_of(&plan, "me/repo"), ["head", "old"]);
    assert_eq!(shas_of(&plan, "up/repo"), ["old"]);
}

/// The query budget applies after combining branch-specific and other remotes.
#[test]
fn projects_for_has_one_budget_but_known_includes_all_eligible_remotes() {
    let remotes: Vec<_> = (0..8)
        .map(|i| {
            remote(
                &format!("r{i}"),
                &format!("https://github.com/o{i}/repo.git"),
            )
        })
        .collect();
    let refs = Refs {
        local: vec![branch("dev", "head", "r7/dev")],
        current: Some("dev".to_owned()),
        head: "head".to_owned(),
        ..Refs::default()
    };
    let main = project("up/repo");
    assert_eq!(
        paths(&projects_for("old", &main, &remotes, &refs)),
        ["o7/repo", "up/repo", "o0/repo", "o1/repo"]
    );
    assert_eq!(known(&main, &remotes).len(), 9);
}

fn paths(projects: &[Project]) -> Vec<&str> {
    projects.iter().map(|p| p.path.as_str()).collect()
}

/// The builds view and the MCP tool look where the commit's branch is pushed first.
#[test]
fn a_commit_only_the_fork_has_is_looked_up_on_the_fork_first() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        local: vec![branch("main", "m1", "origin/main")],
        remotes: vec![("fork".to_owned(), vec![branch("fork/dev", "f1", "")])],
        head: "m1".to_owned(),
        current: Some("main".to_owned()),
        ..Refs::default()
    };
    let main = project("up/repo");
    assert_eq!(
        paths(&projects_for("f1", &main, &remotes, &refs)),
        ["me/repo", "up/repo"]
    );
    assert_eq!(
        paths(&projects_for("m1", &main, &remotes, &refs)),
        ["up/repo", "me/repo"]
    );
}

/// A commit no branch points at (an older one) is looked up where HEAD's branch is pushed.
#[test]
fn a_commit_no_branch_has_is_looked_up_on_the_checked_out_branchs_remote() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        local: vec![branch("dev", "d1", "fork/dev")],
        head: "d1".to_owned(),
        current: Some("dev".to_owned()),
        ..Refs::default()
    };
    let main = project("up/repo");
    assert_eq!(
        paths(&projects_for("old", &main, &remotes, &refs)),
        ["me/repo", "up/repo"]
    );
}

/// Found while checking this against a real fork: once `fork/dev` moved on, its old tip
/// was asked of upstream alone, from a branch with no upstream of its own.
#[test]
fn a_commit_no_branch_has_is_still_looked_up_on_every_remote() {
    let remotes = fork_and_upstream();
    let refs = Refs {
        local: vec![branch("topic", "t1", "")],
        remotes: vec![("fork".to_owned(), vec![branch("fork/dev", "newer", "")])],
        head: "t1".to_owned(),
        current: Some("topic".to_owned()),
        ..Refs::default()
    };
    let main = project("up/repo");
    assert_eq!(
        paths(&projects_for("old", &main, &remotes, &refs)),
        ["up/repo", "me/repo"]
    );
}

#[test]
fn the_known_projects_are_those_of_the_remotes_on_the_main_host() {
    let remotes = vec![
        remote("origin", "https://github.com/up/repo.git"),
        remote("fork", "git@github.com:me/repo.git"),
        remote("mirror", "https://gitlab.com/me/repo.git"),
    ];
    assert_eq!(
        paths(&known(&project("up/repo"), &remotes)),
        ["up/repo", "me/repo"]
    );
}

#[test]
fn the_plan_is_cut_to_the_limit() {
    let remotes = fork_and_upstream();
    let refs = Refs::default();
    let main = project("up/repo");
    let sources = Sources {
        main: &main,
        remotes: &remotes,
        refs: &refs,
        prs: vec![],
        recent: vec!["a", "b", "c"],
    };
    assert_eq!(pairs(&plan(&sources, 2)).len(), 2);
}

fn status(state: CheckState, check: &str) -> Status {
    Status {
        state,
        checks: vec![crate::ci::Check {
            name: check.to_owned(),
            state,
            detail: String::new(),
            url: String::new(),
        }],
    }
}

#[test]
fn a_commit_two_projects_build_shows_both_and_opens_in_the_first() {
    let answers: Answers = vec![
        (
            project("me/repo"),
            Ok(HashMap::from([(
                "c".to_owned(),
                status(CheckState::Passed, "fork ci"),
            )])),
        ),
        (
            project("up/repo"),
            Ok(HashMap::from([(
                "c".to_owned(),
                status(CheckState::Running, "up ci"),
            )])),
        ),
    ];
    let merged = merge(answers, &[]);
    assert!(merged.is_ok(), "{merged:?}");
    let Ok(merged) = merged else { return };
    let c = &merged.by_sha["c"];
    assert_eq!(c.state, CheckState::Running);
    assert_eq!(c.checks.len(), 2);
    assert_eq!(merged.source["c"].path, "me/repo");
    assert_eq!(merged.error, None);
}

#[test]
fn a_project_that_fails_keeps_what_it_said_before() {
    let before = vec![
        (
            project("me/repo"),
            HashMap::from([("f".to_owned(), status(CheckState::Passed, "fork ci"))]),
        ),
        (
            project("up/repo"),
            HashMap::from([("u".to_owned(), status(CheckState::Failed, "up ci"))]),
        ),
    ];
    let answers: Answers = vec![
        (project("me/repo"), Err("rate limited".to_owned())),
        (
            project("up/repo"),
            Ok(HashMap::from([(
                "u".to_owned(),
                status(CheckState::Passed, "up ci"),
            )])),
        ),
    ];
    let merged = merge(answers, &before);
    assert!(merged.is_ok(), "{merged:?}");
    let Ok(merged) = merged else { return };
    assert_eq!(
        merged.by_sha["f"].state,
        CheckState::Passed,
        "kept from before"
    );
    assert_eq!(merged.by_sha["u"].state, CheckState::Passed, "read afresh");
    assert_eq!(merged.error.as_deref(), Some("me/repo: rate limited"));
}

/// An upstream outage must not turn a shared commit's cached failure green.
#[test]
fn a_failed_projects_contribution_is_merged_with_fresh_checks_without_duplication() {
    let initial = vec![
        (
            project("me/repo"),
            Ok(HashMap::from([(
                "c".to_owned(),
                status(CheckState::Passed, "fork"),
            )])),
        ),
        (
            project("up/repo"),
            Ok(HashMap::from([(
                "c".to_owned(),
                status(CheckState::Failed, "upstream"),
            )])),
        ),
    ];
    let first = merge(initial, &[]).unwrap_or_default();
    let refresh = || {
        vec![
            (
                project("me/repo"),
                Ok(HashMap::from([(
                    "c".to_owned(),
                    status(CheckState::Passed, "fork new"),
                )])),
            ),
            (project("up/repo"), Err("offline".to_owned())),
        ]
    };
    let second = merge(refresh(), &first.cached).unwrap_or_default();
    let third = merge(refresh(), &second.cached).unwrap_or_default();
    assert_eq!(third.by_sha["c"].state, CheckState::Failed);
    let names: Vec<_> = third.by_sha["c"]
        .checks
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["fork new", "upstream"]);
    assert_eq!(third.error.as_deref(), Some("up/repo: offline"));
    let cleared = merge(
        vec![
            (project("me/repo"), Ok(HashMap::new())),
            (project("up/repo"), Ok(HashMap::new())),
        ],
        &third.cached,
    )
    .unwrap_or_default();
    assert!(
        cleared.by_sha.is_empty(),
        "successful empty responses replace cached checks"
    );
}

#[test]
fn when_every_project_fails_the_errors_name_each_project() {
    let answers: Answers = vec![
        (project("me/repo"), Err("gh isn't signed in".to_owned())),
        (project("up/repo"), Err("network".to_owned())),
    ];
    let result = merge(answers, &[]);
    assert_eq!(
        result.err().as_deref(),
        Some("me/repo: gh isn't signed in; up/repo: network")
    );
}

#[test]
fn projects_with_nothing_to_say_are_not_a_failure() {
    let answers: Answers = vec![(project("up/repo"), Ok(HashMap::new()))];
    let merged = merge(answers, &[]);
    assert!(merged.is_ok(), "{merged:?}");
    let Ok(merged) = merged else { return };
    assert!(merged.by_sha.is_empty());
    assert_eq!(merged.error, None);
}

#[test]
fn one_project_answering_nothing_and_another_failing_is_a_partial_read() {
    let answers: Answers = vec![
        (project("up/repo"), Ok(HashMap::new())),
        (project("me/repo"), Err("network".to_owned())),
    ];
    let merged = merge(answers, &[]);
    assert!(merged.is_ok(), "{merged:?}");
    let Ok(merged) = merged else { return };
    assert_eq!(merged.error.as_deref(), Some("me/repo: network"));
}
