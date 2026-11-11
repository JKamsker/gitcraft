use super::*;
#[test]
fn stale_sparse_definition_refuses_each_changed_input() {
    let before = SparseState {
        head: "old-head".into(),
        enabled: true,
        cone: true,
        patterns: "/*\n!/*/\n/src/\n".into(),
        selected: BTreeSet::new(),
        folders: Vec::new(),
        partial: true,
    };
    assert!(same_sparse_definition(&before, &before));
    for field in 0..4 {
        let mut changed = before.clone();
        match field {
            0 => changed.head = "new-head".into(),
            1 => changed.patterns.push_str("/other/\n"),
            2 => changed.enabled = false,
            _ => changed.cone = false,
        }
        assert!(!same_sparse_definition(&before, &changed));
    }
}
#[test]
fn clone_plan_keeps_paths_separate_and_honors_all_options() -> Result<(), String> {
    let options = CloneOptions {
        depth: Some(12),
        blobless: true,
        folders: Some(vec!["src/space dir".into()]),
    };
    assert_eq!(
        options.args("https://example.invalid/r", "target path")?,
        [
            "clone",
            "--no-local",
            "--depth=12",
            "--filter=blob:none",
            "--sparse",
            "--",
            "https://example.invalid/r",
            "target path"
        ]
    );
    assert!(
        CloneOptions {
            depth: Some(0),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    Ok(())
}
#[test]
fn cone_paths_reject_traversal_controls_and_config_directories() -> Result<(), String> {
    for path in [
        "../x", "/etc", "a/.git/x", "a\\x", "C:/x", "a\nx", "", "a//b",
    ] {
        assert!(validate_folders(&[path.into()]).is_err(), "{path:?}");
    }
    assert!(validate_folders(&["-literal/日本語".into()]).is_ok());
    assert_eq!(
        sparse_args(Some(&[]))?,
        ["sparse-checkout", "set", "--cone", "--skip-checks", "--"]
    );
    Ok(())
}
#[test]
fn consent_is_explicit_scoped_and_restored_on_unwind() {
    assert!(passive_object_read(&[
        "-c",
        "color.ui=false",
        "show",
        "HEAD:a"
    ]));
    assert!(!passive_object_read(&["fetch", "origin"]));
    assert!(!passive_object_read(&["ls-remote", "origin"]));
    assert!(!passive_object_read(&["--version"]));
    download_scope(|| {
        assert!(!passive_object_read(&["cat-file", "blob", "abc"]));
    });
    assert!(!downloads_allowed());
    let _ = std::panic::catch_unwind(|| download_scope(|| std::panic::resume_unwind(Box::new(()))));
    assert!(!downloads_allowed());
}
