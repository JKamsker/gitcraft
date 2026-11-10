use super::*;
#[test]
fn label_and_audit_fixtures_record_only_owned_literal_paths() -> Result<(), String> {
    let path = Path::new("owned ' $() & ü");
    let label = fixture_policy_command(Path::new("icacls-fixture"), path, false);
    assert_eq!(
        label.get_args().collect::<Vec<_>>(),
        [
            path.as_os_str(),
            std::ffi::OsStr::new("/setintegritylevel"),
            std::ffi::OsStr::new("L"),
            std::ffi::OsStr::new("/Q")
        ]
    );
    let audit = fixture_policy_command(Path::new("powershell-fixture"), path, true);
    let args = audit.get_args().collect::<Vec<_>>();
    assert_eq!(args.len(), 5);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(args[4].to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())?;
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    assert_eq!(
        String::from_utf16(&units).map_err(|e| e.to_string())?,
        AUDIT_FIXTURE
    );
    assert!(AUDIT_FIXTURE.contains("AccessControlSections]::All"));
    assert!(AUDIT_FIXTURE.contains("FileSystemAuditRule"));
    assert!(!AUDIT_FIXTURE.contains("owned '"));
    assert_eq!(
        audit
            .get_envs()
            .find(|(k, _)| *k == "GITCRAFT_OWNED_POLICY_PATH")
            .and_then(|(_, v)| v),
        Some(path.as_os_str())
    );
    Ok(())
}
