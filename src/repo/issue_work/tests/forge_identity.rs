// Included in the parent test module to preserve qualified test names.

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
