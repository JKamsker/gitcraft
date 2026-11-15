use super::*;

fn fixture() -> (RepoTab, PullRequest) {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/pr-checkout-test".into(), &ctx);
    tab.remotes = vec![crate::git::Remote {
        name: "origin".into(),
        url: String::new(),
    }];
    let pr = PullRequest {
        number: 7,
        title: "Incoming change".into(),
        branch: "feature".into(),
        base: "main".into(),
        base_oid: String::new(),
        from_fork: true,
        head_repo: Some("contributor/fork".into()),
        sha: "a".repeat(40),
        author: "contributor".into(),
        url: "https://github.com/owner/repo/pull/7".into(),
        draft: false,
    };
    (tab, pr)
}
fn branch(name: &str, oid: &str) -> Branch {
    Branch {
        name: name.into(),
        target: oid.into(),
        is_head: false,
        upstream: String::new(),
        track: String::new(),
    }
}

#[test]
fn fork_checkout_never_uses_same_named_local_or_remote_branch() {
    let (mut tab, pr) = fixture();
    tab.refs.local = vec![branch("feature", &"b".repeat(40))];
    for remote_oid in ["b".repeat(40), pr.sha.clone()] {
        tab.refs.remotes = vec![("origin".into(), vec![branch("origin/feature", &remote_oid)])];
        assert!(
            matches!(tab.pr_checkout_action(&pr), Some(Action::Checkout(oid)) if oid == pr.sha)
        );
        // Even a remote at the right commit must not go through checkout_remote, which
        // would silently choose the unrelated same-named local branch.
    }
    tab.refs.local.clear();
    tab.refs.remotes.clear();
    assert!(matches!(tab.pr_checkout_action(&pr), Some(Action::Checkout(oid)) if oid == pr.sha));
}

#[test]
fn fork_without_valid_head_does_not_fall_back_to_a_branch_name() {
    let (mut tab, mut pr) = fixture();
    tab.refs.local = vec![branch("feature", &"b".repeat(40))];
    for invalid in ["", "feature", "-Bother"] {
        pr.sha = invalid.into();
        assert!(tab.pr_checkout_action(&pr).is_none());
    }
}

#[test]
fn same_repository_checkout_keeps_local_remote_and_default_paths() {
    let (mut tab, mut pr) = fixture();
    pr.from_fork = false;
    tab.refs.local = vec![branch("feature", &"b".repeat(40))];
    assert!(
        matches!(tab.pr_checkout_action(&pr), Some(Action::Checkout(name)) if name == "feature")
    );
    tab.refs.local.clear();
    tab.refs.remotes = vec![("origin".into(), vec![branch("origin/feature", &pr.sha)])];
    assert!(
        matches!(tab.pr_checkout_action(&pr), Some(Action::CheckoutRemote(name)) if name == "origin/feature")
    );
    tab.refs.remotes.clear();
    assert!(
        matches!(tab.pr_checkout_action(&pr), Some(Action::CheckoutRemote(name)) if name == "origin/feature")
    );
}
