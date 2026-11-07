use super::*;
use crate::forge::{Forge, PullRequest};
use crate::repo::Msg;
use std::time::Instant;

fn remote(name: &str, owner: &str) -> Remote {
    Remote {
        name: name.into(),
        url: format!("https://github.com/{owner}/gitcraft"),
    }
}
fn hosting(owner: &str) -> Hosting {
    Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        path: format!("{owner}/gitcraft"),
        web: "https://github.com".into(),
    }
}
fn tab(ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/nonexistent/forge-selection-tests".into(), ctx);
    tab.remotes = vec![remote("origin", "Vancete"), remote("fork", "JKamsker")];
    tab.forge_remotes = vec![
        ("origin".into(), hosting("Vancete")),
        ("fork".into(), hosting("JKamsker")),
    ];
    tab.take_forge_selection(false, ctx);
    tab
}
fn pr(number: u64, owner: &str) -> PullRequest {
    PullRequest {
        number,
        title: owner.into(),
        branch: "feat/x".into(),
        base: "main".into(),
        from_fork: false,
        sha: "a".repeat(40),
        author: owner.into(),
        url: format!("https://github.com/{owner}/gitcraft/pull/{number}"),
        draft: false,
    }
}

#[test]
fn explicit_origin_then_first_usable_and_no_usable_remote() {
    let hosts = vec![
        ("fork".into(), hosting("JKamsker")),
        ("origin".into(), hosting("Vancete")),
    ];
    let automatic = Selection::default();
    assert_eq!(
        automatic.resolve(&hosts).map(|(n, _)| n.as_str()),
        Some("origin")
    );
    let explicit = Selection {
        name: "fork".into(),
        url: String::new(),
    };
    assert_eq!(
        explicit.resolve(&hosts).map(|(_, h)| &h.path),
        Some(&"JKamsker/gitcraft".into())
    );
    assert_eq!(
        automatic.resolve(&hosts[..1]).map(|(n, _)| n.as_str()),
        Some("fork")
    );
    assert!(automatic.resolve(&[]).is_none());
}

#[test]
fn selection_follows_unique_rename_and_same_name_url_changes_but_removal_falls_back() {
    let mut choice = Selection {
        name: "fork".into(),
        url: remote("fork", "JKamsker").url,
    };
    choice.reconcile(&[remote("renamed", "JKamsker"), remote("origin", "Vancete")]);
    assert_eq!(choice.name, "renamed");
    choice.reconcile(&[remote("renamed", "other")]);
    assert_eq!(choice.url, remote("renamed", "other").url);
    choice.reconcile(&[remote("origin", "Vancete")]);
    assert_eq!(choice, Selection::default());
    choice = Selection {
        name: "gone".into(),
        url: remote("gone", "JKamsker").url,
    };
    choice.reconcile(&[remote("one", "JKamsker"), remote("two", "JKamsker")]);
    assert_eq!(
        choice,
        Selection::default(),
        "ambiguous rename must not guess"
    );
}

#[test]
fn preference_survives_context_serialization_and_is_per_repository() -> Result<(), String> {
    let ctx = egui::Context::default();
    let choice = Selection {
        name: "fork".into(),
        url: remote("fork", "JKamsker").url,
    };
    choice.store(&ctx, Path::new("/repo/one"));
    let encoded = ctx.memory(ron::to_string).map_err(|e| e.to_string())?;
    let restored = egui::Context::default();
    restored.memory_mut(|m| *m = ron::from_str(&encoded).unwrap_or_default());
    assert_eq!(Selection::load(&restored, Path::new("/repo/one")), choice);
    assert_eq!(
        Selection::load(&restored, Path::new("/repo/two")),
        Selection::default()
    );
    Ok(())
}

#[test]
fn selecting_fork_isolates_pr_numbers_ci_and_all_late_hosting_continuations() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let before = tab.remotes.clone();
    let transport = tab.default_remote();
    let old = tab.forge_sender();
    tab.prs = vec![pr(7, "Vancete")];
    tab.versions.seen_heads.insert(7, "old".into());
    tab.set_primary_forge(Some("fork".into()), &ctx);
    assert_eq!(tab.hosting, Some(hosting("JKamsker")));
    assert_eq!(
        tab.ci_project().map(|p| p.path),
        Some("JKamsker/gitcraft".into())
    );
    assert_eq!(tab.remotes, before);
    assert_eq!(
        tab.default_remote(),
        transport,
        "forge preference must not change transport routing"
    );
    assert!(tab.prs.is_empty());
    assert!(tab.versions.seen_heads.is_empty());
    let asked = Instant::now();
    tab.prs_loaded = Some(asked);
    let _ = old.send(Msg::Prs(asked, Ok(vec![pr(7, "Vancete")])));
    let _ = old.send(Msg::CiActed("old CI result".into(), Ok(())));
    let _ = old.send(Msg::Then(Box::new(|tab, _| {
        tab.prs = vec![pr(7, "Vancete")];
        tab.status = Some(("stale issues/link/detail result".into(), true));
    })));
    let _ = tab
        .forge_sender()
        .send(Msg::Prs(asked, Ok(vec![pr(7, "JKamsker")])));
    tab.poll(&ctx);
    assert_eq!(tab.prs, vec![pr(7, "JKamsker")]);
    assert!(tab.status.is_none());
    // Going back does not make the first upstream lifetime's messages valid again.
    tab.set_primary_forge(Some("origin".into()), &ctx);
    let _ = old.send(Msg::Then(Box::new(|tab, _| {
        tab.status = Some(("old A".into(), true))
    })));
    tab.poll(&ctx);
    assert!(tab.status.is_none());
}

#[test]
fn a_late_reload_chooses_the_current_preference_and_url_change_drops_old_results() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    tab.set_primary_forge(Some("fork".into()), &ctx);
    // Remote detection started before that choice. Applying the detected catalog must use
    // the preference now persisted, not its old origin selection.
    tab.take_forge_selection(false, &ctx);
    assert_eq!(tab.primary_forge_remote(), Some("fork"));
    let old = tab.forge_sender();
    tab.remotes[1] = remote("fork", "changed");
    tab.forge_remotes[1].1 = hosting("changed");
    tab.take_forge_selection(true, &ctx);
    assert_eq!(tab.hosting, Some(hosting("changed")));
    let _ = old.send(Msg::CiActed("previous URL".into(), Ok(())));
    tab.poll(&ctx);
    assert!(tab.status.is_none());
    tab.remotes.remove(1);
    tab.forge_remotes.remove(1);
    tab.take_forge_selection(true, &ctx);
    assert_eq!(tab.primary_forge_remote(), Some("origin"));
}

#[test]
fn selecting_a_forge_does_not_write_git_config_refs_or_upstreams() -> Result<(), String> {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let dir =
        std::env::temp_dir().join(format!("gitcraft-primary-transport-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let run = |args: &[&str]| crate::git::run(Some(&dir), args);
    run(&["init", "-q", "-b", "main"])?;
    for r in [remote("origin", "Vancete"), remote("fork", "JKamsker")] {
        run(&["remote", "add", &r.name, &r.url])?;
    }
    run(&["config", "branch.main.remote", "origin"])?;
    run(&["config", "branch.main.merge", "refs/heads/main"])?;
    let config = std::fs::read(dir.join(".git/config")).map_err(|e| e.to_string())?;
    let refs = run(&["show-ref", "--head"]).unwrap_or_default();
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.remotes = crate::git::remotes(&dir)?;
    tab.forge_remotes = crate::forge::detect_remotes(&dir, &tab.remotes);
    tab.take_forge_selection(false, &ctx);
    tab.set_primary_forge(Some("fork".into()), &ctx);
    assert_eq!(tab.primary_forge_remote(), Some("fork"));
    assert_eq!(
        std::fs::read(dir.join(".git/config")).map_err(|e| e.to_string())?,
        config
    );
    assert_eq!(run(&["show-ref", "--head"]).unwrap_or_default(), refs);
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn another_tab_rebinds_before_delivering_old_results_and_does_not_overwrite_a_new_preference() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let ctx = egui::Context::default();
    let mut first = tab(&ctx);
    let mut second = tab(&ctx);
    let old = second.forge_sender();
    first.set_primary_forge(Some("fork".into()), &ctx);
    let _ = old.send(Msg::CiActed("from old source".into(), Ok(())));
    second.poll(&ctx);
    assert_eq!(second.hosting, first.hosting);
    assert!(second.status.is_none());
    let before_rename = second.forge_generation;
    let preferred = Selection {
        name: "renamed".into(),
        url: remote("renamed", "JKamsker").url,
    };
    preferred.store(&ctx, second.repo_key());
    second.poll(&ctx);
    assert_eq!(
        Selection::load(&ctx, second.repo_key()),
        preferred,
        "a tab with an old remote catalog must not rewrite a newer choice"
    );
    assert_eq!(
        second.hosting,
        Some(hosting("JKamsker")),
        "a renamed primary must not temporarily fall back to upstream in a stale tab"
    );
    Selection {
        name: "fork".into(),
        url: remote("fork", "new-owner").url,
    }
    .store(&ctx, second.repo_key());
    second.poll(&ctx);
    assert!(
        second.hosting.is_none(),
        "a newer URL must wait for fresh detection instead of acting on the previous repository"
    );
    second.take_forge_catalog(false, before_rename, &ctx);
    assert_eq!(
        Selection::load(&ctx, second.repo_key()).url,
        remote("fork", "new-owner").url,
        "a reload begun before the new URL was known must not restore the old preference"
    );
    assert!(second.hosting.is_none());
    assert!(second.reload_again);
}

#[test]
fn late_create_completion_releases_busy_slot_without_running_its_old_continuation() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let old = tab.forge_operation_sender();
    tab.busy_op = Some("Create pull request".into());
    // An external URL edit can change hosting even though the UI selector is disabled.
    tab.remotes[0] = remote("origin", "changed");
    tab.forge_remotes[0].1 = hosting("changed");
    tab.take_forge_selection(true, &ctx);
    let _ = old.send(Msg::Then(Box::new(|tab, _| {
        tab.prs = vec![pr(7, "Vancete")]
    })));
    tab.poll(&ctx);
    assert!(tab.busy_op.is_none());
    assert!(tab.prs.is_empty());
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(s, _)| s.contains("previous primary forge"))
    );
}

#[test]
fn unrelated_remote_edits_and_same_repository_renames_do_not_strand_a_posting_review() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let sender = tab.forge_sender();
    tab.pr_drafts.entry(7).or_default().posting = true;
    tab.remotes[1] = remote("fork", "other");
    tab.forge_remotes[1].1 = hosting("other");
    tab.take_forge_selection(true, &ctx);
    tab.remotes[0].name = "renamed".into();
    tab.forge_remotes[0].0 = "renamed".into();
    tab.take_forge_selection(true, &ctx);
    let _ = sender.send(Msg::CiActed("stale refresh after rename".into(), Ok(())));
    let _ = sender.send(Msg::PrPosted {
        hosting: Box::new(hosting("Vancete")),
        number: 7,
        what: crate::repo::pr_review::Posted::Comment,
        result: Ok(()),
    });
    tab.poll(&ctx);
    assert!(!tab.pr_drafts[&7].posting);
    assert!(
        tab.status.is_none(),
        "same-repository edits must still discard stale reads"
    );
}
