//! The markers' reads in a tab (when they start, which answers count, what an error keeps)
//! and the ancestry check against a real repository.

use super::model::tests::fixture_markers;
use super::pills::on_commit;
use super::*;
use crate::repo::insights::test_support::{required, tab};
use crate::repo::insights::{InsightsAct, Kind};
use crate::repo::messages;

fn read() -> Read {
    Read {
        markers: fixture_markers(),
        partial: Vec::new(),
        in_flight: false,
    }
}

/// Shows the markers for a frame, recording (not running) the read it starts.
fn frame(tab: &mut RepoTab, ctx: &egui::Context, visible: bool) -> usize {
    let (_, jobs) = messages::record_jobs(|| tab.deploy_markers_frame(visible, ctx));
    jobs.len()
}

/// Runs `test` with this thread's account pinned: other tests switch the process's.
fn pinned<T>(test: impl FnOnce() -> T) -> T {
    crate::connections::Context::default().run(test)
}

#[test]
fn markers_land_on_their_commits_with_previews_apart() {
    pinned(|| {
        let markers = fixture_markers();
        let (named, previews) = on_commit(&markers, &"b".repeat(40));
        assert_eq!(
            named.iter().map(|m| &m.environment).collect::<Vec<_>>(),
            ["staging"]
        );
        assert_eq!(previews.len(), 2);
        // An abbreviated id finds the same commit.
        let (named, _) = on_commit(&markers, "aaaaaaa");
        assert_eq!(named[0].environment, "production");
        let (named, previews) = on_commit(&markers, &"d".repeat(40));
        assert!(named.is_empty() && previews.is_empty());
    })
}

#[test]
fn markers_are_read_only_while_shown_and_not_while_paused() {
    pinned(|| {
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx, Kind::Deploy);
        {
            let _run = crate::network::test_pause::set(None);
            assert_eq!(frame(&mut tab, &ctx, false), 0, "hidden: nothing is asked");
            assert!(tab.deploy_markers.loaded.is_none());
        }
        {
            let _paused = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
            assert_eq!(
                frame(&mut tab, &ctx, true),
                0,
                "offline or metered: nothing is asked"
            );
            assert!(
                tab.deploy_markers.loaded.is_none(),
                "read once that is over"
            );
        }
        let _run = crate::network::test_pause::set(None);
        assert_eq!(
            frame(&mut tab, &ctx, true),
            1,
            "shown: read once, in the background"
        );
        assert_eq!(
            frame(&mut tab, &ctx, true),
            0,
            "not again while it is on its way"
        );
        let (_, jobs) = messages::record_jobs(|| tab.refresh_deploy_markers(&ctx));
        assert!(
            jobs.is_empty(),
            "a reload reads them again only once they are old"
        );
    })
}

#[test]
fn only_the_latest_answer_counts_and_a_failure_keeps_what_was_read_as_stale() {
    pinned(|| {
        let _run = crate::network::test_pause::set(None);
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx, Kind::Deploy);
        frame(&mut tab, &ctx, true);
        let request = required(tab.deploy_markers.pending).0;
        tab.deploy_markers_arrived(request + 1, Ok(read()));
        assert!(
            tab.deploy_markers.markers.is_empty(),
            "an older request's answer is dropped"
        );
        tab.deploy_markers_arrived(request, Ok(read()));
        let state = &tab.deploy_markers;
        assert_eq!(state.markers.len(), 5);
        assert!(state.pending.is_none() && state.error.is_none());
        assert!(!state.read_stale(crate::git::now()));
        // The next read fails: the markers stay, drawn as stale, saying why.
        tab.deploy_markers.loaded = None;
        frame(&mut tab, &ctx, true);
        let request = required(tab.deploy_markers.pending).0;
        tab.deploy_markers_arrived(request, Err("gh: not logged in".into()));
        let state = &tab.deploy_markers;
        assert_eq!(state.markers.len(), 5);
        assert!(state.read_stale(crate::git::now()));
        assert!(state.read_stale(i64::MAX), "an old read is stale too");
    })
}

#[test]
fn another_forge_drops_the_markers_and_reads_them_again() {
    pinned(|| {
        let _run = crate::network::test_pause::set(None);
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx, Kind::Deploy);
        frame(&mut tab, &ctx, true);
        let request = required(tab.deploy_markers.pending).0;
        tab.deploy_markers_arrived(request, Ok(read()));
        messages::record_jobs(|| tab.set_primary_forge(Some("fork".into()), &ctx));
        assert_eq!(frame(&mut tab, &ctx, true), 1);
        assert!(
            tab.deploy_markers.markers.is_empty(),
            "the old project's markers go"
        );
        // The old project's answer, should it still come, isn't taken.
        tab.deploy_markers_arrived(request, Ok(read()));
        assert!(tab.deploy_markers.markers.is_empty());
    })
}

#[test]
fn a_forge_without_deployments_asks_nothing_and_says_why() {
    pinned(|| {
        let _run = crate::network::test_pause::set(None);
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx, Kind::Deploy);
        tab.hosting = None;
        assert_eq!(frame(&mut tab, &ctx, true), 0);
        assert!(tab.deploy_markers.error.is_some());
        assert_eq!(
            frame(&mut tab, &ctx, true),
            0,
            "not asked again every frame"
        );
    })
}

#[test]
fn the_palette_toggles_the_setting() {
    pinned(|| {
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx, Kind::Deploy);
        let items = tab.additional_palette_items(&ctx);
        let toggle = items
            .iter()
            .find(|(_, _, a)| {
                matches!(a, Action::Insights(InsightsAct::DeployMarkers(Act::Toggle)))
            })
            .map(|(label, _, _)| label.clone());
        assert_eq!(
            toggle.as_deref(),
            Some(crate::i18n::text("graph-deployments-palette"))
        );
        assert!(
            !crate::settings::Settings::get(&ctx).show_deploy_markers,
            "off by default"
        );
        tab.insights_act(InsightsAct::DeployMarkers(Act::Toggle), &ctx);
        assert!(crate::settings::Settings::get(&ctx).show_deploy_markers);
        tab.insights_act(InsightsAct::DeployMarkers(Act::Toggle), &ctx);
        assert!(!crate::settings::Settings::get(&ctx).show_deploy_markers);
    })
}

fn git_repo(name: &str) -> Result<(std::path::PathBuf, Vec<String>), String> {
    let path = std::env::temp_dir().join(format!("gitcraft-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| crate::git::run(Some(&path), args);
    git(&["init", "-q"])?;
    git(&["config", "user.name", "Deploy test"])?;
    git(&["config", "user.email", "deploy@example.invalid"])?;
    git(&["config", "commit.gpgSign", "false"])?;
    let mut commits = Vec::new();
    for message in ["fix", "release"] {
        git(&["commit", "--allow-empty", "-qm", message])?;
        commits.push(git(&["rev-parse", "HEAD"])?.trim().to_owned());
    }
    Ok((path, commits))
}

fn ask(tab: &mut RepoTab, ctx: &egui::Context, commit: &str) -> String {
    let act = Act::Reached {
        commit: commit.to_owned(),
        environment: "production".into(),
    };
    let (_, jobs) = messages::record_jobs(|| tab.deploy_markers_act(act, ctx));
    assert_eq!(jobs.len(), 1, "asked in the background");
    for job in jobs {
        tab.handle(job(), ctx);
    }
    tab.status
        .as_ref()
        .map(|n| n.raw().0.clone())
        .unwrap_or_default()
}

#[test]
fn reached_is_answered_by_the_repository_history() -> Result<(), String> {
    pinned(|| {
        let (path, commits) = git_repo("deploy-reached")?;
        let (fix, release) = (&commits[0], &commits[1]);
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(path.clone(), &ctx);
        let mut production = read().markers.remove(0);
        production.deployment.sha = release.clone();
        tab.deploy_markers.markers = vec![production];
        let yes = ask(&mut tab, &ctx, fix);
        assert!(yes.starts_with("Yes"), "{yes}");
        tab.deploy_markers.markers[0].deployment.sha = fix.clone();
        let no = ask(&mut tab, &ctx, release);
        assert!(no.starts_with("No"), "{no}");
        let unknown = ask(&mut tab, &ctx, &"e".repeat(40));
        assert!(unknown.starts_with("Can't tell"), "{unknown}");
        let _ = std::fs::remove_dir_all(&path);
        Ok(())
    })
}
