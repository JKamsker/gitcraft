use super::*;
use crate::git::testlane::fixture::Repo;

fn shell_marker(repo: &Repo, name: &str) -> String {
    let path = repo.path().join(name).to_string_lossy().replace('\\', "/");
    crate::remote::sh_quote(path.strip_prefix("//?/").unwrap_or(&path))
}
fn refuses(repo: &Repo, key: &'static str) -> Result<(), String> {
    let Err(error) = test_staged(repo.path(), "exit 0", &RunStop::default()) else {
        return Err(format!("expected refusal: {key}"));
    };
    assert_eq!(error, crate::i18n::text(key));
    Ok(())
}

#[cfg(unix)]
fn hook(repo: &Repo, name: &str, marker: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let path = repo.path().join(".git/hooks").join(name);
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\necho unsafe > '{}'\n",
            repo.path().join(marker).display()
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())
}
#[cfg(unix)]
#[test]
fn private_index_writes_and_cleanup_never_run_source_hooks() -> Result<(), String> {
    let repo = Repo::new("staged-review-hooks")?;
    repo.commit("base", &[])?;
    std::fs::write(repo.path().join("staged"), "new").map_err(|e| e.to_string())?;
    repo.git(&["add", "staged"])?;
    hook(&repo, "post-index-change", "index-hook")?;
    hook(&repo, "post-checkout", "checkout-hook")?;
    hook(&repo, "fsmonitor-probe", "fsmonitor-ran")?;
    repo.git(&["config", "core.fsmonitor", ".git/hooks/fsmonitor-probe"])?;
    let index = index_bytes(repo.path())?;
    let refs = repo.git(&["show-ref"])?;
    let result = test_staged(repo.path(), "exit 0", &RunStop::default())?;
    assert_eq!(result.ended.code, Some(0));
    assert_eq!(index_bytes(repo.path())?, index);
    assert_eq!(repo.git(&["show-ref"])?, refs);
    for name in ["index-hook", "checkout-hook", "fsmonitor-ran"] {
        assert!(!repo.path().join(name).exists());
    }
    Ok(())
}
#[test]
fn sparse_index_and_standalone_skip_worktree_are_explicitly_refused() -> Result<(), String> {
    let repo = Repo::new("staged-review-sparse")?;
    repo.commit("base", &[("keep/a", Some("a")), ("omit/b", Some("b"))])?;
    repo.git(&["sparse-checkout", "init", "--cone", "--sparse-index"])?;
    repo.git(&["sparse-checkout", "set", "keep"])?;
    refuses(&repo, "staged-sparse")?;
    repo.git(&["sparse-checkout", "disable"])?;
    repo.git(&["update-index", "--skip-worktree", "omit/b"])?;
    refuses(&repo, "staged-sparse")?;
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}
#[test]
fn filters_lfs_and_info_attributes_are_refused_before_they_can_execute() -> Result<(), String> {
    let repo = Repo::new("staged-review-filters")?;
    repo.commit("base", &[("payload", Some("literal"))])?;
    let smudge = format!("echo unsafe > {}; cat", shell_marker(&repo, "filter-ran"));
    repo.git(&["config", "filter.probe.smudge", &smudge])?;
    repo.git(&[
        "config",
        "filter.probe.process",
        &format!(
            "echo unsafe > {}; exit 1",
            shell_marker(&repo, "process-ran")
        ),
    ])?;
    for attribute in [
        "filter=probe",
        "filter=lfs",
        "filter=unset",
        "filter=unspecified",
        "-filter",
    ] {
        std::fs::write(
            repo.path().join(".gitattributes"),
            format!("payload {attribute}\n"),
        )
        .map_err(|e| e.to_string())?;
        // Git add refreshes other entries too. Keep fixture staging inert while
        // retaining the dangerous configured drivers for the operation under test.
        repo.git(&[
            "-c",
            "filter.probe.process=",
            "-c",
            "filter.probe.smudge=",
            "add",
            ".gitattributes",
        ])?;
        refuses(&repo, "staged-filters")?;
        assert_eq!(repo.worktree_count()?, 1);
    }
    repo.git(&["reset", "--", ".gitattributes"])?;
    std::fs::remove_file(repo.path().join(".gitattributes")).map_err(|e| e.to_string())?;
    std::fs::write(
        repo.path().join(".git/info/attributes"),
        "payload filter=probe\n",
    )
    .map_err(|e| e.to_string())?;
    refuses(&repo, "staged-filters")?;
    for name in ["filter-ran", "process-ran"] {
        assert!(!repo.path().join(name).exists());
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn symlink_targets_and_chains_cannot_expose_live_helpers() -> Result<(), String> {
    use std::os::unix::fs::symlink;
    let repo = Repo::new("staged-review-links")?;
    repo.commit("base", &[("inside", Some("tracked"))])?;
    std::fs::write(repo.path().join("helper"), "untracked").map_err(|e| e.to_string())?;
    for target in [
        repo.path().join("helper").display().to_string(),
        "../../escape".into(),
        "b".into(),
        ".git/config".into(),
    ] {
        let a = repo.path().join("a");
        if a.symlink_metadata().is_ok() {
            std::fs::remove_file(&a).map_err(|e| e.to_string())?;
        }
        symlink(&target, &a).map_err(|e| e.to_string())?;
        if target == "b" {
            symlink("../../escape", repo.path().join("b")).map_err(|e| e.to_string())?;
            repo.git(&["add", "b"])?;
        }
        repo.git(&["add", "a"])?;
        refuses(&repo, "staged-links")?;
    }
    repo.git(&["rm", "-f", "a", "b"])?;
    symlink("inside", repo.path().join("safe")).map_err(|e| e.to_string())?;
    repo.git(&["add", "safe"])?;
    assert_eq!(
        test_staged(repo.path(), "test -f safe", &RunStop::default())?
            .ended
            .code,
        Some(0)
    );
    repo.git(&["config", "core.symlinks", "false"])?;
    refuses(&repo, "staged-paths")?;
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

#[test]
fn expired_overall_deadline_stops_preparation_and_cleanup_uses_its_own_budget() -> Result<(), String>
{
    let repo = Repo::new("staged-review-deadline")?;
    repo.commit("base", &[])?;
    crate::processes::deadline::within(Instant::now(), || {
        assert!(test_staged(repo.path(), "exit 0", &RunStop::default()).is_err());
    });
    let (scratch, _) = snapshot(repo.path(), None)?;
    crate::processes::deadline::within(Instant::now(), || scratch.remove())?;
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

#[test]
fn oversized_source_index_is_refused_without_reading_or_copying_it() -> Result<(), String> {
    let repo = Repo::new("staged-review-index-budget")?;
    repo.commit("base", &[])?;
    let index = std::fs::File::options()
        .write(true)
        .open(repo.path().join(".git/index"))
        .map_err(|e| e.to_string())?;
    index
        .set_len(64 * 1024 * 1024 + 1)
        .map_err(|e| e.to_string())?;
    drop(index);
    refuses(&repo, "staged-budget")?;
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

#[test]
fn too_many_staged_entries_are_refused_before_materialization() -> Result<(), String> {
    let repo = Repo::new("staged-review-count-budget")?;
    repo.commit("base", &[])?;
    let object = git::run_captured(repo.path(), &["hash-object", "-w", "--stdin"], Some(b"x"))?;
    assert_eq!(object.code, Some(0));
    let rows: String = (0..10_001)
        .map(|n| format!("100644 {}\tfile{n}\n", object.stdout.trim()))
        .collect();
    let staged = git::run_captured(
        repo.path(),
        &["update-index", "--index-info"],
        Some(rows.as_bytes()),
    )?;
    assert_eq!(staged.code, Some(0));
    refuses(&repo, "staged-budget")?;
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    Ok(())
}

#[test]
fn global_attributes_cannot_switch_from_unstaged_source_to_staged_filter_rules()
-> Result<(), String> {
    let repo = Repo::new("staged-review-global-attributes")?;
    repo.commit(
        "base",
        &[
            ("payload", Some("literal")),
            ("attrs", Some("payload filter=probe\n")),
        ],
    )?;
    repo.git(&["config", "core.attributesFile", "attrs"])?;
    repo.git(&[
        "config",
        "filter.probe.smudge",
        &format!("echo unsafe > {}; cat", shell_marker(&repo, "filter-ran")),
    ])?;
    std::fs::write(repo.path().join("attrs"), "").map_err(|e| e.to_string())?;
    let result = test_staged(
        repo.path(),
        "test $(cat payload) = literal",
        &RunStop::default(),
    )?;
    assert_eq!(result.ended.code, Some(0));
    assert!(!repo.path().join("filter-ran").exists());
    let before = staged_fingerprint(repo.path(), "exit 0")?;
    std::fs::write(
        repo.path().join(".git/info/attributes"),
        "payload text eol=lf\n",
    )
    .map_err(|e| e.to_string())?;
    assert_ne!(before, staged_fingerprint(repo.path(), "exit 0")?);
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}
