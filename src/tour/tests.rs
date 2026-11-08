use super::*;
use std::time::{SystemTime, UNIX_EPOCH};
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
fn repository_tour_reports_workflow_documents_and_activity() -> Result<(), String> {
    let repo = crate::git::doctor::tests::repo("tour-complete", &[])?;
    std::fs::create_dir_all(repo.join(".github")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(repo.join("src")).map_err(|e| e.to_string())?;
    for (path, text) in [
        ("README.md", "First-day introduction"),
        ("CONTRIBUTING.md", "Sign off contributions"),
        (".github/pull_request_template.md", "Test results"),
        (".editorconfig", "root=true"),
        (".gitattributes", "*.bin filter=lfs"),
        ("src/code.rs", "// hello"),
    ] {
        std::fs::write(repo.join(path), text).map_err(|e| e.to_string())?;
    }
    let run = |args: &[&str]| crate::git::doctor::tests::git(&repo, args);
    run(&["add", "."])?;
    run(&[
        "-c",
        "user.name=Tour Author",
        "-c",
        "user.email=tour@test.invalid",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-m",
        "feat: first day\n\nSigned-off-by: Tour Author <tour@test.invalid>",
    ])?;
    run(&["branch", "release/test"])?;
    run(&["tag", "v1.0"])?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64
        / 86400;
    let report = read(&repo, None, &|| true, &AtomicUsize::new(0), now)?;
    assert!(
        report
            .documents
            .iter()
            .any(|d| d.path == "README.md" && d.text.contains("First-day"))
    );
    assert!(
        report
            .documents
            .iter()
            .any(|d| d.path.contains("pull_request_template"))
    );
    assert!(report.needs.contains("Git LFS"));
    assert!(report.needs.contains("EditorConfig"));
    assert_eq!(report.workflow.conventional, 1);
    assert_eq!(report.workflow.signoffs, 1);
    assert!(report.branches.iter().any(|b| b.ends_with("release/test")));
    assert!(report.tags.contains(&"v1.0".into()));
    assert!(
        report
            .folders
            .iter()
            .any(|f| f.path == b"src" && f.authors[0].0 == "Tour Author"),
        "{:?}",
        report.folders
    );
    assert!(read(&repo, None, &|| false, &AtomicUsize::new(0), now).is_err());
    std::fs::remove_dir_all(repo).map_err(|e| e.to_string())?;
    Ok(())
}
