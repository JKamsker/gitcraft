use super::*;
#[test]
fn r2_pure_raw_export_uses_only_blob_tree_reads_and_does_not_apply_attributes() -> Result<(), String>
{
    struct Recorded(Vec<String>);
    impl Objects for Recorded {
        fn read(&mut self, _: &Path, args: &[&str], _: usize) -> Result<Vec<u8>, String> {
            self.0.push(args.join(" "));
            let oid = "a".repeat(40);
            match args[0] {
                "ls-tree" => Ok(format!("100644 blob {oid} 3\t.gitattributes\0").into_bytes()),
                "cat-file" => Ok(b"raw".to_vec()),
                _ => Err("host helper requested".into()),
            }
        }
    }
    let mut recorded = Recorded(Vec::new());
    let mut export = Export {
        entries: BTreeMap::new(),
        bytes: 0,
        started: Instant::now(),
    };
    export.walk(
        &mut recorded,
        Path::new("recording-only"),
        &"b".repeat(40),
        "",
        0,
    )?;
    assert_eq!(export.entries[".gitattributes"].data, b"raw");
    let tar = pack(export.entries, "#[test] fn same() {}")?;
    assert!(tar.windows(3).any(|w| w == b"raw"));
    assert!(
        tar.windows(GENERATED.len())
            .any(|w| w == GENERATED.as_bytes())
    );
    assert_eq!(recorded.0.len(), 2);
    assert!(recorded.0[0].starts_with("ls-tree --full-tree -r -z -l"));
    assert!(recorded.0[1].starts_with("cat-file blob"));
    Ok(())
}
#[test]
fn r2_pure_archive_rejects_traversal_links_and_preserves_long_paths_modes_and_identical_test()
-> Result<(), String> {
    for path in [
        "../outside",
        "/etc/x",
        ".git/hooks/post-checkout",
        "src/../../x",
        "C:/x",
    ] {
        assert!(safe_path(path).is_err());
    }
    assert!(safe_link("src/link", b"../../outside").is_err());
    assert_eq!(safe_link("src/link", b"../safe")?, "../safe");
    let mut entries = BTreeMap::new();
    entries.insert(
        "tests".into(),
        Entry {
            mode: 0o120000,
            data: b"src".to_vec(),
        },
    );
    assert!(pack(entries, "test").is_err());
    let mut entries = BTreeMap::new();
    let long = format!("src/{}/file.rs", "a".repeat(200));
    entries.insert(
        long.clone(),
        Entry {
            mode: 0o755,
            data: b"raw executable".to_vec(),
        },
    );
    let archive = pack(entries, "identical test")?;
    assert!(archive.windows(long.len()).any(|w| w == long.as_bytes()));
    assert!(archive.windows(7).any(|w| w == b"0000755"));
    assert!(archive.windows(14).any(|w| w == b"identical test"));
    assert!(archive.ends_with(&[0; 1024]));
    Ok(())
}
#[test]
#[ignore = "R2 runtime hold: hook/filter sentinel fixture authored, not executed"]
fn r2_runtime_raw_archive_does_not_execute_checkout_or_filter_sentinels() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("reproducer-inert-export")?;
    r.commit("source.txt", "raw source\n", "source")?;
    r.commit(
        ".gitattributes",
        "source.txt filter=sentinel\n",
        "attributes",
    )?;
    let marker = r.path().join("HOST_SENTINEL");
    let script = format!(
        "printf ran >> {}; cat",
        remote::sh_quote(marker.to_str().ok_or("path")?)
    );
    r.git(&["config", "filter.sentinel.smudge", &script])?;
    r.git(&["config", "filter.sentinel.process", &script])?;
    let hooks = r.path().join("hooks");
    std::fs::create_dir_all(&hooks).map_err(|e| e.to_string())?;
    std::fs::write(
        hooks.join("post-checkout"),
        format!("#!/bin/sh\n{script}\n"),
    )
    .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            hooks.join("post-checkout"),
            std::fs::Permissions::from_mode(0o755),
        )
        .map_err(|e| e.to_string())?;
    }
    r.git(&["config", "core.hooksPath", hooks.to_str().ok_or("path")?])?;
    std::fs::write(
        hooks.join("reference-transaction"),
        format!("#!/bin/sh\n{script}\n"),
    )
    .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            hooks.join("reference-transaction"),
            std::fs::Permissions::from_mode(0o755),
        )
        .map_err(|e| e.to_string())?;
    }
    let bytes = archive(r.path(), &r.tip("HEAD")?, "#[test] fn repro() {}")?;
    assert!(bytes.windows(10).any(|w| w == b"raw source"));
    save(r.path(), "recording-sentinel", "case", &"inert metadata")?;
    assert!(!marker.exists());
    Ok(())
}
