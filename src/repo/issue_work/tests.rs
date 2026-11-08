//! Starting work on an issue in real repositories, through the steps the tab runs: the branch or
//! worktree made, named after the issue, and the link to the issue kept and read back; and, in a
//! tab, where the branch starts from and what the picker would say.

use std::path::{Path, PathBuf};

use super::link::{self, IssueLink, Place, Source, Start};
use super::naming::branch_name;
use super::{IssueAct, Plan};
use crate::forge::Issue;
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{OpKind, RepoTab};

/// A repository with a commit on `main`, and `origin/main` a commit ahead of it (the remote
/// has moved on since the last pull), in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = scratch(name);
    for args in [
        &["init", "-q"][..],
        &["symbolic-ref", "HEAD", "refs/heads/main"],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["commit", "-q", "--allow-empty", "-m", "on the remote"],
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        &["reset", "-q", "--hard", "HEAD~1"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    dir
}

/// A fresh, empty folder named after `name` (and nothing beside it left from a run before).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-issue-{name}-{}", std::process::id()));
    clean(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn worktree_dir(repo: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wt", repo.display()))
}

fn clean(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    for beside in ["wt", "remote", "seed", "clone"] {
        let _ = std::fs::remove_dir_all(format!("{}-{beside}", dir.display()));
    }
}

fn issue(number: u64, title: &str) -> Issue {
    Issue {
        tracker: None,
        acceptance: None,
        number,
        title: title.into(),
        body:
            "Login crashes.\n\n## Acceptance criteria\n- [ ] no crash\n- [x] a test\n\n## Notes\n\
               later"
                .into(),
        url: format!("https://github.com/o/r/issues/{number}"),
        labels: vec!["bug".into()],
    }
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    git::run(Some(dir), args)
        .map(|out| out.trim().to_owned())
        .unwrap_or_default()
}

fn new_from(from: &str) -> Source {
    Source::New { from: from.into() }
}

#[test]
fn a_branch_named_after_the_issue_is_made_and_checked_out_with_its_link() {
    let dir = repo("branch");
    let issue = issue(42, "Fix login crash");
    let link = IssueLink::of(&issue);
    let start = Start {
        branch: branch_name(issue.number, &issue.title),
        source: new_from("refs/remotes/origin/main"),
        place: Place::Here,
        fetch: None,
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let current = git_out(&dir, &["branch", "--show-current"]);
    let at = git_out(&dir, &["rev-parse", "HEAD"]);
    let remote = git_out(&dir, &["rev-parse", "origin/main"]);
    let tracks = git::run(Some(&dir), &["config", "branch.42-fix-login-crash.merge"]);
    let links = link::read(&dir);
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(current, "42-fix-login-crash");
    // From where the remote's default branch is, and not tracking it.
    assert_eq!(at, remote);
    assert!(tracks.is_err(), "{tracks:?}");
    let links = links.unwrap_or_default();
    let read = links.get("42-fix-login-crash");
    assert_eq!(read, Some(&link));
    assert_eq!(
        read.map(|l| l.criteria.as_str()),
        Some("- [ ] no crash\n- [x] a test")
    );
    assert_eq!(read.map(|l| l.read.len()), Some(10));
}

#[test]
fn a_worktree_named_after_the_issue_shares_the_link() {
    let dir = repo("worktree");
    let issue = issue(7, "Füße — ünïcode 日本語 🎉");
    let link = IssueLink::of(&issue);
    let name = branch_name(issue.number, &issue.title);
    let folder = worktree_dir(&dir);
    let start = Start {
        branch: name.clone(),
        source: new_from("refs/remotes/origin/main"),
        place: Place::NewWorktree(folder.to_string_lossy().into_owned()),
        fetch: None,
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let here = git_out(&dir, &["branch", "--show-current"]);
    let there = git_out(&folder, &["branch", "--show-current"]);
    // Read from the worktree, as its own tab reads them.
    let links = link::read(&folder).unwrap_or_default();
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(name, "7-füße-ünïcode-日本語");
    assert_eq!(here, "main");
    assert_eq!(there, name);
    assert_eq!(links.get(&name), Some(&link));
}

#[test]
fn a_branch_there_already_is_checked_out_as_it_is() {
    let dir = repo("exists");
    assert!(git::run(Some(&dir), &["branch", "3-again"]).is_ok());
    let before = git_out(&dir, &["rev-parse", "3-again"]);
    let link = IssueLink::of(&issue(3, "Again"));
    let start = Start {
        branch: "3-again".into(),
        source: Source::Exists,
        place: Place::Here,
        fetch: None,
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let current = git_out(&dir, &["branch", "--show-current"]);
    let after = git_out(&dir, &["rev-parse", "HEAD"]);
    let links = link::read(&dir).unwrap_or_default();
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!((current.as_str(), after), ("3-again", before));
    assert_eq!(links.get("3-again").map(|l| l.number), Some(3));
}

#[test]
fn a_branch_a_deleted_worktree_held_is_checked_out_here() {
    let dir = repo("pruned");
    let folder = worktree_dir(&dir);
    let there = folder.to_string_lossy().into_owned();
    assert!(
        git::run(
            Some(&dir),
            &["worktree", "add", "-q", "-b", "4-held", &there]
        )
        .is_ok()
    );
    // Its folder deleted by hand: git still says the branch is checked out there.
    let _ = std::fs::remove_dir_all(&folder);
    let refused = git::run(Some(&dir), &["checkout", "4-held"]);
    let start = Start {
        branch: "4-held".into(),
        source: Source::Exists,
        place: Place::Here,
        fetch: None,
    };
    let link = IssueLink::of(&issue(4, "Held"));
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let current = git_out(&dir, &["branch", "--show-current"]);
    clean(&dir);
    assert!(refused.is_err(), "{refused:?}");
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(current, "4-held");
}

#[test]
fn a_checkout_changes_are_in_the_way_of_leaves_no_branch_and_no_link() {
    let dir = repo("dirty");
    // `main` has a file the remote's branch changed, and it's changed here too.
    let write = |text: &str| std::fs::write(dir.join("a.txt"), text);
    assert!(write("one\n").is_ok());
    for args in [&["add", "a.txt"][..], &["commit", "-q", "-m", "a"]] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    assert!(write("two\n").is_ok());
    for args in [
        &["commit", "-q", "-am", "remote's a"][..],
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        &["reset", "-q", "--hard", "HEAD~1"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    assert!(write("mine\n").is_ok());
    let start = Start {
        branch: "5-dirty".into(),
        source: new_from("refs/remotes/origin/main"),
        place: Place::Here,
        fetch: None,
    };
    let link = IssueLink::of(&issue(5, "Dirty"));
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let branch = git::run(Some(&dir), &["rev-parse", "--verify", "-q", "5-dirty"]);
    let links = link::read(&dir).unwrap_or_default();
    let kept = std::fs::read_to_string(dir.join("a.txt")).unwrap_or_default();
    clean(&dir);
    assert!(result.is_err(), "{result:?}");
    assert!(branch.is_err(), "{branch:?}");
    assert!(links.is_empty(), "{links:?}");
    assert_eq!(kept, "mine\n");
}

#[test]
fn the_link_follows_the_branch_and_can_be_forgotten() {
    let dir = repo("follow");
    let link = IssueLink {
        number: 9,
        title: "-starts like an option".into(),
        criteria: "- [ ] a\n\n  - [ ] nested: yes".into(),
        read: "2026-11-06".into(),
        ..Default::default()
    };
    let steps = [
        &["branch", "9-x"][..],
        &["branch", "release.v1.2"],
        &["branch", "other"],
        &["branch", "old"],
    ];
    for args in steps {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let mut ops = link::write_ops("9-x", &link);
    ops.extend(link::write_ops("release.v1.2", &link));
    let (wrote, _) = run_steps(&dir, &ops, OpKind::plain(), None);
    // A link written before gitcraft kept what and when: its number and title alone.
    for args in [
        &["config", "branch.old.gitcraft-issue", "3"][..],
        &["config", "branch.old.gitcraft-issue-title", "Old"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let written = link::read(&dir).unwrap_or_default();
    let _ = git::run(Some(&dir), &["branch", "-m", "9-x", "9-renamed"]);
    let renamed = link::read(&dir).unwrap_or_default();
    let unlinked = link::unlink(&dir, "9-renamed");
    let old_unlinked = link::unlink(&dir, "old");
    let _ = git::run(Some(&dir), &["branch", "-D", "release.v1.2"]);
    let after = link::read(&dir);
    clean(&dir);
    assert!(wrote.is_ok(), "{wrote:?}");
    assert_eq!(written.get("9-x"), Some(&link));
    assert_eq!(written.get("release.v1.2"), Some(&link));
    assert_eq!(written.get("old").map(|l| l.number), Some(3));
    assert!(!written.contains_key("other"));
    assert_eq!(renamed.get("9-renamed"), Some(&link));
    assert!(!renamed.contains_key("9-x"));
    assert_eq!(unlinked, Ok(()));
    // The settings it never had are no error.
    assert_eq!(old_unlinked, Ok(()));
    // No branch has one any more: git says so with exit code 1, which is no error.
    assert_eq!(after.map(|l| l.len()), Ok(0));
}

/// A remote whose default branch (its `HEAD`) is `dev`, with a `master` beside it, and a clone
/// of it; then `dev` moves on, on the remote, without the clone fetching. The clone's folder.
fn cloned(name: &str) -> PathBuf {
    let dir = scratch(name);
    let _ = std::fs::remove_dir_all(&dir);
    let remote = format!("{}-remote", dir.display());
    let seed = PathBuf::from(format!("{}-seed", dir.display()));
    let _ = std::fs::create_dir_all(&seed);
    let ok = |dir: Option<&Path>, args: &[&str]| {
        assert!(git::run(dir, args).is_ok(), "{args:?}");
    };
    ok(None, &["init", "-q", "--bare", &remote]);
    for args in [
        &["init", "-q"][..],
        &["symbolic-ref", "HEAD", "refs/heads/master"],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "master"],
        &["remote", "add", "origin", &remote],
        &["push", "-q", "origin", "master"],
        &["checkout", "-q", "-b", "dev"],
        &["commit", "-q", "--allow-empty", "-m", "dev"],
        &["push", "-q", "origin", "dev"],
        &["checkout", "-q", "-b", "8-elsewhere"],
        &["commit", "-q", "--allow-empty", "-m", "started elsewhere"],
        &["push", "-q", "origin", "8-elsewhere"],
        &["checkout", "-q", "dev"],
    ] {
        ok(Some(&seed), args);
    }
    ok(
        None,
        &[
            "--git-dir",
            &remote,
            "symbolic-ref",
            "HEAD",
            "refs/heads/dev",
        ],
    );
    let clone = dir.to_string_lossy().into_owned();
    ok(None, &["clone", "-q", "--no-checkout", &remote, &clone]);
    for args in [
        &["checkout", "-q", "master"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        ok(Some(&dir), args);
    }
    ok(
        Some(&seed),
        &["commit", "-q", "--allow-empty", "-m", "dev moved on"],
    );
    ok(Some(&seed), &["push", "-q", "origin", "dev"]);
    dir
}

/// A tab on `dir`, its first read in.
fn tab(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.reload(ctx);
    wait(&mut tab, ctx);
    tab
}

/// Until the tab has read the repository again, and isn't running anything.
fn wait(tab: &mut RepoTab, ctx: &egui::Context) {
    for _ in 0..400 {
        tab.poll(ctx);
        if !tab.loading && tab.busy_op.is_none() && tab.history_gen > 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn start(tab: &mut RepoTab, ctx: &egui::Context, issue: Issue, branch: &str, worktree: bool) {
    tab.issue_act(
        IssueAct::Start {
            issue: Box::new(issue),
            branch: branch.to_owned(),
            worktree,
        },
        ctx,
    );
    wait(tab, ctx);
    // The reload the operation asked for.
    tab.reload(ctx);
    wait(tab, ctx);
}

fn note(plan: &Plan) -> String {
    match &plan.start {
        Ok(_) => plan.note.clone(),
        Err(why) => format!("refused: {why}"),
    }
}

#[test]
fn work_starts_from_the_remotes_default_branch_as_it_is_now() {
    let dir = cloned("remote-head");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let base = tab.issue_base();
    let plan = tab.issue_plan(21, "21-x", false);
    let said = note(&plan);
    start(&mut tab, &ctx, issue(21, "X"), "21-x", false);
    let at = git_out(&dir, &["rev-parse", "21-x"]);
    let remote_dev = git_out(
        Path::new(&format!("{}-remote", dir.display())),
        &["rev-parse", "dev"],
    );
    let master = git_out(&dir, &["rev-parse", "origin/master"]);
    let current = git_out(&dir, &["branch", "--show-current"]);
    let status = tab.status.as_deref().cloned();
    let pr_base = tab.issue_pull_request("21-x").map(|(_, _, base)| base);
    clean(&dir);
    assert_eq!((base.name.as_str(), base.known), ("dev", true));
    assert!(
        said.contains("origin/dev (the remote's default branch), fetched first"),
        "{said}"
    );
    // From dev as the remote has it now (fetched first), not master, nor dev as last fetched.
    assert_eq!(at, remote_dev);
    assert_ne!(at, master);
    assert_eq!(current, "21-x");
    assert_eq!(status, Some(("Start work on #21: done".to_owned(), false)));
    // Its pull request goes into the branch it started from.
    assert_eq!(pr_base.as_deref(), Some("dev"));
}

#[test]
fn offline_it_starts_from_the_branch_as_last_fetched_and_says_so() {
    let dir = cloned("offline");
    let gone = format!("{}-gone", dir.display());
    assert!(git::run(Some(&dir), &["remote", "set-url", "origin", &gone]).is_ok());
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    start(&mut tab, &ctx, issue(22, "Y"), "22-y", false);
    let at = git_out(&dir, &["rev-parse", "22-y"]);
    let fetched = git_out(&dir, &["rev-parse", "origin/dev"]);
    let status = tab.status.as_deref().cloned().unwrap_or_default();
    let links = link::read(&dir).unwrap_or_default();
    clean(&dir);
    assert_eq!(at, fetched);
    assert!(
        status.0.contains(
            "done, but couldn't fetch origin, so it starts from origin/dev as last fetched"
        ),
        "{status:?}"
    );
    assert_eq!(links.get("22-y").map(|l| l.number), Some(22));
}

#[test]
fn a_branch_only_the_remote_has_is_tracked() {
    let dir = cloned("tracked");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let said = note(&tab.issue_plan(8, "8-elsewhere", false));
    start(&mut tab, &ctx, issue(8, "Elsewhere"), "8-elsewhere", false);
    let upstream = git_out(
        &dir,
        &["rev-parse", "--abbrev-ref", "8-elsewhere@{upstream}"],
    );
    let at = git_out(&dir, &["rev-parse", "8-elsewhere"]);
    let there = git_out(&dir, &["rev-parse", "origin/8-elsewhere"]);
    clean(&dir);
    assert!(
        said.contains("origin/8-elsewhere is there: a branch tracking it"),
        "{said}"
    );
    assert_eq!(upstream, "origin/8-elsewhere");
    assert_eq!(at, there);
}

#[test]
fn what_the_picker_says_is_what_happens() {
    let dir = cloned("plans");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    // The default branch is no branch of an issue's.
    let refused = note(&tab.issue_plan(1, "dev", false));
    assert!(
        refused.starts_with("refused: dev is the default branch"),
        "{refused}"
    );
    let bad = note(&tab.issue_plan(1, "x.lock", false));
    assert!(bad.contains(".lock"), "{bad}");
    // A branch for another issue already.
    for args in [
        &["branch", "6-taken"][..],
        &["config", "branch.6-taken.gitcraft-issue", "6"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let _ = std::fs::write(dir.join("wip.txt"), "changes\n");
    tab.reload(&ctx);
    wait(&mut tab, &ctx);
    let taken = tab.issue_plan(7, "6-taken", false);
    let worktree = tab.issue_plan(7, "6-taken", true);
    let fresh = tab.issue_plan(7, "7-fresh", true);
    clean(&dir);
    assert!(
        taken
            .warnings
            .iter()
            .any(|w| w.contains("6-taken is for #6 already")),
        "{:?}",
        taken.warnings
    );
    // Here, the changes go along; in a new worktree they stay.
    assert!(
        taken
            .warnings
            .iter()
            .any(|w| w.contains("working tree has changes")),
        "{:?}",
        taken.warnings
    );
    assert!(
        !worktree.warnings.iter().any(|w| w.contains("working tree")),
        "{:?}",
        worktree.warnings
    );
    assert!(note(&taken).contains("exists already"), "{}", note(&taken));
    assert!(
        matches!(
            &worktree.start,
            Ok(Start {
                place: Place::NewWorktree(_),
                source: Source::Exists,
                ..
            })
        ),
        "{:?}",
        worktree.start
    );
    assert!(
        note(&fresh).contains("in a new worktree"),
        "{}",
        note(&fresh)
    );
    assert!(fresh.open.is_some());
}

#[test]
fn a_saved_upstream_issue_is_not_refreshed_or_closed_by_number_on_the_fork() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/issue-forge-isolation".into(), &ctx);
    let mut link = IssueLink::of(&issue(7, "Upstream issue"));
    link.url = "https://github.com/Vancete/gitcraft/issues/7".into();
    tab.issue_work.links.insert("topic".into(), link);
    tab.hosting = Some(crate::forge::Hosting {
        forge: crate::forge::Forge::GitHub,
        host: "github.com".into(),
        path: "JKamsker/gitcraft".into(),
        web: "https://github.com".into(),
    });
    assert!(
        tab.issue_pull_request("topic").is_none(),
        "must not generate Closes #7 on the other repository"
    );
    tab.refresh_issue("topic".into(), &ctx);
    assert!(tab.issue_work.refreshing.is_none());
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|n| n.0.contains("another forge repository"))
    );
    if let Some(hosting) = &mut tab.hosting {
        hosting.path = "Vancete/gitcraft".into();
    }
    assert!(tab.issue_pull_request("topic").is_some());
}

// Captured on the caller's thread before dispatch, so an accidental scheduled refresh is
// recorded and receives a local answer even though the actual job runs on another thread.
struct RefreshRecording {
    answer: Issue,
    calls: Vec<(crate::forge::Hosting, u64)>,
}

thread_local! {
    static REFRESH_RECORDING: std::cell::RefCell<Option<RefreshRecording>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn record_refresh(hosting: &crate::forge::Hosting, number: u64) -> Option<Issue> {
    REFRESH_RECORDING.with(|r| {
        let mut r = r.borrow_mut();
        let recording = r.as_mut()?;
        recording.calls.push((hosting.clone(), number));
        Some(recording.answer.clone())
    })
}

fn recorded_refresh(answer: Issue, work: impl FnOnce()) -> Vec<(crate::forge::Hosting, u64)> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            REFRESH_RECORDING.with(|r| *r.borrow_mut() = None);
        }
    }
    REFRESH_RECORDING.with(|r| {
        assert!(r.borrow().is_none());
        *r.borrow_mut() = Some(RefreshRecording {
            answer,
            calls: Vec::new(),
        });
    });
    let _reset = Reset;
    work();
    REFRESH_RECORDING.with(|r| r.borrow_mut().take().map(|r| r.calls).unwrap_or_default())
}

fn review13_cases() -> Vec<(crate::forge::Hosting, String, bool)> {
    use crate::forge::{Forge, Hosting};
    let mut cases = Vec::new();
    for forge in Forge::ALL {
        let h = Hosting {
            forge,
            host: "forgejo.example".into(),
            path: "Team/Repo".into(),
            web: "https://forgejo.example:4000/reviews".into(),
        };
        let marker = if forge == Forge::GitLab {
            "-/issues"
        } else {
            "issues"
        };
        for (base, path, expected) in [
            ("https://forgejo.example:3000/reviews", "Team/Repo", false),
            ("http://forgejo.example:4000/reviews", "Team/Repo", false),
            ("https://forgejo.example:4000/REVIEWS", "Team/Repo", false),
            ("https://forgejo.example:4000/other", "Team/Repo", false),
            ("https://forgejo.example:4000", "Team/Repo", false),
            ("https://forgejo.example:4000/reviews", "Other/Repo", false),
            ("HTTPS://FORGEJO.example:4000/reviews", "Team/Repo", true),
            (
                "https://forgejo.example:4000/reviews",
                "team/repo",
                !matches!(forge, Forge::GitLab | Forge::Azure | Forge::Bitbucket),
            ),
        ] {
            cases.push((h.clone(), format!("{base}/{path}/{marker}/7"), expected));
        }
        let standard = Hosting {
            web: "https://forgejo.example".into(),
            ..h.clone()
        };
        cases.push((
            standard,
            format!("https://FORGEJO.example:443/Team/Repo/{marker}/7"),
            true,
        ));
        // The exact reported case, without a proxy prefix, must also fail.
        cases.push((
            Hosting {
                web: "https://forgejo.example:4000".into(),
                ..h
            },
            format!("https://forgejo.example:3000/Team/Repo/{marker}/7"),
            false,
        ));
    }
    cases
}

#[test]
fn review13_saved_issue_pr_preparation_uses_canonical_instance_identity() {
    for (hosting, url, expected) in review13_cases() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/saved-issue-pr".into(), &ctx);
        let mut saved = issue(7, "Saved issue title");
        saved.url = url.clone();
        tab.issue_work
            .links
            .insert("topic".into(), IssueLink::of(&saved));
        tab.hosting = Some(hosting.clone());
        // A selected SSH origin has the same captured web identity; the URL guard must not
        // try to infer its web port/prefix from that Git transport.
        tab.remotes = vec![crate::git::Remote {
            name: "origin".into(),
            url: "ssh://git@forgejo.example:2222/Team/Repo.git".into(),
        }];
        let prepared = tab.issue_pull_request("topic");
        assert_eq!(prepared.is_some(), expected, "{hosting:?} {url}");
        if let Some((title, body, _)) = prepared {
            assert_eq!(title, saved.title);
            assert_eq!(body, "Closes #7");
        }
    }
}

fn review13_refresh_cases(expected: bool, fixture: &str) {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let dir = repo(fixture);
    assert!(git::run(Some(&dir), &["branch", "topic"]).is_ok());
    for (hosting, url, _) in review13_cases()
        .into_iter()
        .filter(|(_, _, matches)| *matches == expected)
    {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let mut saved = issue(7, "Original title");
        saved.url = url.clone();
        let saved = IssueLink::of(&saved);
        let (wrote, _) = run_steps(
            &dir,
            &link::write_ops("topic", &saved),
            OpKind::plain(),
            None,
        );
        assert!(wrote.is_ok(), "{wrote:?}");
        let before = std::fs::read(dir.join(".git/config")).unwrap_or_default();
        tab.issue_work.links.insert("topic".into(), saved.clone());
        tab.hosting = Some(hosting.clone());
        let mut answer = issue(7, "Refreshed selected issue");
        let marker = if hosting.forge == crate::forge::Forge::GitLab {
            "-/issues"
        } else {
            "issues"
        };
        answer.url = format!("{}/{}/{marker}/7", hosting.web, hosting.path);
        let refreshed = IssueLink::of(&answer);
        let calls = recorded_refresh(answer, || {
            tab.refresh_issue("topic".into(), &ctx);
            for _ in 0..400 {
                tab.poll(&ctx);
                if tab.issue_work.refreshing.is_none() && tab.busy_op.is_none() && !tab.loading {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        });
        assert!(tab.issue_work.refreshing.is_none() && tab.busy_op.is_none() && !tab.loading);
        if expected {
            assert_eq!(calls, vec![(hosting.clone(), 7)], "{url}");
            assert_eq!(
                link::read(&dir).unwrap_or_default().get("topic"),
                Some(&refreshed)
            );
            assert_eq!(tab.issue_work.links.get("topic"), Some(&refreshed));
        } else {
            assert!(
                calls.is_empty(),
                "must refuse before scheduling a read: {hosting:?} {url}"
            );
            assert_eq!(
                std::fs::read(dir.join(".git/config")).unwrap_or_default(),
                before,
                "metadata changed for {url}"
            );
            assert_eq!(
                link::read(&dir).unwrap_or_default().get("topic"),
                Some(&saved)
            );
            assert_eq!(tab.issue_work.links.get("topic"), Some(&saved));
            assert!(
                tab.status
                    .as_ref()
                    .is_some_and(|n| n.0.contains("another forge repository"))
            );
        }
    }
    clean(&dir);
}

#[test]
fn review13_saved_issue_refresh_refuses_mismatch_and_preserves_metadata() {
    review13_refresh_cases(false, "instance-refused");
}

#[test]
fn review13_saved_issue_refresh_accepts_same_canonical_instance() {
    review13_refresh_cases(true, "instance-accepted");
}

#[test]
fn review14_issue_remote_branch_uses_primary_or_same_canonical_repository_only() {
    use crate::forge::{Forge, Hosting};
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/review14-issues".into(), &ctx);
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
        ("ssh", "ssh://git@forge.example:2222/Group/Project.git"),
    ]
    .map(|(name, url)| git::Remote {
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
                vec![git::Branch {
                    name: format!("{}/7-fix", r.name),
                    target: r.name.clone(),
                    is_head: false,
                    upstream: String::new(),
                    track: String::new(),
                }],
            )
        })
        .collect();
    if let Some((_, branches)) = tab
        .refs
        .remotes
        .iter_mut()
        .find(|(name, _)| name == "selected")
    {
        branches.push(git::Branch {
            name: "selected/develop".into(),
            target: "selected-base".into(),
            is_head: false,
            upstream: String::new(),
            track: String::new(),
        });
    }
    assert_eq!(
        tab.issue_base().name,
        "develop",
        "the forge's fallback base must not use origin"
    );
    assert_eq!(
        tab.remote_branch_named("7-fix").as_deref(),
        Some("selected/7-fix")
    );
    tab.refs.remotes.retain(|(r, _)| r != "selected");
    assert_eq!(
        tab.remote_branch_named("7-fix").as_deref(),
        Some("ssh/7-fix")
    );
    tab.refs.remotes.retain(|(r, _)| r != "ssh");
    assert!(tab.remote_branch_named("7-fix").is_none());
}

#[test]
fn review14_queued_issue_start_from_previous_forge_is_refused() {
    use crate::forge::{Forge, Hosting};
    let dir = repo("stale-picker");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.hosting = Some(Hosting {
        forge: Forge::Forgejo,
        host: "forge.example".into(),
        web: "https://forge.example:4000".into(),
        path: "team/repo".into(),
    });
    let mut old = issue(7, "A's issue");
    old.url = "https://forge.example:3000/team/repo/issues/7".into();
    let before = git::head(&dir);
    let config = std::fs::read(dir.join(".git/config")).unwrap_or_default();
    tab.issue_act(
        IssueAct::Start {
            issue: Box::new(old),
            branch: "7-fix".into(),
            worktree: false,
        },
        &ctx,
    );
    assert!(tab.busy_op.is_none());
    assert_eq!(git::head(&dir), before);
    assert_eq!(
        std::fs::read(dir.join(".git/config")).unwrap_or_default(),
        config
    );
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|n| n.0.contains("another primary forge"))
    );
    clean(&dir);
}
