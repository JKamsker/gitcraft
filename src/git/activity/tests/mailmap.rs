//! Mailmap.

use super::*;

pub(super) fn chain(r: &Repo, n: usize) -> Result<(), String> {
    let mut parent = r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    let tree = r.git(&["rev-parse", "HEAD^{tree}"])?;
    for i in 1..n {
        parent = r.git(&["commit-tree", &tree, "-p", &parent, "-m", &i.to_string()])?;
    }
    let branch = r.git(&["symbolic-ref", "HEAD"])?;
    r.git(&["update-ref", &branch, &parent])?;
    Ok(())
}

#[test]
pub(super) fn a_mailmap_edit_between_real_git_pages_cannot_split_an_author() -> Result<(), String> {
    let r = Repo::new()?;
    chain(&r, 270)?;
    let first = "First <first@test.invalid> Alias <old@test.invalid>\n";
    let second = "Second <second@test.invalid> Alias <old@test.invalid>\n";
    std::fs::write(r.0.join(".mailmap"), first).map_err(|e| e.to_string())?;
    let before = summarize(&r.read(1000)?, Query::default(), &|| true)?;
    assert_eq!(before.contributors.len(), 1);
    assert_eq!(before.contributors[0].name, "First");
    let mut edited = false;
    let history = read_observing(&r.0, 1000, &|| true, &AtomicUsize::new(0), |count| {
        if count == PAGE && !edited {
            assert!(std::fs::write(r.0.join(".mailmap"), second).is_ok());
            edited = true;
        }
    })?;
    assert!(edited);
    assert_eq!(history.commits.len(), 270);
    let result = summarize(&history, Query::default(), &|| true)?;
    assert_eq!(result.contributors.len(), 1);
    assert_eq!(result.contributors[0].name, "Second");
    assert_eq!(result.contributors[0].commits, 270);
    assert!(
        history
            .commits
            .iter()
            .all(|c| c.email == "second@test.invalid")
    );
    // The same one-pass contract covers configured files and changing blob revisions.
    let extra = r.0.join("extra.map");
    std::fs::write(&extra, first).map_err(|e| e.to_string())?;
    r.git(&["config", "mailmap.file", "extra.map"])?;
    let history = read_observing(&r.0, 1000, &|| true, &AtomicUsize::new(0), |count| {
        if count == PAGE {
            assert!(std::fs::write(&extra, second).is_ok());
        }
    })?;
    assert!(
        history
            .commits
            .iter()
            .all(|c| c.email == "second@test.invalid")
    );
    r.git(&["config", "mailmap.file", ""])?;
    std::fs::write(&extra, first).map_err(|e| e.to_string())?;
    let first_blob = r.git(&["hash-object", "-w", "--", "extra.map"])?;
    std::fs::write(&extra, second).map_err(|e| e.to_string())?;
    let second_blob = r.git(&["hash-object", "-w", "--", "extra.map"])?;
    r.git(&["config", "mailmap.blob", &first_blob])?;
    let history = read_observing(&r.0, 1000, &|| true, &AtomicUsize::new(0), |count| {
        if count == PAGE {
            assert!(r.git(&["config", "mailmap.blob", &second_blob]).is_ok());
        }
    })?;
    assert!(
        history
            .commits
            .iter()
            .all(|c| c.email == "second@test.invalid")
    );
    Ok(())
}

#[test]
pub(super) fn mailmap_blob_and_configured_file_precedence_match_git() -> Result<(), String> {
    let r = Repo::new()?;
    std::fs::write(
        r.0.join(".mailmap"),
        "Blob <blob@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    r.git(&["config", "mailmap.blob", "HEAD:.mailmap"])?;
    std::fs::write(
        r.0.join(".mailmap"),
        "Dirty <dirty@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    let check = |repo: &Repo| -> Result<(), String> {
        let expected = repo.git(&["--no-lazy-fetch", "log", "-1", "--format=%aN <%aE>"])?;
        let h = repo.read(100)?;
        let c = &h.commits[0];
        assert_eq!(format!("{} <{}>", c.name, c.email), expected);
        Ok(())
    };
    check(&r)?;
    assert_eq!(r.read(100)?.commits[0].name, "Blob");
    std::fs::write(
        r.0.join("extra map"),
        "Configured <configured@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    r.git(&["config", "mailmap.file", "extra map"])?;
    check(&r)?;
    assert_eq!(r.read(100)?.commits[0].name, "Configured");
    // Relative augmenting files are relative to the worktree root, even from a subfolder.
    std::fs::create_dir(r.0.join("nested")).map_err(|e| e.to_string())?;
    let h = read(&r.0.join("nested"), 100, &|| true, &AtomicUsize::new(0))?;
    assert_eq!(h.commits[0].name, "Configured");
    r.git(&["config", "mailmap.file", ""])?;
    r.git(&["config", "mailmap.blob", ""])?;
    check(&r)?;
    assert_eq!(r.read(100)?.commits[0].name, "Dirty");
    Ok(())
}

#[test]
pub(super) fn bare_mailmap_defaults_and_absent_optional_maps_match_git() -> Result<(), String> {
    let source = Repo::new()?;
    source.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "unmapped")?;
    let bare = Repo(source.0.with_extension("bare"));
    let out = Command::new("git")
        .args(["clone", "--bare"])
        .arg(&source.0)
        .arg(&bare.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(out.status.success());
    assert_eq!(bare.read(100)?.commits[0].name, "Alias");
    std::fs::write(
        source.0.join(".mailmap"),
        "Canonical <canonical@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    source.commit("2026-01-02T00:00:00Z", "2026-01-02T00:00:00Z", "mapped")?;
    bare.git(&["fetch", "-q", "origin", "+HEAD:refs/heads/mapped"])?;
    bare.git(&["symbolic-ref", "HEAD", "refs/heads/mapped"])?;
    let h = bare.read(100)?;
    assert!(h.commits.iter().all(|c| c.name == "Canonical"));
    let expected = bare.git(&["--no-lazy-fetch", "log", "-1", "--format=%aN <%aE>"])?;
    assert_eq!(
        format!("{} <{}>", h.commits[0].name, h.commits[0].email),
        expected
    );
    Ok(())
}

#[test]
pub(super) fn unavailable_promised_mailmap_blob_is_an_error_without_downloading()
-> Result<(), String> {
    let source = Repo::new()?;
    std::fs::write(
        source.0.join(".mailmap"),
        "Canonical <canonical@test.invalid> Alias <old@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    source.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "mapped")?;
    source.git(&["config", "mailmap.blob", "HEAD:.mailmap"])?;
    assert_eq!(source.read(100)?.commits[0].name, "Canonical");
    source.git(&["config", "uploadpack.allowFilter", "true"])?;
    let partial = Repo(source.0.join("partial"));
    let out = Command::new("git")
        .args(["clone", "--no-checkout", "--no-local", "--filter=blob:none"])
        .arg(&source.0)
        .arg(&partial.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(out.status.success());
    partial.git(&["config", "mailmap.blob", "HEAD:.mailmap"])?;
    let missing = partial.git(&[
        "--no-lazy-fetch",
        "rev-list",
        "--objects",
        "--all",
        "--missing=print",
    ])?;
    assert!(missing.lines().any(|s| s.starts_with('?')));
    assert!(
        partial
            .read(100)
            .is_err_and(|e| e.contains("Contributor mailmap unavailable"))
    );
    assert_eq!(
        partial.git(&[
            "--no-lazy-fetch",
            "rev-list",
            "--objects",
            "--all",
            "--missing=print"
        ])?,
        missing
    );
    let bare = Repo(source.0.join("partial-bare"));
    let out = Command::new("git")
        .args(["clone", "--bare", "--no-local", "--filter=blob:none"])
        .arg(&source.0)
        .arg(&bare.0)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(out.status.success());
    let missing = bare.git(&[
        "--no-lazy-fetch",
        "rev-list",
        "--objects",
        "--all",
        "--missing=print",
    ])?;
    assert!(
        bare.read(100)
            .is_err_and(|e| e.contains("Contributor mailmap unavailable"))
    );
    assert_eq!(
        bare.git(&[
            "--no-lazy-fetch",
            "rev-list",
            "--objects",
            "--all",
            "--missing=print"
        ])?,
        missing
    );
    Ok(())
}

#[test]
pub(super) fn configured_missing_mailmap_file_and_successful_git_errors_are_reported()
-> Result<(), String> {
    let r = Repo::new()?;
    r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    r.git(&["config", "mailmap.file", "missing-map"])?;
    assert!(
        r.read(100)
            .is_err_and(|e| e.contains("Contributor mailmap unavailable"))
    );
    #[cfg(unix)]
    {
        let result = process::run(
            &r.0,
            &[
                "-c",
                "alias.activitydiagnostic=!printf 'error: unavailable mailmap\\n' >&2; exit 0",
                "activitydiagnostic",
            ],
            None,
            &|| true,
        );
        assert!(result.is_err_and(|e| e.contains("unavailable mailmap")));
    }
    Ok(())
}
