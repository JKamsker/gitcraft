use super::*;
#[test]
fn recording_transfer_has_no_source_ref_mutations_and_creates_exact_generation_refs()
-> Result<(), Box<dyn std::error::Error>> {
    for width in [40, 64] {
        let wanted = vec!["a".repeat(width), "b".repeat(width)];
        let present = vec!["c".repeat(width)];
        let bundle = TransferBundle::header(&wanted, &present, &"d".repeat(64), &"e".repeat(32))?;
        let header = String::from_utf8(bundle.bytes.clone())?;
        assert!(header.starts_with(if width == 40 {
            "# v2 git bundle\n"
        } else {
            "# v3 git bundle\n@object-format=sha256\n"
        }));
        assert!(header.contains(&format!("-{} gitcraft prerequisite\n", present[0])));
        assert!(header.ends_with("\n\n"));
        for (name, hash) in &bundle.refs {
            assert!(name.starts_with(&format!("refs/gitcraft-bisect/{}/sent-", "d".repeat(64))));
            assert!(header.contains(&format!("{hash} {name}\n")));
        }
        let transaction = bundle.ref_transaction();
        assert!(transaction.starts_with("start\ncreate refs/gitcraft-bisect/"));
        assert!(!transaction.contains("update ") && !transaction.contains("delete "));
        assert!(transaction.ends_with("prepare\ncommit\n"));
    }
    for bad in [
        "HEAD".to_owned(),
        "a".repeat(39),
        format!("{}\nrefs/heads/main", "a".repeat(40)),
    ] {
        assert!(TransferBundle::header(&[bad], &[], &"d".repeat(64), &"e".repeat(32)).is_err());
    }
    assert_eq!(
        transfer_digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    Ok(())
}
#[test]
fn recording_cleanup_requires_exact_generation_and_structured_complete_receipt()
-> Result<(), Box<dyn std::error::Error>> {
    let authority = WorkspaceAuthority {
        root: "/synthetic".into(),
        anchor: "original".into(),
        generation: "a".repeat(64),
        mode: FenceMode::Mutation,
    };
    let mut receipt = serde_json::json!({"v":1,"state":"cleaned","anchor":"original","generation":authority.generation,"records":2,"records_sha256":"b".repeat(64)});
    assert!(cleaned_receipt(&serde_json::to_vec(&receipt)?, &authority));
    receipt["generation"] = "old".into();
    assert!(!cleaned_receipt(&serde_json::to_vec(&receipt)?, &authority));
    assert!(!cleaned_receipt(b"true", &authority));
    Ok(())
}
