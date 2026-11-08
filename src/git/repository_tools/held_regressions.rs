//! Authored actual-repository regressions. Execution remains under the global
//! native/process hold; the pure gate does not select this module.
use super::*;
use std::path::PathBuf;

#[derive(serde::Serialize, serde::Deserialize)]
struct Fixture {
    selected: PathBuf,
    foreign: PathBuf,
    base: String,
    tip: String,
}
const CHILD: &str = "GITCRAFT_OWNED_INTERNALS_ENV_REGRESSION";
const TEST: &str =
    "git::repository_tools::held_regressions::inherited_redirects_cannot_change_selected_decisions";

#[test]
fn inherited_redirects_cannot_change_selected_decisions() -> Result<(), String> {
    if let Some(serialized) = std::env::var_os(CHILD) {
        let fixture: Fixture =
            serde_json::from_str(&serialized.to_string_lossy()).map_err(|e| e.to_string())?;
        let selected = &fixture.selected;
        assert!(remote_config::boolean(
            selected,
            "remote.vendor.mirror",
            false
        )?);
        assert!(publication::prepare(selected, publication::read(selected, "review")?).is_err());
        let branches = routes::read(selected, "main")?;
        let branch = branches
            .branches
            .iter()
            .find(|b| b.name == "main")
            .ok_or("Missing selected branch")?;
        assert_eq!(branch.pull.counts, Some((1, 0)));
        let refs = all_refs::read(selected, "")?;
        assert!(refs.refs.iter().any(|r| r.name == "refs/gitcraft/selected"));
        assert!(!refs.refs.iter().any(|r| r.name == "refs/gitcraft/foreign"));
        assert!(refs.history.contains("TARGET_TIP"));
        assert!(!refs.history.contains("DECOY_ROOT"));
        let control = crate::git::OpControl::new(|| {});
        let runner = guard::Runner {
            repo: selected,
            control: &control,
            credentials: None,
        };
        let history = replacements::read(selected)?;
        // The foreign graph replaces B by a root. Reading its graph would hide
        // the A -> B ancestry and permit the cyclic A-parent-B graft.
        assert!(replacements::graft(&runner, &history, &fixture.base, &fixture.tip).is_err());
        assert!(
            guard::text(
                selected,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("refs/replace/{}", fixture.base)
                ]
            )
            .is_err()
        );
        // The key exists only in selected. A raw foreign existence read would
        // skip this unset and leave the selected override behind.
        remote_config::set_values(&runner, "test.selectedOverride", &[])?;
        assert!(remote_config::values(selected, "test.selectedOverride")?.is_empty());
        assert_eq!(
            remote_config::values(&fixture.foreign, "test.marker")?,
            ["DECOY"]
        );
        return Ok(());
    }
    let selected = crate::git::fixture::Repo::new("selected-env-regression")?;
    let base = selected.commit("TARGET_BASE", &[])?;
    let tip = selected.commit("TARGET_TIP", &[])?;
    let foreign = crate::git::fixture::Repo::new("foreign-env-regression")?;
    foreign.git(&[
        "fetch",
        &selected.path().to_string_lossy(),
        "main:refs/heads/main",
    ])?;
    foreign.git(&["checkout", "main"])?;
    let tree = foreign.tree("main")?;
    let root = foreign.git(&["commit-tree", &tree, "-m", "DECOY_ROOT"])?;
    foreign.git(&["replace", &tip, root.trim()])?;
    foreign.git(&["update-ref", "refs/gitcraft/foreign", &tip])?;
    foreign.git(&["update-ref", "refs/remotes/vendor/main", &tip])?;
    foreign.git(&["config", "test.marker", "DECOY"])?;
    selected.git(&["update-ref", "refs/gitcraft/selected", &tip])?;
    selected.git(&["remote", "add", "vendor", &foreign.path().to_string_lossy()])?;
    selected.git(&["update-ref", "refs/remotes/vendor/main", &base])?;
    for (key, value) in [
        ("branch.main.remote", "vendor"),
        ("branch.main.merge", "refs/heads/main"),
        ("branch.main.pushRemote", "vendor"),
        ("remote.vendor.mirror", "true"),
        ("gitcraft-publication.review.remote", "vendor"),
        ("test.selectedOverride", "TARGET"),
    ] {
        selected.git(&["config", key, value])?;
    }
    let fixture = Fixture {
        selected: selected.path().into(),
        foreign: foreign.path().into(),
        base,
        tip,
    };
    // A child test owns this inherited environment; never mutate process-global
    // variables in the parallel application test harness.
    let output = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(
            CHILD,
            serde_json::to_string(&fixture).map_err(|e| e.to_string())?,
        )
        .env("GIT_DIR", foreign.path().join(".git"))
        .env("GIT_WORK_TREE", foreign.path())
        .env("GIT_COMMON_DIR", foreign.path().join(".git"))
        .env("GIT_INDEX_FILE", foreign.path().join(".git/index"))
        .env("GIT_OBJECT_DIRECTORY", foreign.path().join(".git/objects"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "remote.vendor.mirror")
        .env("GIT_CONFIG_VALUE_0", "false")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Owned foreign-environment child failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[test]
#[cfg(unix)]
fn graft_metadata_refuses_oversize_nonregular_and_changed_exact_witness() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("graft-metadata-bounds")?;
    let base = repo.commit("base", &[])?;
    let path = repo.path().join(".git/info/grafts");
    std::fs::write(&path, vec![b'#'; 64 * 1024 + 1]).map_err(|e| e.to_string())?;
    assert!(replacements::read(repo.path()).is_err());
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    let socket = std::os::unix::net::UnixListener::bind(&path).map_err(|e| e.to_string())?;
    // A nonregular owned endpoint must be refused before any Git graph read.
    // The production opener also uses O_NONBLOCK for FIFO-like endpoints.
    assert!(replacements::read(repo.path()).is_err());
    drop(socket);
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    let history = replacements::read(repo.path())?;
    std::fs::write(&path, b"").map_err(|e| e.to_string())?;
    let control = crate::git::OpControl::new(|| {});
    let runner = guard::Runner {
        repo: repo.path(),
        control: &control,
        credentials: None,
    };
    // Missing and empty are different exact witnesses, although their display
    // strings are equal. Refuse even a no-op selection before any Git mutation.
    let error = replacements::graft(&runner, &history, &base, &base)
        .err()
        .ok_or("Changed graft witness was accepted")?;
    assert!(error.contains("graft file changed"));
    Ok(())
}
