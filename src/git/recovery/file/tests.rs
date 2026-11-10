use super::*;
#[test]
fn destinations_refuse_metadata_aliases_devices_and_traversal() {
    for path in [
        "../x",
        ".git/config",
        "GIT~1/config",
        ".git\u{200c}/config",
        "a/../b",
        "a\\b",
        "C:/x",
        "NUL",
        "lpt1.txt",
        "a/./b",
        "space./x",
        "x ",
    ] {
        assert!(valid_path(path).is_err(), "{path}");
    }
    assert!(valid_path("recovered ü ' file.rs").is_ok());
}
#[cfg(unix)]
#[test]
fn remote_new_file_script_is_quoted_exclusive_and_keeps_existing_and_linked_files()
-> Result<(), String> {
    use std::process::{Command, Stdio};
    let f = super::super::tests::Fixture::new()?;
    let mut identity = Reader::new(&f.0, &|| true).identity()?;
    let probe = Command::new("sh")
        .args(["-c", &pin_script(identity.root.as_deref().ok_or("root")?)])
        .output()
        .map_err(|e| e.to_string())?;
    identity.root_pin = Some(parse_pin(&probe.stdout)?);
    let run = |name: &str, bytes: &[u8]| -> Result<std::process::Output, String> {
        let script = remote_script(&identity, name, true)?;
        let mut child = Command::new("sh")
            .args(["-c", &script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let written = child.stdin.take().ok_or("stdin")?.write_all(bytes);
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if let Err(error) = written
            && (error.kind() != std::io::ErrorKind::BrokenPipe || output.status.success())
        {
            return Err(error.to_string());
        }
        Ok(output)
    };
    let name = "literal ' $(touch marker) ü";
    assert!(run(name, b"raw\0bytes")?.status.success());
    assert_eq!(
        std::fs::read(f.0.join(name)).map_err(|e| e.to_string())?,
        b"raw\0bytes"
    );
    assert!(!run(name, b"overwrite")?.status.success());
    std::os::unix::fs::symlink("missing", f.0.join("link")).map_err(|e| e.to_string())?;
    assert!(!run("link", b"bad")?.status.success());
    assert!(!run("link/file", b"bad")?.status.success());
    assert!(!f.0.join("marker").exists());
    assert!(!f.0.join("missing").exists());
    assert!(
        !std::fs::read_dir(f.0.join(".git"))
            .map_err(|e| e.to_string())?
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("gitcraft-content-recovery."))
    );
    Ok(())
}
