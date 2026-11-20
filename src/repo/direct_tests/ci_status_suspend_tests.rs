//! A tab suspended while its CI dots are being read (#195): the read's answer goes to the
//! channel the suspend replaced, so the read must not keep the tab from reading them again.

use std::time::Instant;

use super::*;
use crate::forge::{Forge, Hosting, PullRequest};
use crate::git::Remote;
use crate::repo::lifecycle::IDLE_AFTER;
use crate::repo::messages::record_jobs;

/// A GitHub tab whose only commits worth asking about are no commit ids, so a read of their
/// statuses answers at once without going on the network (`ci::statuses` asks nothing).
fn tab(ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/nonexistent/ci-suspend-tests".into(), ctx);
    tab.remotes = vec![Remote {
        name: "origin".into(),
        url: "https://github.com/owner/gitcraft".into(),
    }];
    tab.forge_remotes = vec![(
        "origin".into(),
        Hosting {
            forge: Forge::GitHub,
            host: "github.com".into(),
            path: "owner/gitcraft".into(),
            web: "https://github.com".into(),
        },
    )];
    tab.take_forge_selection(false, ctx);
    tab.prs = vec![PullRequest {
        number: 1,
        title: "x".into(),
        branch: "feat/x".into(),
        base: "main".into(),
        base_oid: String::new(),
        head_repo: None,
        from_fork: false,
        sha: "not-a-commit".into(),
        author: "owner".into(),
        url: "https://github.com/owner/gitcraft/pull/1".into(),
        draft: false,
    }];
    tab
}

/// Runs the one CI read `work` submits and hands its answer to `tab`.
fn read_once(tab: &mut RepoTab, ctx: &egui::Context, work: impl FnOnce(&mut RepoTab)) {
    let ((), jobs) = record_jobs(|| work(tab));
    assert_eq!(jobs.len(), 1, "one CI read is submitted");
    for job in jobs {
        let msg = job();
        assert!(matches!(msg, Msg::CiStatuses(..)));
        tab.handle(msg, ctx);
    }
}

#[test]
fn a_ci_read_lost_to_a_suspend_is_asked_again_after_resuming() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let ctx = egui::Context::default();
    // A background tab's poll starts a read in the frame it is suspended in; its answer
    // never comes, as the suspend replaces the channel it is sent to.
    let (mut tab, _lost) = record_jobs(|| {
        let mut tab = tab(&ctx);
        tab.life.shown(Instant::now());
        tab.load_ci(&ctx);
        tab
    });
    assert!(tab.ci.loading, "a read is on its way");
    tab.life.shown(Instant::now() - IDLE_AFTER * 2);
    tab.background_tick();
    assert!(tab.sleeping());
    tab.shown_now(&ctx);
    // What `take_loaded` does once the history is read again.
    read_once(&mut tab, &ctx, |tab| tab.refresh_ci(&ctx));
    assert!(!tab.ci.loading);
    assert!(tab.ci.error.is_none(), "{:?}", tab.ci.error);
}
