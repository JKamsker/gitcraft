#[test]
fn explicit_save_retains_the_old_inode_and_never_writes_inputs() -> Result<(), String> {
    use std::io::{Seek, SeekFrom, Write};
    let temp = crate::git::file_comparison::Scratch::new()?;
    let source = temp.0.join("local");
    let merged = temp.0.join("merged");
    write(&source, b"input\0\xff")?;
    write(&merged, b"original")?;
    let inputs = vec![Snapshot::read(&source, &|| true)?];
    let output = Output::capture(&merged, &inputs)?;
    let mut writer = None;
    let save = output.save_with(b"new result", &inputs, &mut |phase| {
        if phase == Phase::BeforeMove {
            writer = std::fs::OpenOptions::new().write(true).open(&merged).ok();
        }
    });
    #[cfg(windows)]
    let saved = {
        // Windows pins the displaced file without write sharing while checking
        // its identity. An already-open writer must cause a safe refusal,
        // not a successful save based on an unstable snapshot.
        assert!(writer.is_some(), "No fixture writer");
        assert!(save.is_err(), "Windows must refuse an active writer");
        assert_eq!(read(&merged)?, b"original");
        assert_eq!(read(&source)?, b"input\0\xff");
        drop(writer.take());
        Output::capture(&merged, &inputs)?.save(b"new result", &inputs)?
    };
    #[cfg(not(windows))]
    let saved = save?;
    assert_eq!(read(&merged)?, b"new result");
    assert_eq!(read(&source)?, b"input\0\xff");
    let backup = saved.previous.ok_or("No displaced recovery file")?;
    assert_eq!(read(&backup)?, b"original");
    // Recovery must preserve the old file identity, not just copy its bytes.
    let old_identity = output.original.as_ref().and_then(|s| s.handle.clone());
    assert_eq!(Snapshot::read(&backup, &|| true)?.handle, old_identity);
    #[cfg(windows)]
    let mut writer = std::fs::OpenOptions::new()
        .write(true)
        .open(&backup)
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let mut writer = writer.ok_or("No fixture writer")?;
    writer
        .seek(SeekFrom::Start(0))
        .and_then(|_| writer.write_all(b"late edit"))
        .map_err(|e| e.to_string())?;
    assert_eq!(read(&backup)?, b"late edit");
    assert_eq!(read(&merged)?, b"new result");
    Ok(())
}
#[test]
fn destination_drift_and_concurrent_creators_are_preserved_without_success() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    for (phase, expected) in [
        (Phase::BeforeMove, b"changed before move".as_slice()),
        (Phase::BeforeInstall, b"concurrent creator".as_slice()),
        (Phase::AfterInstall, b"concurrent result edit".as_slice()),
    ] {
        let path = temp.0.join(format!("{phase:?}"));
        write(&path, b"original")?;
        let output = Output::capture(&path, &[])?;
        let result = output.save_with(b"candidate", &[], &mut |at| {
            if at == phase {
                let _ = std::fs::write(&path, expected);
            }
        });
        assert!(result.is_err());
        assert_eq!(read(&path)?, expected);
    }
    let path = temp.0.join("changed");
    write(&path, b"original")?;
    let output = Output::capture(&path, &[])?;
    write(&path, b"new owner bytes")?;
    assert!(output.save(b"candidate", &[]).is_err());
    assert_eq!(read(&path)?, b"new owner bytes");
    Ok(())
}
#[test]
fn missing_outputs_install_create_only_aliases_and_read_only_outputs_are_refused()
-> Result<(), String> {
    assert!(Output::capture(Path::new("/dev/null"), &[]).is_err());
    #[cfg(windows)]
    assert!(Output::capture(Path::new("NUL"), &[]).is_err());
    let temp = crate::git::file_comparison::Scratch::new()?;
    let path = temp.0.join("new");
    let output = Output::capture(&path, &[])?;
    assert!(output.save(b"new bytes", &[])?.previous.is_none());
    assert_eq!(read(&path)?, b"new bytes");
    let input = Snapshot::read(&path, &|| true)?;
    assert!(Output::capture(&path, &[input]).is_err());
    let hard = temp.0.join("hard-alias");
    std::fs::hard_link(&path, &hard).map_err(|e| e.to_string())?;
    assert!(Output::capture(&hard, &[Snapshot::read(&path, &|| true)?]).is_err());
    let original_permissions = std::fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .permissions();
    let mut readonly = original_permissions.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&path, readonly).map_err(|e| e.to_string())?;
    assert!(Output::capture(&path, &[]).is_err());
    std::fs::set_permissions(&path, original_permissions).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(unix)]
#[test]
fn swapped_parents_and_output_links_never_redirect_writes() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let a = temp.0.join("a");
    let b = temp.0.join("b");
    std::fs::create_dir(&a)
        .and_then(|_| std::fs::create_dir(&b))
        .map_err(|e| e.to_string())?;
    write(&a.join("result"), b"a original")?;
    write(&b.join("result"), b"b sentinel")?;
    let alias = temp.0.join("parent-link");
    std::os::unix::fs::symlink(&a, &alias).map_err(|e| e.to_string())?;
    let output = Output::capture(&alias.join("result"), &[])?;
    std::fs::remove_file(&alias).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&b, &alias).map_err(|e| e.to_string())?;
    assert!(output.save(b"candidate", &[]).is_err());
    assert_eq!(read(&a.join("result"))?, b"a original");
    assert_eq!(read(&b.join("result"))?, b"b sentinel");
    let linked = temp.0.join("output-link");
    std::os::unix::fs::symlink(b.join("result"), &linked).map_err(|e| e.to_string())?;
    assert!(Output::capture(&linked, &[]).is_err());
    assert_eq!(read(&b.join("result"))?, b"b sentinel");
    Ok(())
}
