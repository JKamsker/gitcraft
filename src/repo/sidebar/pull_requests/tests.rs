use super::*;
use crate::git::{self, Remote};

fn numbered(number: u64) -> PullRequest {
    PullRequest {
        number,
        title: format!("Change {number}"),
        branch: format!("change-{number}"),
        base: "main".into(),
        base_oid: String::new(),
        from_fork: false,
        sha: format!("{number:040x}"),
        author: "ann".into(),
        url: String::new(),
        draft: false,
    }
}

#[test]
fn a_newer_list_waits_while_the_shown_one_is_in_use() {
    let shown = [numbered(1), numbered(2)];
    let fresh = [numbered(3), numbered(1), numbered(2)];
    assert!(!apply_now(&shown, &fresh, Some(0.5)));
    assert!(apply_now(&shown, &fresh, Some(IN_USE_GRACE)));
    assert!(apply_now(&shown, &fresh, None));
    // Nothing would move: the same list, or none shown yet.
    assert!(apply_now(&shown, &shown, Some(0.0)));
    assert!(apply_now(&[], &fresh, Some(0.0)));
}

#[test]
fn a_list_read_while_in_use_is_kept_aside_until_asked_for() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/pr-list-pending".into(), &ctx);
    tab.prs = vec![numbered(1)];
    tab.prs_from_cache = true;
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| d.insert_temp(in_use_id(&tab.path), now));
    tab.prs_arrived(vec![numbered(2), numbered(1)], &ctx);
    assert_eq!(tab.prs, vec![numbered(1)]);
    assert_eq!(tab.pending_prs, Some(vec![numbered(2), numbered(1)]));
    assert!(tab.prs_from_cache); // A pending response does not refresh displayed rows.
    tab.apply_pending_prs(&ctx);
    assert_eq!(tab.prs, vec![numbered(2), numbered(1)]);
    assert!(tab.pending_prs.is_none());
    assert!(!tab.prs_from_cache);
}

#[test]
fn seeded_pr_list_keeps_its_provenance_until_a_fresh_response_is_displayed() {
    let ctx = egui::Context::default();
    let hosting = Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        path: "o/r".into(),
        web: "https://github.com".into(),
    };
    let mut tab = RepoTab::new_deferred("/nonexistent/pr-list-seeded".into(), &ctx);
    let id = cache_id(tab.repo_key(), &hosting);
    ctx.data_mut(|d| d.insert_persisted(id, vec![numbered(7)]));
    tab.seed_prs(&hosting, &ctx);
    assert_eq!(tab.prs, vec![numbered(7)]);
    assert!(tab.prs_from_cache);
    // Even an identical fresh response makes the displayed endpoints trustworthy.
    tab.prs_arrived(vec![numbered(7)], &ctx);
    assert!(!tab.prs_from_cache);
}

#[test]
fn a_read_after_a_failed_one_clears_its_error() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/pr-list-error".into(), &ctx);
    tab.prs = vec![numbered(1)];
    tab.pr_error = Some("timeout".into());
    tab.prs_arrived(vec![numbered(1)], &ctx);
    assert!(tab.pr_error.is_none());
    assert_eq!(tab.prs, vec![numbered(1)]);
}

#[test]
fn review14_pr_branch_lookup_does_not_fall_back_to_another_instance() {
    use crate::forge::Hosting;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/review14-pr-lookup".into(), &ctx);
    let h = Hosting {
        forge: Forge::GitLab,
        host: "forge.example".into(),
        web: "https://forge.example:4000/proxy".into(),
        path: "Group/Project".into(),
    };
    tab.remotes = [
        ("origin", "https://forge.example:3000/proxy/Group/Project"),
        ("case", "https://forge.example:4000/proxy/group/project"),
        ("selected", "https://forge.example:4000/proxy/Group/Project"),
    ]
    .map(|(name, url)| Remote {
        name: name.into(),
        url: url.into(),
    })
    .into();
    tab.forge_remotes = vec![("selected".into(), h)];
    tab.take_forge_selection(false, &ctx);
    tab.refs.remotes = tab
        .remotes
        .iter()
        .map(|r| {
            (
                r.name.clone(),
                vec![branch(&format!("{}/fix", r.name), &r.name)],
            )
        })
        .collect();
    assert_eq!(
        tab.remote_branch_of("fix", "").map(|b| b.name.as_str()),
        Some("selected/fix")
    );
    tab.refs.remotes.retain(|(r, _)| r != "selected");
    assert!(tab.remote_branch_of("fix", "").is_none());
    // A known immutable commit can safely be read from another local object source.
    assert_eq!(
        tab.remote_branch_of("fix", "origin")
            .map(|b| b.name.as_str()),
        Some("origin/fix")
    );
}

fn branch(name: &str, target: &str) -> Branch {
    Branch {
        name: name.to_owned(),
        target: target.to_owned(),
        is_head: false,
        upstream: String::new(),
        track: String::new(),
    }
}

fn pr(branch: &str, sha: &str) -> PullRequest {
    PullRequest {
        number: 1,
        title: String::new(),
        branch: branch.to_owned(),
        base: "main".to_owned(),
        base_oid: String::new(),
        from_fork: false,
        sha: sha.to_owned(),
        author: String::new(),
        url: String::new(),
        draft: false,
    }
}

/// The second click of a double-click must open details without dispatching another
/// selection that could replace the page with a late comparison result.
#[test]
fn a_click_selects_the_whole_pr_and_a_double_click_opens_its_details() {
    for forge in Forge::ALL {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let tab = RepoTab::new_deferred("/nonexistent/pr-click-test".into(), &ctx);
        let pr = PullRequest {
            number: 7,
            title: "Two commits to review".to_owned(),
            url: "https://example.org/team/app/pull/7".to_owned(),
            ..pr("feature", "head")
        };
        let point = pos2(100.0, 18.0);
        let frame = |time: f64, events: Vec<egui::Event>| {
            let mut acts = Vec::new();
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 120.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| tab.pr_row(ui, &mut acts, forge, &pr, true));
                },
            )
            .drop_without_applying_deltas();
            acts
        };
        let click = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(1.0, vec![egui::Event::PointerMoved(point)]);
        let first = frame(1.1, vec![click(true), click(false)]);
        assert!(
            matches!(first.as_slice(), [Action::SelectPr(7)]),
            "{forge:?}"
        );
        let second = frame(1.2, vec![click(true), click(false)]);
        if forge.has_pages() {
            assert!(
                matches!(second.as_slice(), [Action::OpenPr(7)]),
                "{forge:?}"
            );
        } else {
            assert!(
                matches!(second.as_slice(), [
                    Action::OpenUrl(url),
                    Action::Versions(crate::repo::versions::VersionsAct::PrSeen(7)),
                ] if url == &pr.url),
                "{forge:?}"
            );
        }
    }
}

#[test]
fn a_pull_request_branch_is_found_on_the_remote_that_has_it() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/pr-remote-test".into(), &ctx);
    tab.remotes = ["origin", "fork"]
        .map(|name| Remote {
            name: name.to_owned(),
            url: String::new(),
        })
        .into();
    tab.refs.remotes = vec![
        ("origin".to_owned(), vec![branch("origin/main", "m")]),
        (
            "fork".to_owned(),
            vec![branch("fork/feat", "f"), branch("fork/main", "fm")],
        ),
    ];
    let name = |pr: &PullRequest| tab.pr_remote_branch(pr).map(|b| b.name.clone());
    // Only on the fork: not `origin/feat`, which does not exist.
    assert_eq!(name(&pr("feat", "f")).as_deref(), Some("fork/feat"));
    assert_eq!(name(&pr("feat", "")).as_deref(), Some("fork/feat"));
    // On both: the one at the pull request's commit, else the default remote's.
    assert_eq!(name(&pr("main", "fm")).as_deref(), Some("fork/main"));
    assert_eq!(name(&pr("main", "x")).as_deref(), Some("origin/main"));
    assert_eq!(name(&pr("gone", "x")), None);
}

fn worktree(path: &str, branch: &str, current: bool) -> git::Worktree {
    git::Worktree {
        path: path.into(),
        branch: Some(branch.to_owned()),
        current,
        main: path == "/r/app",
        ..git::Worktree::default()
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// On another machine, the worktree goes beside the repository there, named to git as a
/// path of that machine, and nothing is asked of it to pick the folder.
#[test]
fn a_pull_request_on_another_machine_gets_a_worktree_there() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("ssh://build/home/me/app".into(), &ctx);
    tab.refs.local = vec![branch("main", "m"), branch("feat", "f")];
    tab.worktrees = vec![
        worktree("ssh://build/home/me/app", "main", true),
        worktree("ssh://build/home/me/app.wt/feat", "other", false),
    ];
    assert_eq!(
        tab.pr_worktree_plan(3, "feat", "f"),
        PrWorktree::Create {
            ops: vec![
                strings(&["worktree", "prune"]),
                strings(&["worktree", "add", "--", "/home/me/app.wt/feat-2", "feat"]),
            ],
            path: PathBuf::from("ssh://build/home/me/app.wt/feat-2"),
        }
    );
}

/// A saved root takes precedence over the default, including from linked checkouts.
#[test]
fn a_pull_request_worktree_uses_the_saved_root() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("ssh://build/elsewhere/linked".into(), &ctx);
    tab.worktrees = vec![worktree("ssh://build/home/me/app", "main", false)];
    tab.worktree_root = "custom.worktrees".to_owned();
    tab.refs.local = vec![branch("feat/awesome-stuff", "f")];
    let path = "/home/me/app/custom.worktrees/feat/awesome-stuff";
    assert_eq!(
        tab.pr_worktree_plan(3, "feat/awesome-stuff", "f"),
        PrWorktree::Create {
            ops: vec![
                strings(&["worktree", "prune"]),
                strings(&["worktree", "add", "--", path, "feat/awesome-stuff"])
            ],
            path: format!("ssh://build{path}").into(),
        }
    );
}

#[test]
fn a_pull_request_opens_in_its_worktree_made_when_needed() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/r/app".into(), &ctx);
    let hosting = |forge| crate::forge::Hosting {
        forge,
        host: "example.org".to_owned(),
        path: "team/app".to_owned(),
        web: "https://example.org".to_owned(),
    };
    tab.hosting = Some(hosting(Forge::GitHub));
    tab.remotes = vec![Remote {
        name: "origin".to_owned(),
        url: String::new(),
    }];
    tab.refs.local = vec![
        branch("main", "m"),
        branch("mine", "x"),
        branch("feat", "f"),
        branch("pr-9", "elsewhere"),
    ];
    tab.refs.remotes = vec![(
        "origin".to_owned(),
        vec![branch("origin/main", "m"), branch("origin/feat", "f2")],
    )];
    tab.worktrees = vec![
        worktree("/nonexistent/r/app", "main", true),
        worktree("/nonexistent/r/app-mine", "mine", false),
    ];
    // Joined as the code joins it (with `\\` on Windows).
    let folder = |name: &str| Path::new("/nonexistent/r/app.wt").join(name);
    let prune = strings(&["worktree", "prune"]);
    // As this system joins paths (with `\\` on Windows).
    let arg = |name: &str| folder(name).display().to_string();
    let add = |name: &str| strings(&["worktree", "add", "--", &arg(name), name]);
    // Checked out here, or in another worktree.
    assert_eq!(tab.pr_worktree_plan(1, "main", "m"), PrWorktree::Here);
    assert_eq!(
        tab.pr_worktree_plan(2, "mine", "x"),
        PrWorktree::Open(PathBuf::from("/nonexistent/r/app-mine"))
    );
    // A local branch at the pull request's commit: a worktree on it.
    assert_eq!(
        tab.pr_worktree_plan(3, "feat", "f"),
        PrWorktree::Create {
            ops: vec![prune.clone(), add("feat")],
            path: folder("feat"),
        }
    );
    // The remote branch is at it and the local one isn't: a branch of its own from the
    // remote one, the local one left alone.
    assert_eq!(
        tab.pr_worktree_plan(3, "feat", "f2"),
        PrWorktree::Create {
            ops: vec![
                prune.clone(),
                strings(&[
                    "worktree",
                    "add",
                    "-b",
                    "pr-3",
                    "--",
                    &arg("pr-3"),
                    "origin/feat",
                ]),
            ],
            path: folder("pr-3"),
        }
    );
    // Only on the remote: a local branch tracking it, made with the worktree.
    tab.refs.remotes[0].1.push(branch("origin/fix/x", "fx"));
    assert_eq!(
        tab.pr_worktree_plan(4, "fix/x", "fx"),
        PrWorktree::Create {
            ops: vec![
                prune.clone(),
                strings(&[
                    "worktree",
                    "add",
                    "--track",
                    "-b",
                    "fix/x",
                    "--",
                    &arg("fix/x"),
                    "origin/fix/x",
                ]),
            ],
            path: folder("fix/x"),
        }
    );
    // From a fork that isn't a remote: fetched from the pull request's ref (never forced).
    let fetch = |n: u64, name: &str| {
        strings(&[
            "fetch",
            "--",
            "origin",
            &format!("refs/pull/{n}/head:refs/heads/{name}"),
        ])
    };
    assert_eq!(
        tab.pr_worktree_plan(5, "topic", "t"),
        PrWorktree::Create {
            ops: vec![prune.clone(), fetch(5, "topic"), add("topic")],
            path: folder("topic"),
        }
    );
    // A fork's `main`, though ours is a branch here and on origin: not ours, fetched into a
    // branch of its own.
    assert_eq!(
        tab.pr_worktree_plan(6, "main", "fork-main"),
        PrWorktree::Create {
            ops: vec![prune.clone(), fetch(6, "pr-6"), add("pr-6")],
            path: folder("pr-6"),
        }
    );
    // A branch name git would take as an option never reaches it as one.
    assert_eq!(
        tab.pr_worktree_plan(8, "-Bvictim", "v"),
        PrWorktree::Create {
            ops: vec![prune.clone(), fetch(8, "pr-8"), add("pr-8")],
            path: folder("pr-8"),
        }
    );
    // A `pr-N` of the user's own elsewhere: fetched into without force, which git refuses.
    assert!(matches!(
        tab.pr_worktree_plan(9, "-x", "new"),
        PrWorktree::Create { ops, .. } if ops[1] == fetch(9, "pr-9")
    ));
    // Forgejo and Gitea keep them where GitHub does; GitLab, under its own name.
    tab.hosting = Some(hosting(Forge::Forgejo));
    assert!(matches!(
        tab.pr_worktree_plan(7, "topic", "t"),
        PrWorktree::Create { ops, .. } if ops[1][3] == "refs/pull/7/head:refs/heads/topic"
    ));
    tab.hosting = Some(hosting(Forge::GitLab));
    assert!(matches!(
        tab.pr_worktree_plan(7, "topic", "t"),
        PrWorktree::Create { ops, .. } if ops[1][3] == "refs/merge-requests/7/head:refs/heads/topic"
    ));
}

#[test]
fn only_plain_branch_names_are_taken_as_they_are() {
    for good in ["main", "fix/x", "feature-1", "a.b"] {
        assert!(plain_branch_name(good), "{good}");
    }
    for bad in [
        "", "-Bx", "--force", "a..b", "a b", "a:b", "a~1", "x.lock", "/a", "a/", "a//b", "@",
        "a@{1}", "a/.b", ".a", "a\\b",
    ] {
        assert!(!plain_branch_name(bad), "{bad}");
    }
}
