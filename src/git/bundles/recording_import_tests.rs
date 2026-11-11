use super::*;
fn refs(names: &[&str]) -> Vec<BundleRef> {
    names
        .iter()
        .map(|n| BundleRef {
            name: (*n).into(),
            oid: "a".repeat(40),
        })
        .collect()
}
#[test]
fn selected_refs_map_to_new_names_and_reject_stale_or_colliding_selection() -> Result<(), String> {
    let refs = refs(&["HEAD", "refs/heads/main", "refs/tags/v1"]);
    let selected = ["HEAD", "refs/tags/v1"]
        .map(str::to_owned)
        .into_iter()
        .collect();
    let mapped = import_mapping(&refs, &selected, "handoff_1")?;
    assert_eq!(
        mapped.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        [
            "refs/bundle-import/handoff_1/HEAD",
            "refs/bundle-import/handoff_1/tags/v1"
        ]
    );
    assert!(mapped.iter().all(|r| r.oid == "a".repeat(40)));
    for prefix in [
        "",
        "../main",
        "NEW",
        "a/b",
        "refs/heads/main",
        "x\ncreate other",
    ] {
        assert!(import_mapping(&refs, &selected, prefix).is_err());
    }
    assert!(import_mapping(&refs, &BTreeSet::new(), "new").is_err());
    assert!(
        import_mapping(
            &refs,
            &["refs/heads/stale".into()].into_iter().collect(),
            "new"
        )
        .is_err()
    );
    for names in [
        ["refs/heads/Main", "refs/heads/main"],
        ["refs/heads/a", "refs/heads/a/b"],
    ] {
        let colliding = self::refs(&names);
        let all = colliding.iter().map(|r| r.name.clone()).collect();
        assert!(import_mapping(&colliding, &all, "new").is_err());
    }
    Ok(())
}
#[test]
fn held_snapshot_digest_binds_pack_bytes_not_only_advertised_header() -> Result<(), String> {
    let header = format!(
        "# v2 git bundle\n{} refs/heads/main\n\nPACK",
        "a".repeat(40)
    );
    let original = header.into_bytes();
    let mut altered = original.clone();
    altered.push(0);
    assert_eq!(self::header(&original)?, self::header(&altered)?);
    assert_ne!(bundle_digest(&original), bundle_digest(&altered));
    let (refs, prerequisites) = self::header(&altered)?;
    let preview = BundlePreview {
        refs,
        prerequisites,
        verification_error: None,
        bytes: std::sync::Arc::new(altered),
        digest: bundle_digest(&original),
        repository: "recording-only".into(),
    };
    // This pure production guard runs before any capability probe. No process-
    // capable backend exists in this test, including on a validation regression.
    assert!(checked_snapshot(&preview).is_err_and(|error| error.contains("snapshot changed")));
    assert_eq!(
        bundle_digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    Ok(())
}
