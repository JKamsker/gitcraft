//! Paths.

use super::*;

#[cfg(unix)]
#[test]
pub(super) fn link_targets_and_binary_versions_are_read_without_following_links()
-> Result<(), String> {
    let r = Repo::new()?;
    r.write("binary", b"\0\xfforiginal")?;
    std::os::unix::fs::symlink("/dev/zero", r.0.join("link")).map_err(|e| e.to_string())?;
    r.commit()?;
    let link = r.lens("link")?;
    assert_eq!(link.versions[2].mode.as_deref(), Some("120000"));
    assert_eq!(link.versions[2].bytes, b"/dev/zero");
    assert!(r.lens("binary")?.versions[2].text().is_none());
    let outside = Repo::new()?;
    outside.write("secret", b"outside bytes must not be exposed")?;
    std::os::unix::fs::symlink(&outside.0, r.0.join("escape")).map_err(|e| e.to_string())?;
    assert!(
        r.lens("escape/secret")
            .is_err_and(|e| e.contains("not safely readable"))
    );
    assert_eq!(
        std::fs::read(outside.0.join("secret")).map_err(|e| e.to_string())?,
        b"outside bytes must not be exposed"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
pub(super) fn raw_non_utf8_paths_stay_distinct_and_literal_folders_do_not_expand_globs()
-> Result<(), String> {
    use std::os::unix::ffi::OsStringExt;
    let r = Repo::new()?;
    r.write("[folder]/file", "literal\n")?;
    r.commit()?;
    // APFS cannot create invalid UTF-8 filenames, but Git trees preserve raw paths.
    // Populate the index directly so this tests the object model on every Unix host.
    let oid = r.git(&["rev-parse", "HEAD:[folder]/file"])?;
    for bytes in [b"odd\xff".to_vec(), b"odd\xfe".to_vec()] {
        let out = r
            .command(&["update-index", "--add", "--cacheinfo", "100644", &oid])
            .arg(std::ffi::OsString::from_vec(bytes))
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into());
        }
    }
    r.git(&["commit", "-qm", "raw paths"])?;
    r.git(&["branch", "release"])?;
    let m = matrix(
        &r.0,
        &["HEAD".into(), "refs/heads/release".into()],
        "",
        &|| true,
    )?;
    assert!(m.paths.contains(&b"odd\xff".to_vec()));
    assert!(m.paths.contains(&b"odd\xfe".to_vec()));
    let m = matrix(
        &r.0,
        &["HEAD".into(), "refs/heads/release".into()],
        "[folder]",
        &|| true,
    )?;
    assert_eq!(m.paths, [b"[folder]/file".to_vec()]);
    Ok(())
}
