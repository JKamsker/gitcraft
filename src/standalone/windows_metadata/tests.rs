use super::*;
#[test]
fn metadata_protocol_records_only_constant_script_and_literal_native_path_data()
-> Result<(), String> {
    let source = Path::new("source ' $() & ü");
    let target = Path::new("target ' $() & ü");
    let cmd = command(Path::new("powershell-fixture"), source, target);
    let args = cmd
        .get_args()
        .map(|s| s.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        &args[..4],
        [
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand"
        ]
    );
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&args[4])
        .map_err(|e| e.to_string())?;
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    assert_eq!(
        String::from_utf16(&units).map_err(|e| e.to_string())?,
        SCRIPT
    );
    assert!(!SCRIPT.contains("source ' $()"));
    assert!(!SCRIPT.contains("Invoke-Expression"));
    let env = cmd.get_envs().collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        env.get(std::ffi::OsStr::new("GITCRAFT_ACL_SOURCE")),
        Some(&Some(source.as_os_str()))
    );
    assert_eq!(
        env.get(std::ffi::OsStr::new("GITCRAFT_ACL_TARGET")),
        Some(&Some(target.as_os_str()))
    );
    assert_eq!(env.get(std::ffi::OsStr::new("PSModulePath")), Some(&None));
    assert_eq!(
        env.get(std::ffi::OsStr::new("GITCRAFT_ACL_SNAPSHOT")),
        Some(&None)
    );
    assert!(SCRIPT.contains("AllPolicy = 0x000001ff"));
    assert!(SCRIPT.contains("SACL|LABEL|ATTRIBUTE|SCOPE|PROCESS_TRUST_LABEL|ACCESS_FILTER"));
    assert!(SCRIPT.contains("BasicPolicy = 0x00000007"));
    assert!(SCRIPT.contains("if (error == 1314)"));
    assert!(SCRIPT.contains("info = BasicPolicy"));
    assert!(SCRIPT.contains("GetNamedSecurityInfo(path,1,info"));
    assert!(SCRIPT.contains("SetNamedSecurityInfo(target,1,info|protection"));
    assert!(SCRIPT.contains("if ((info & 0x8) != 0) protection |="));
    assert!(SCRIPT.contains("parsed.SetFlags(parsed.ControlFlags & ~InheritanceStatus);"));
    assert!(SCRIPT.contains("ControlFlags.DiscretionaryAclProtected"));
    assert!(SCRIPT.contains("ControlFlags.SystemAclProtected"));
    assert!(SCRIPT.contains("Array.Copy(BitConverter.GetBytes(info),snapshot,4)"));
    assert!(SCRIPT.contains("SeSecurityPrivilege"));
    assert!(!SCRIPT.contains("[IO.File]::GetAccessControl"));
    assert!(SCRIPT.contains("length > 1048576"));
    assert!(SCRIPT.contains("if ($snapshot)"));
    assert!(SCRIPT.find("if ($snapshot)") < SCRIPT.rfind("[GitcraftNativePolicy]::Copy"));
    Ok(())
}
