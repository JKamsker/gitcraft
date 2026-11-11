use super::*;
#[test]
fn remote_program_records_no_follow_create_only_policy_and_literal_data() -> Result<(), String> {
    let r = super::super::tests::Repo::new()?;
    let path = "literal ' $(not-code) ü";
    r.write(path, "old\n")?;
    r.commit()?;
    r.write(path, "new\n")?;
    let lens = r.lens(path)?;
    let script = remote_apply_script(&lens, "fixture patch", false)?;
    assert!(script.contains("requires Python 3"));
    assert!(script.contains("O_NOFOLLOW"));
    assert!(script.contains("resource.setrlimit(resource.RLIMIT_FSIZE"));
    assert!(script.contains("stdout=stdout, stderr=stderr"));
    assert!(!script.contains("subprocess.PIPE"));
    assert!(script.contains("O_EXCL"));
    assert!(script.contains("src_dir_fd=recovery,dst_dir_fd=parent,follow_symlinks=False"));
    assert!(script.contains("os.fchown"));
    assert!(script.contains("acl_get_fd_np"));
    assert!(script.contains("core.sharedRepository"));
    assert!(script.contains("released-lock"));
    assert!(!script.contains("cat \"$d/index\" >\"$lock\""));
    let json = script
        .split("<<'GITCRAFT_INDEX_DATA'\n")
        .nth(1)
        .ok_or("payload")?
        .lines()
        .next()
        .ok_or("json")?;
    let data: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    assert_eq!(data["path"], path);
    assert_eq!(
        data["patch"],
        base64::engine::general_purpose::STANDARD.encode(b"fixture patch")
    );
    assert!(remote_stamp_read().contains("O_NOFOLLOW"));
    let md5 = portable_md5();
    assert!(md5.contains("md5sum"));
    assert!(md5.contains("md5 -q"));
    assert!(md5.contains("900150983cd24fb0d6963f7d28e17f72"));
    Ok(())
}
