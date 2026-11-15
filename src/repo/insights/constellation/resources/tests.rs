//! Deterministic presentation-state tests; no repaint waits or remote connections.
use super::*;
use crate::git::{fixture::Repo, worktree_resources::collect};
use crate::repo::insights::{
    Kind,
    test_support::{required, tab},
};

#[test]
fn remote_measure_and_preview_reject_without_scheduling_any_worker() {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx, Kind::Constellation);
    tab.path = "ssh://unavailable.fixture.invalid/repo".into();
    for preview in [false, true] {
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.constellation_resources(
                &ctx,
                "ssh://unavailable.fixture.invalid/tree".into(),
                preview,
            );
        });
        assert!(jobs.is_empty());
        let state = &required(tab.insights.as_ref()).constellation.resources;
        assert!(!state.result.loading && !state.preview);
        assert_eq!(
            state.result.error(),
            Some(crate::i18n::text("resources-remote"))
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn presenting_preview_revalidates_ignored_head_status_and_presence() -> Result<(), String> {
    for change in ["ignored", "head", "status", "presence"] {
        let repo = Repo::new(&format!("resource-revalidate-{change}"))?;
        repo.commit(
            "base",
            &[("a", Some("base")), (".gitignore", Some(".env\n"))],
        )?;
        std::fs::write(repo.path().join(".env"), "before").map_err(|e| e.to_string())?;
        let before = collect(repo.path(), repo.path(), 1, None, None)?;
        let mut state = State::default();
        let id = state.result.start();
        state.result.accept(id, Ok(before));
        let id = state.validation.start();
        assert_eq!(
            state.verified(id, collect(repo.path(), repo.path(), 2, None, None)),
            Some(true)
        );
        assert!(state.preview);
        match change {
            "ignored" => std::fs::write(repo.path().join(".env"), "changed length")
                .map_err(|e| e.to_string())?,
            "head" => {
                repo.commit("moved", &[("a", Some("moved"))])?;
            }
            "status" => {
                std::fs::write(repo.path().join("a"), "dirty").map_err(|e| e.to_string())?
            }
            _ => {}
        }
        let presence = (change == "presence").then(|| "terminal now open".into());
        let id = state.validation.start();
        assert_eq!(
            state.verified(id, collect(repo.path(), repo.path(), 3, presence, None)),
            Some(false)
        );
        assert!(!state.preview);
        assert!(state.result.ready().is_none());
        assert_eq!(
            state.result.error(),
            Some(crate::i18n::text("resources-stale"))
        );
        // An old completion cannot resurrect it after stop/reload.
        state.stop();
        assert_eq!(
            state.verified(id, collect(repo.path(), repo.path(), 4, None, None)),
            None
        );
    }
    Ok(())
}

#[test]
fn reload_retires_disk_observations_and_distinguishes_unmeasured_worktrees() {
    use crate::insights::Evidence;
    use crate::insights::constellation::{Flag, Kind as NodeKind, Node};
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx, Kind::Constellation);
    let state = &mut required(tab.insights.as_mut()).constellation;
    let mut sizes = std::collections::HashMap::new();
    sizes.insert(
        "/first".into(),
        Ok(DiskObservation {
            size: crate::insights::constellation::read::Disk::default(),
            head: "head".into(),
            fingerprint: "evidence".into(),
            observed: crate::git::now(),
        }),
    );
    let id = state.disk.start();
    state.disk.accept(id, Ok(sizes));
    let mut other = Node {
        kind: NodeKind::Worktree,
        key: "/second".into(),
        label: "second".into(),
        flags: Vec::new(),
        detail: Vec::new(),
        commit: None,
    };
    let line = required(super::super::picture::disk_line(
        &other,
        state.disk.ready(),
        false,
    ));
    assert!(line.contains("not measured") && !line.contains("missing"));
    other.flags.push((Flag::Missing, Evidence::Observed));
    assert!(
        required(super::super::picture::disk_line(
            &other,
            state.disk.ready(),
            false
        ))
        .contains("Git reports missing")
    );
    tab.invalidate_resource_observations();
    let state = &required(tab.insights.as_ref()).constellation;
    assert!(state.disk.ready().is_none());
    assert!(state.forge_read.is_none());
}
