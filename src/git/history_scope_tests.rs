use super::*;

#[test]
fn namespace_roots_are_reversible_counted_and_consistent_across_readers() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-history-scope-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| super::super::run(Some(&dir), args);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Person"],
        &["config", "user.email", "human@example.com"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        git(args)?;
    }
    let base = git(&["rev-parse", "HEAD"])?.trim().to_owned();
    git(&["switch", "-q", "--orphan", "checkpoint-temp"])?;
    let message = format!(
        "t3 checkpoint ref=refs/t3/test\n\nMentions {base}\n\nCo-authored-by: Claude <noreply@anthropic.com>"
    );
    git(&[
        "commit",
        "-q",
        "--allow-empty",
        "--author=T3 Code <t3code@users.noreply.github.com>",
        "-m",
        &message,
    ])?;
    let checkpoint = git(&["rev-parse", "HEAD"])?.trim().to_owned();
    git(&["update-ref", "refs/t3/test", &checkpoint])?;
    git(&["switch", "-q", "main"])?;
    git(&["branch", "-D", "checkpoint-temp"])?;
    let refs_before = git(&["for-each-ref", "--format=%(refname) %(objectname)"])?;
    let excluded = default_hidden_namespaces();
    let hidden = super::super::log_scoped(&dir, None, Default::default(), 0, 10, &excluded)?;
    assert_eq!(
        hidden.iter().map(|c| c.hash.as_str()).collect::<Vec<_>>(),
        [&base]
    );
    assert_eq!(excluded_namespace_count(&dir, None, &excluded)?, 1);
    let mut stream = super::super::LogStream::start_scoped(&dir, None, Default::default(), 0, &[])?;
    let mut all = stream.next_page(1)?;
    all.extend(stream.next_page(1)?);
    assert_eq!(all.len(), 2);
    assert!(stream.next_page(1)?.is_empty());
    assert!(all.iter().any(|c| c.hash == checkpoint));
    assert!(!crate::authorship::load::all_scoped(&dir, &excluded)?.contains_key(&checkpoint));
    assert!(crate::authorship::load::all_scoped(&dir, &[])?.contains_key(&checkpoint));
    assert!(
        super::super::backlinks_scoped(&dir, &base, &excluded)?
            .0
            .is_empty()
    );
    assert_eq!(super::super::backlinks_scoped(&dir, &base, &[])?.0.len(), 1);
    let roots = search_roots(&dir, None, &excluded)?;
    assert!(
        super::super::search(&dir, Some(&roots), Default::default(), "t3 checkpoint")?.is_empty()
    );
    assert_eq!(
        refs_before,
        git(&["for-each-ref", "--format=%(refname) %(objectname)"])?
    );
    assert_eq!(git(&["symbolic-ref", "HEAD"])?.trim(), "refs/heads/main");
    assert!(git(&["status", "--porcelain"])?.is_empty());
    // A real branch sharing the excluded root must keep that tip and its ancestry.
    git(&["branch", "real-work", &checkpoint])?;
    assert_eq!(excluded_namespace_count(&dir, None, &excluded)?, 0);
    assert_eq!(
        super::super::log_scoped(&dir, None, Default::default(), 0, 10, &excluded)?.len(),
        2
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn namespace_input_cannot_become_revision_options_or_expressions() {
    assert_eq!(
        namespace_patterns("refs/t3/*\n--all\nrefs/test:bad\nrefs/other/*\nHEAD~1\nrefs/bad name"),
        ["refs/t3/*", "refs/other/*"]
    );
}
