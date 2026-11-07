use super::*;
#[test]
fn workflow_observations_do_not_verify_signatures_or_assert_a_policy() -> Result<(), String> {
    let object = "tree aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nparent bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nparent cccccccccccccccccccccccccccccccccccccccc\ngpgsig fixture-only-never-verified\n more fixture bytes\n\nfeat(ui): a contribution\n\nSigned-off-by: A <a@test.invalid>\n";
    let bytes = format!("{} commit {}\n{object}\n", "d".repeat(40), object.len());
    let mut w = Workflow::default();
    inspect_objects(bytes.as_bytes(), 1, &mut w)?;
    assert_eq!(
        (
            w.inspected,
            w.merges,
            w.conventional,
            w.signature_headers,
            w.signoffs
        ),
        (1, 1, 1, 1, 1)
    );
    assert!(inspect_objects(b"bad", 1, &mut w).is_err());
    Ok(())
}
#[test]
fn real_repository_tour_reads_documents_needs_and_one_cached_history_without_hooks()
-> Result<(), String> {
    let repo = crate::practice::create(crate::practice::Scenario::Published)?;
    std::fs::create_dir_all(repo.join(".github")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(repo.join(".husky")).map_err(|e| e.to_string())?;
    for (path, text) in [
        ("README.md", "Read this project first."),
        ("CONTRIBUTING.md", "Review changes and sign off."),
        (".github/pull_request_template.md", "Describe validation."),
        (".gitattributes", "*.bin filter=lfs\n"),
        (".editorconfig", "root=true\n"),
        (
            ".gitmodules",
            "[submodule \"example\"]\n path=example\n url=https://example.invalid/project.git\n",
        ),
        (".husky/pre-commit", "echo NEVER-RUN\n"),
    ] {
        std::fs::write(repo.join(path), text).map_err(|e| e.to_string())?;
    }
    git::practice_run(&repo, &["add", "."])?;
    git::practice_run(
        &repo,
        &[
            "commit",
            "-m",
            "docs: first day guide\n\nSigned-off-by: Gitcraft Practice <practice@example.invalid>",
        ],
    )?;
    let h = Arc::new(git::activity::read(
        &repo,
        100,
        &|| true,
        &AtomicUsize::new(0),
    )?);
    let now = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs()
        / 86400) as i64;
    let result = read(&repo, Some(h.clone()), &|| true, &AtomicUsize::new(0), now)?;
    assert!(Arc::ptr_eq(&h, &result.history));
    assert!(result.needs.contains("Git LFS"));
    assert!(result.needs.contains("EditorConfig"));
    assert!(result.needs.contains("Submodules"));
    assert!(
        result
            .documents
            .iter()
            .any(|d| d.path == "README.md" && d.text.contains("Read this"))
    );
    assert!(result.workflow.signoffs > 0);
    assert_eq!(result.workflow.inspected, h.commits.len());
    assert!(!result.folders.is_empty());
    assert!(read(&repo, None, &|| false, &AtomicUsize::new(0), now).is_err());
    let _ = std::fs::remove_dir_all(repo.parent().ok_or("root")?);
    Ok(())
}
