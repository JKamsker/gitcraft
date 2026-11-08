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

#[test]
fn fluent_tour_cached_raw_statuses_follow_language_at_paint_time() {
    let raw = "Invalid repository HEAD".to_owned();
    let incomplete = "Some commit metadata was unavailable locally or exceeded the reading limit; it was not downloaded";
    let omitted = "Additional documents omitted by the 20-document reading limit";
    let path = "docs/ü { $not-a-variable }\nREADME.md";
    let unavailable =
        format!("{path}: content unavailable locally or above the reading limit (not downloaded)");
    let day = Day(19782); // 2024-02-29
    for (locale, error, date, document) in [
        (
            "en-US",
            "Invalid repository HEAD",
            "02/29/2024 UTC",
            "content unavailable locally or above the reading limit (not downloaded)",
        ),
        (
            "de",
            "Ungültiger Repository-HEAD",
            "29.02.2024 UTC",
            "Inhalt lokal nicht verfügbar oder über dem Leselimit (nicht heruntergeladen)",
        ),
        (
            "en-US",
            "Invalid repository HEAD",
            "02/29/2024 UTC",
            "content unavailable locally or above the reading limit (not downloaded)",
        ),
    ] {
        crate::i18n::set_language(locale);
        assert_eq!(presentation::diagnostic(&raw), error);
        assert_eq!(
            presentation::document_unavailable(&unavailable),
            format!("{path}: {document}")
        );
        assert_eq!(day.display(), date);
        assert_eq!(day.to_string(), "2024-02-29 UTC");
        assert!(presentation::document_unavailable(omitted).contains("20"));
        assert!(
            presentation::diagnostic(incomplete).contains(if locale == "de" {
                "nicht heruntergeladen"
            } else {
                "not downloaded"
            })
        );
        assert_eq!(
            presentation::diagnostic("provider: Invalid repository HEAD"),
            "provider: Invalid repository HEAD"
        );
        assert_eq!(presentation::diagnostic(path), path);
        assert_eq!(presentation::document_unavailable(path), path);
        assert_eq!(presentation::need("Git LFS"), "Git LFS");
        assert_eq!(presentation::need("EditorConfig"), "EditorConfig");
        assert_eq!(presentation::need(path), path);
        assert_eq!(
            presentation::need("Submodules"),
            if locale == "de" {
                "Submodule"
            } else {
                "Submodules"
            }
        );
        assert!(
            presentation::need("Husky hooks (review project instructions)").contains(
                if locale == "de" {
                    "Projektanweisungen"
                } else {
                    "project instructions"
                }
            )
        );
    }
    assert_eq!(raw, "Invalid repository HEAD");
    crate::i18n::set_language("en-US");
}

#[test]
fn fluent_tour_root_label_keeps_real_directory_names_opaque() -> Result<(), String> {
    let snapshot = |paths| History {
        commits: vec![ObservedCommit {
            hash: "a".repeat(40),
            name: "Name".into(),
            email: "a@test.invalid".into(),
            committer: Day(19782),
            paths,
        }],
        more: false,
        shallow: false,
        bytes_limited: false,
    };
    let root = snapshot(vec![b"README.md".to_vec()]);
    let real = snapshot(vec![b"(root files)/code.rs".to_vec()]);
    let mixed = snapshot(vec![
        b"README.md".to_vec(),
        b"(root files)/code.rs".to_vec(),
    ]);
    let (folders, _, _) = recent(&root, Day(19700), Day(19800), &|| true)?;
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        assert_eq!(folders[0].path, b"(root files)");
        assert_eq!(
            folders[0].display_path(),
            if locale == "de" {
                "(Dateien im Stammverzeichnis)"
            } else {
                "(root files)"
            }
        );
        for history in [&real, &mixed] {
            let (folders, _, _) = recent(history, Day(19700), Day(19800), &|| true)?;
            assert_eq!(folders[0].display_path(), "(root files)");
            assert_eq!(folders[0].authors[0].2, 1); // Preserve original grouping/deduplication.
        }
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
