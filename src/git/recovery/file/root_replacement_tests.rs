use super::*;
use std::process::{Command, Stdio};
fn nested(f: &super::super::tests::Fixture, name: &str) -> Result<std::path::PathBuf, String> {
    let root = f.0.join(name);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    crate::git::run(Some(&root), &["init", "-q"])?;
    Ok(root)
}
#[test]
fn local_creation_rejects_root_and_ancestor_replacements_after_the_final_git_check()
-> Result<(), String> {
    let f = super::super::tests::Fixture::new()?;
    let root = nested(&f, "selected")?;
    let reader = Reader::new(&root, &|| true);
    let identity = reader.identity()?;
    let moved = f.0.join("saved-selected");
    let unrelated = nested(&f, "unrelated")?;
    std::fs::rename(&root, &moved).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(&unrelated, &root).map_err(|e| e.to_string())?;
    assert!(create(&reader, &identity, "must-not-escape", b"saved raw bytes").is_err());
    assert!(!unrelated.join("must-not-escape").exists());
    std::fs::remove_file(&root)
        .and_then(|_| std::fs::rename(&moved, &root))
        .map_err(|e| e.to_string())?;
    let root = nested(&f, "container/repo")?;
    let reader = Reader::new(&root, &|| true);
    let identity = reader.identity()?;
    let parent = f.0.join("container");
    let moved = f.0.join("saved-container");
    let outside = nested(&f, "outside-container/repo")?;
    std::fs::rename(&parent, &moved).map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink(outside.parent().ok_or("parent")?, &parent)
        .map_err(|e| e.to_string())?;
    assert!(create(&reader, &identity, "must-not-escape", b"saved raw bytes").is_err());
    assert!(!outside.join("must-not-escape").exists());
    std::fs::remove_file(&parent)
        .and_then(|_| std::fs::rename(&moved, &parent))
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[test]
fn typed_remote_publication_rejects_root_symlink_and_same_name_directory_substitution()
-> Result<(), String> {
    let f = super::super::tests::Fixture::new()?;
    let selected = nested(&f, "selected")?;
    let unrelated = nested(&f, "unrelated")?;
    for symlink in [true, false] {
        let mut identity = Reader::new(&selected, &|| true).identity()?;
        let out = Command::new("sh")
            .args(["-c", &pin_script(selected.to_str().ok_or("utf8")?)])
            .output()
            .map_err(|e| e.to_string())?;
        identity.root_pin = Some(parse_pin(&out.stdout)?);
        let script = remote_script(&identity, "must-not-escape", true)?;
        let saved = f.0.join("saved-selected");
        std::fs::rename(&selected, &saved).map_err(|e| e.to_string())?;
        if symlink {
            std::os::unix::fs::symlink(&unrelated, &selected).map_err(|e| e.to_string())?;
        } else {
            std::fs::create_dir(&selected)
                .and_then(|_| std::fs::create_dir(selected.join(".git")))
                .map_err(|e| e.to_string())?;
        }
        let mut child = Command::new("sh")
            .args(["-c", &script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        // Feed the already-previewed bytes, just as the production typed request does.
        let _ = child
            .stdin
            .take()
            .ok_or("stdin")?
            .write_all(b"saved raw bytes");
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        assert!(!out.status.success());
        assert!(!unrelated.join("must-not-escape").exists());
        assert!(!selected.join("must-not-escape").exists());
        if symlink {
            std::fs::remove_file(&selected).map_err(|e| e.to_string())?;
        } else {
            std::fs::remove_dir_all(&selected).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&saved, &selected).map_err(|e| e.to_string())?;
    }
    Ok(())
}
