//! Execute only noninteractive CLI cases; interactive behavior is exercised by the real UI harness.
use std::process::Command;

#[test]
fn help_and_usage_exit_without_repository_profile_or_display_fallback() -> Result<(), String> {
    let executable = env!("CARGO_BIN_EXE_gitcraft");
    for flag in ["--help", "--version"] {
        let out = Command::new(executable)
            .arg(flag)
            .env("GIT_DIR", "nonexistent-caller-repository")
            .env("GITCRAFT_ASKPASS", "not-a-valid-request")
            .output()
            .map_err(|e| e.to_string())?;
        assert_eq!(out.status.code(), Some(0));
        assert!(!out.stdout.is_empty());
    }
    for args in [
        vec!["--diff", "one"],
        vec!["--mergetool", "one", "two"],
        vec!["--unknown"],
    ] {
        let out = Command::new(executable)
            .args(args)
            .env("GIT_DIR", "nonexistent-caller-repository")
            .output()
            .map_err(|e| e.to_string())?;
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("Usage:"));
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn native_non_utf8_file_arguments_never_panic_in_utf8_helper_dispatch() -> Result<(), String> {
    use std::os::unix::ffi::OsStringExt;
    let out = Command::new(env!("CARGO_BIN_EXE_gitcraft"))
        .arg("--diff")
        .arg(std::ffi::OsString::from_vec(b"native\xffpath".to_vec()))
        .env("GITCRAFT_ASKPASS", "not-a-valid-request")
        .output()
        .map_err(|e| e.to_string())?;
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Expected 2"));
    Ok(())
}
