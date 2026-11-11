use super::*;
#[test]
fn blobless_sparse_history_stays_missing_until_explicit_consent() -> Result<(), String> {
    struct Owned(std::path::PathBuf);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "gitcraft-clone-policy-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let owned = Owned(root);
    let source = owned.0.join("source");
    std::fs::create_dir(&source).map_err(|e| e.to_string())?;
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.invalid"],
        vec!["config", "commit.gpgSign", "false"],
        vec!["config", "uploadpack.allowFilter", "true"],
    ] {
        run(Some(&source), &args)?;
    }
    std::fs::create_dir(source.join("folder")).map_err(|e| e.to_string())?;
    for content in ["old contents", "new contents"] {
        std::fs::write(source.join("folder/file"), content).map_err(|e| e.to_string())?;
        run(Some(&source), &["add", "."])?;
        run(Some(&source), &["commit", "-qm", "fixture"])?;
    }
    let dest = owned.0.join("clone");
    let url = source.to_str().ok_or("non-UTF8 fixture path")?;
    clone_with_options(
        url,
        &dest,
        None,
        &CloneOptions {
            depth: Some(2),
            blobless: true,
            folders: Some(vec![]),
        },
        &super::super::OpControl::new(|| {}),
    )?;
    assert_eq!(
        run(Some(&dest), &["rev-parse", "--is-shallow-repository"])?.trim(),
        "true"
    );
    let missing = || {
        run(
            Some(&dest),
            &["rev-list", "--objects", "--missing=print", "--all"],
        )
    };
    let before = missing()?;
    assert!(before.lines().any(|s| s.starts_with('?')));
    assert!(run(Some(&dest), &["show", "HEAD~:folder/file"]).is_err());
    assert_eq!(
        missing()?,
        before,
        "passive history selection hydrated an object"
    );
    let state = sparse_state(&dest)?;
    assert!(set_sparse(&dest, &state, Some(&["folder".into()]), false, None).is_err());
    let state = sparse_state(&dest)?;
    set_sparse(&dest, &state, Some(&["folder".into()]), true, None)?;
    assert_eq!(
        std::fs::read_to_string(dest.join("folder/file")).map_err(|e| e.to_string())?,
        "new contents"
    );
    assert!(!downloads_allowed());
    Ok(())
}
