use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static IDS: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "gitcraft-clone-options-{}-{}",
            std::process::id(),
            IDS.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        let f = Self(path);
        Ok(f)
    }
    fn repo(&self) -> Result<PathBuf, String> {
        let repo = self.0.join("source");
        std::fs::create_dir(&repo).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "commit.gpgSign", "false"],
            vec!["config", "uploadpack.allowFilter", "true"],
        ] {
            run(Some(&repo), &args)?;
        }
        for folder in ["src/deep", "docs", "-odd", "with space", "[literal]"] {
            std::fs::create_dir_all(repo.join(folder)).map_err(|e| e.to_string())?;
            std::fs::write(repo.join(folder).join("file.txt"), folder)
                .map_err(|e| e.to_string())?;
        }
        std::fs::write(repo.join("README"), "root").map_err(|e| e.to_string())?;
        run(Some(&repo), &["add", "-A"])?;
        run(Some(&repo), &["commit", "-qm", "one"])?;
        std::fs::write(repo.join("docs/file.txt"), "two").map_err(|e| e.to_string())?;
        run(Some(&repo), &["commit", "-qam", "two"])?;
        Ok(repo)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn option_arguments_do_not_become_commands_or_patterns() -> Result<(), String> {
    for path in [
        "/absolute",
        "../escape",
        "x/../y",
        "a//b",
        ".git",
        "x/.Git/z",
        "C:/x",
        "x\\y",
        "line\nbreak",
        "",
    ] {
        assert!(validate_folders(&[path.into()]).is_err(), "{path}");
    }
    validate_folders(&["-option".into(), "[literal]".into(), "with space/é".into()])?;
    assert!(
        CloneOptions {
            depth: Some(0),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    let opts = CloneOptions {
        depth: Some(3),
        blobless: true,
        folders: Some(vec!["-x".into()]),
    };
    let args = opts.args("--upload-pack=evil", "dest")?;
    assert_eq!(
        &args[args.len() - 3..],
        ["--", "--upload-pack=evil", "dest"]
    );
    assert!(args.contains(&"--sparse".into()));
    assert!(!args.contains(&"--no-checkout".into()));
    assert!(args.contains(&"--filter=blob:none".into()));
    Ok(())
}

#[test]
fn shallow_sparse_clone_and_folder_edits_preserve_history_and_changes() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo()?;
    let dest = f.0.join("cloned with spaces");
    clone_with_options(
        &source.to_string_lossy(),
        &dest,
        None,
        &CloneOptions {
            depth: Some(1),
            blobless: false,
            folders: Some(vec!["src".into(), "-odd".into(), "[literal]".into()]),
        },
    )?;
    assert_eq!(
        run(Some(&dest), &["rev-list", "--count", "HEAD"])?.trim(),
        "1"
    );
    assert!(dest.join("README").exists());
    assert!(dest.join("src/deep/file.txt").exists());
    assert!(dest.join("-odd/file.txt").exists());
    assert!(dest.join("[literal]/file.txt").exists());
    assert!(!dest.join("docs/file.txt").exists());
    let before = sparse_state(&dest)?;
    assert!(before.folders.contains(&"docs".into()));
    set_sparse(&dest, &before, Some(&["docs".into()]))?;
    assert!(dest.join("docs/file.txt").exists());
    assert!(!dest.join("src/deep/file.txt").exists());
    assert!(set_sparse(&dest, &before, None).is_err());
    let before = sparse_state(&dest)?;
    std::fs::write(dest.join("docs/file.txt"), "unsaved").map_err(|e| e.to_string())?;
    assert!(set_sparse(&dest, &before, Some(&[])).is_err());
    assert_eq!(
        std::fs::read_to_string(dest.join("docs/file.txt")).map_err(|e| e.to_string())?,
        "unsaved"
    );
    run(Some(&dest), &["restore", "docs/file.txt"])?;
    let before = sparse_state(&dest)?;
    set_sparse(&dest, &before, Some(&[]))?;
    assert!(!dest.join("docs/file.txt").exists());
    let before = sparse_state(&dest)?;
    set_sparse(&dest, &before, None)?;
    assert!(dest.join("src/deep/file.txt").exists());
    assert!(dest.join("docs/file.txt").exists());
    Ok(())
}

#[test]
fn blobless_clone_downloads_only_selected_contents_then_can_expand() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo()?;
    let dest = f.0.join("partial");
    clone_with_options(
        &source.to_string_lossy(),
        &dest,
        None,
        &CloneOptions {
            blobless: true,
            folders: Some(vec!["src".into()]),
            ..Default::default()
        },
    )?;
    assert!(is_partial_clone(&dest));
    let missing = run(
        Some(&dest),
        &["rev-list", "--objects", "--all", "--missing=print"],
    )?;
    assert!(missing.lines().any(|l| l.starts_with('?')), "{missing}");
    let state = sparse_state(&dest)?;
    assert!(state.partial);
    set_sparse(&dest, &state, Some(&["docs".into()]))?;
    assert_eq!(
        std::fs::read_to_string(dest.join("docs/file.txt")).map_err(|e| e.to_string())?,
        "two"
    );
    Ok(())
}

#[test]
fn missing_blob_probe_does_not_download_and_changes_after_reading() -> Result<(), String> {
    let f = Fixture::new()?;
    let source = f.repo()?;
    let dest = f.0.join("probe");
    clone_with_options(
        &source.to_string_lossy(),
        &dest,
        None,
        &CloneOptions {
            blobless: true,
            folders: Some(vec!["src".into()]),
            ..Default::default()
        },
    )?;
    let before = run(
        Some(&dest),
        &["rev-list", "--objects", "--all", "--missing=print"],
    )?;
    assert!(file_needs_download(&dest, "HEAD~1", "docs/file.txt")?);
    assert!(!file_needs_download(&dest, "HEAD", "src/deep/file.txt")?);
    assert!(!file_needs_download(&dest, "HEAD", "absent")?);
    assert_eq!(
        before,
        run(
            Some(&dest),
            &["rev-list", "--objects", "--all", "--missing=print"]
        )?
    );
    assert_eq!(
        super::super::file_at(&dest, Some("HEAD~1"), "docs/file.txt")?,
        b"docs"
    );
    assert!(!file_needs_download(&dest, "HEAD~1", "docs/file.txt")?);
    Ok(())
}

#[test]
fn sparse_normal_repo_non_cone_untracked_and_head_guards() -> Result<(), String> {
    let f = Fixture::new()?;
    let repo = f.repo()?;
    let state = sparse_state(&repo)?;
    assert!(!state.enabled);
    set_sparse(&repo, &state, Some(&["src".into()]))?;
    let state = sparse_state(&repo)?;
    std::fs::write(repo.join("untracked"), "keep").map_err(|e| e.to_string())?;
    assert!(set_sparse(&repo, &state, Some(&[])).is_err());
    std::fs::remove_file(repo.join("untracked")).map_err(|e| e.to_string())?;
    run(Some(&repo), &["commit", "--allow-empty", "-qm", "moved"])?;
    assert!(set_sparse(&repo, &state, None).is_err());
    run(
        Some(&repo),
        &["sparse-checkout", "set", "--no-cone", "/docs/"],
    )?;
    let state = sparse_state(&repo)?;
    assert!(state.enabled && !state.cone);
    assert!(state.patterns.contains("/docs/"));
    set_sparse(&repo, &state, Some(&["src/deep".into()]))?;
    assert!(sparse_state(&repo)?.cone);
    Ok(())
}
