use super::*;
use crate::git::investigations::tests::Repo;
fn formatting() -> Result<(Repo, String), String> {
    let r = Repo::new()?;
    for (i, text) in ["fn f() { return 1; }\n", "fn f(){return 1;}\n"]
        .into_iter()
        .enumerate()
    {
        std::fs::write(r.0.join("f.rs"), text).map_err(|e| e.to_string())?;
        r.git(&["add", "f.rs"])?;
        r.commit("ignore@test.invalid", 1_700_000_000 + i as i64)?;
    }
    let head = r.git(&["rev-parse", "HEAD"])?;
    Ok((r, head))
}
#[test]
fn preview_is_read_only_append_preserves_all_old_bytes_and_the_staged_index() -> Result<(), String>
{
    let (r, head) = formatting()?;
    std::fs::write(r.0.join(NAME), b"# existing staged note").map_err(|e| e.to_string())?;
    r.git(&["add", NAME])?;
    std::fs::write(
        r.0.join(NAME),
        b"# existing staged note\n# unstaged note without final newline",
    )
    .map_err(|e| e.to_string())?;
    let old = std::fs::read(r.0.join(NAME)).map_err(|e| e.to_string())?;
    let index = std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?;
    let refs = r.git(&["show-ref"])?;
    let preview = prepare(&r.0, &head, &|| true)?;
    assert!(preview.patch.contains("return 1"));
    assert_eq!(preview.existing.as_bytes(), old);
    assert_eq!(
        std::fs::read(r.0.join(NAME)).map_err(|e| e.to_string())?,
        old
    );
    assert_eq!(perform(&preview, &|| true)?, Appended::Changed);
    let mut expected = old;
    expected.extend(payload(&head));
    assert_eq!(
        std::fs::read(r.0.join(NAME)).map_err(|e| e.to_string())?,
        expected
    );
    assert_eq!(
        std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(r.git(&["show-ref"])?, refs);
    let again = prepare(&r.0, &head, &|| true)?;
    assert_eq!(perform(&again, &|| true)?, Appended::AlreadyPresent);
    assert_eq!(
        std::fs::read(r.0.join(NAME)).map_err(|e| e.to_string())?,
        expected
    );
    Ok(())
}
#[test]
fn stale_missing_existing_semantic_and_cancelled_edits_refuse_without_overwrite()
-> Result<(), String> {
    let (r, head) = formatting()?;
    let missing = prepare(&r.0, &head, &|| true)?;
    std::fs::write(r.0.join(NAME), b"new user data").map_err(|e| e.to_string())?;
    assert!(perform(&missing, &|| true).is_err());
    assert_eq!(
        std::fs::read(r.0.join(NAME)).map_err(|e| e.to_string())?,
        b"new user data"
    );
    let before = prepare(&r.0, &head, &|| true)?;
    std::fs::write(r.0.join(NAME), b"concurrent change").map_err(|e| e.to_string())?;
    assert!(perform(&before, &|| true).is_err());
    assert_eq!(
        std::fs::read(r.0.join(NAME)).map_err(|e| e.to_string())?,
        b"concurrent change"
    );
    assert!(prepare(&r.0, &head, &|| false).is_err());
    assert!(perform(&before, &|| false).is_err());
    std::fs::write(r.0.join("f.rs"), b"fn f(){return 2;}\n").map_err(|e| e.to_string())?;
    r.git(&["add", "f.rs"])?;
    let semantic = r.commit("ignore@test.invalid", 1_700_000_002)?;
    assert!(prepare(&r.0, &semantic, &|| true).is_err());
    assert!(prepare(&r.0, "--all", &|| true).is_err());
    Ok(())
}
#[cfg(unix)]
#[test]
fn links_hardlinks_and_root_replacement_never_modify_an_outside_file() -> Result<(), String> {
    let (r, head) = formatting()?;
    let outside = r.0.join("outside");
    std::fs::write(&outside, b"must stay unchanged").map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&outside, r.0.join(NAME)).map_err(|e| e.to_string())?;
    assert!(prepare(&r.0, &head, &|| true).is_err());
    std::fs::remove_file(r.0.join(NAME)).map_err(|e| e.to_string())?;
    std::fs::hard_link(&outside, r.0.join(NAME)).map_err(|e| e.to_string())?;
    assert!(prepare(&r.0, &head, &|| true).is_err());
    std::fs::remove_file(r.0.join(NAME)).map_err(|e| e.to_string())?;
    let preview = prepare(&r.0, &head, &|| true)?;
    let saved = r.0.with_extension("saved-root");
    std::fs::rename(&r.0, &saved).map_err(|e| e.to_string())?;
    std::fs::create_dir(&r.0).map_err(|e| e.to_string())?;
    let refused = perform(&preview, &|| true).is_err();
    assert!(!r.0.join(NAME).exists());
    std::fs::remove_dir(&r.0).map_err(|e| e.to_string())?;
    std::fs::rename(&saved, &r.0).map_err(|e| e.to_string())?;
    assert!(refused);
    assert_eq!(
        std::fs::read(&outside).map_err(|e| e.to_string())?,
        b"must stay unchanged"
    );
    Ok(())
}
#[test]
fn snapshot_and_payload_frames_reject_incomplete_or_large_untrusted_data() {
    assert!(
        Snapshot {
            bytes: Some(B64.encode(b"x")),
            identity: None,
            modified: None
        }
        .decoded()
        .is_err()
    );
    assert!(
        Snapshot {
            bytes: Some(B64.encode(vec![b'x'; LIMIT + 1])),
            identity: Some("1:2".into()),
            modified: Some("0".into())
        }
        .decoded()
        .is_err()
    );
    let cid = "a".repeat(40);
    assert!(payload(&cid).starts_with(b"\n# gitcraft:"));
    assert!(contains(format!("{cid} # a note\n").as_bytes(), &cid));
    assert!(!contains(format!("# {cid}\n").as_bytes(), &cid));
}
#[cfg(unix)]
#[test]
fn remote_helper_recording_fixture_appends_exclusively_and_refuses_stale_and_linked_files()
-> Result<(), String> {
    // Owned local interpreter fixture only; no SSH/WSL/network or host process selectors.
    // Execution is deferred until the coordinator lifts the runtime hold and approves fixtures.
    let (r, head) = formatting()?;
    let call = |op: &str, expected: &Snapshot| -> Result<serde_json::Value, String> {
        let mut command = std::process::Command::new("python3");
        command.args(["-c", REMOTE]).current_dir(&r.0);
        let data =
            serde_json::json!({"op":op,"expected":expected,"payload":B64.encode(payload(&head))})
                .to_string()
                .into_bytes();
        let out = crate::processes::run(
            command,
            crate::processes::Options {
                input: Some(data),
                background: true,
                timeout: Some(std::time::Duration::from_secs(5)),
                output_limit: 256 * 1024,
                stderr_limit: Some(8192),
                ..Default::default()
            },
        )?
        .complete()?;
        if !out.status.success() {
            return Err("Recording helper refused".into());
        }
        serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
    };
    let initial: Snapshot =
        serde_json::from_value(call("read", &Snapshot::missing())?).map_err(|e| e.to_string())?;
    assert_eq!(initial, Snapshot::missing());
    call("append", &initial)?;
    assert!(call("append", &initial).is_err());
    let state: Snapshot =
        serde_json::from_value(call("read", &initial)?).map_err(|e| e.to_string())?;
    assert_eq!(
        call("append", &state)?
            .get("result")
            .and_then(serde_json::Value::as_str),
        Some("present")
    );
    std::fs::remove_file(r.0.join(NAME)).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("/dev/zero", r.0.join(NAME)).map_err(|e| e.to_string())?;
    assert!(call("read", &initial).is_err());
    Ok(())
}
