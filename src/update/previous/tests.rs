//! Keeping the version an update replaced, swapping back to it, and the pin.

use super::*;

/// A fresh folder for one test, holding a "running" program file.
fn program(name: &str, file: &str, text: &str) -> Result<(PathBuf, PathBuf), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-previous-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(file);
    fs::write(&target, text).map_err(|e| e.to_string())?;
    Ok((dir, target))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn plain_move(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| e.to_string())
}

#[test]
fn an_update_keeps_the_version_it_replaces() -> Result<(), String> {
    let (dir, target) = program("keep", "gitcraft", "v0.3.3")?;
    let new = dir.join("parked");
    fs::write(&new, "v0.3.4").map_err(|e| e.to_string())?;
    replace_keeping(&target, &new, "0.3.3", plain_move)?;
    let (now, kept) = (read(&target), find(&target));
    let old_left = aside(&target).exists();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(now, "v0.3.4");
    let kept = kept.ok_or("nothing kept")?;
    assert_eq!(kept.version.as_deref(), Some("0.3.3"));
    assert_eq!(kept.label(), "0.3.3");
    assert!(!old_left);
    Ok(())
}

#[test]
fn going_back_swaps_the_two_and_can_go_forward_again() -> Result<(), String> {
    let (dir, target) = program("swap", "gitcraft.exe", "v0.3.3")?;
    let new = dir.join("parked.exe");
    fs::write(&new, "v0.3.4").map_err(|e| e.to_string())?;
    replace_keeping(&target, &new, "0.3.3", plain_move)?;
    let back = swap_back(&target, "0.3.4")?;
    let (after_back, kept) = (read(&target), find(&target));
    let forward = swap_back(&target, "0.3.3")?;
    let after_forward = read(&target);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(back.version.as_deref(), Some("0.3.3"));
    assert_eq!(after_back, "v0.3.3");
    assert_eq!(kept.and_then(|k| k.version).as_deref(), Some("0.3.4"));
    assert_eq!(forward.version.as_deref(), Some("0.3.4"));
    assert_eq!(after_forward, "v0.3.4");
    Ok(())
}

#[test]
fn a_failed_update_leaves_the_running_version_and_the_kept_one() -> Result<(), String> {
    let (dir, target) = program("failed", "gitcraft", "v0.3.4")?;
    fs::write(kept_path(&target), "v0.3.3").map_err(|e| e.to_string())?;
    fs::write(version_file(&target), "0.3.3").map_err(|e| e.to_string())?;
    let refused = replace_keeping(&target, &dir.join("missing"), "0.3.4", |_, _| {
        Err("disk full".to_owned())
    });
    let (now, kept) = (read(&target), find(&target));
    let _ = fs::remove_dir_all(&dir);
    let e = refused.err().ok_or("it went in")?;
    assert!(e.contains("disk full"), "{e}");
    assert_eq!(now, "v0.3.4");
    assert_eq!(kept.and_then(|k| k.version).as_deref(), Some("0.3.3"));
    Ok(())
}

#[test]
fn nothing_to_go_back_to() -> Result<(), String> {
    let (dir, target) = program("none", "gitcraft", "v0.3.4")?;
    let refused = swap_back(&target, "0.3.4");
    // An empty file isn't a version to go back to.
    fs::write(kept_path(&target), "").map_err(|e| e.to_string())?;
    let empty = find(&target);
    let now = read(&target);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        refused.err().as_deref(),
        Some("No previous version of gitcraft is kept")
    );
    assert_eq!(empty, None);
    assert_eq!(now, "v0.3.4");
    Ok(())
}

#[test]
fn an_unwritten_or_odd_version_is_left_unnamed() -> Result<(), String> {
    let (dir, target) = program("odd ünï", "gitcraft", "v0.3.4")?;
    fs::write(kept_path(&target), "v0.3.3").map_err(|e| e.to_string())?;
    let unwritten = find(&target).map(|p| p.label().to_owned());
    fs::write(version_file(&target), "../../etc\n").map_err(|e| e.to_string())?;
    let odd = find(&target).and_then(|p| p.version);
    fs::write(version_file(&target), "0.3.3-dev.12\n").map_err(|e| e.to_string())?;
    let dev = find(&target).and_then(|p| p.version);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(unwritten.as_deref(), Some("the version kept"));
    assert_eq!(odd, None);
    assert_eq!(dev.as_deref(), Some("0.3.3-dev.12"));
    Ok(())
}

#[test]
fn an_app_bundle_is_kept_and_swapped_whole() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-previous-bundle-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let make = |at: &Path, text: &str| -> Result<(), String> {
        let macos = at.join("Contents/MacOS");
        fs::create_dir_all(&macos).map_err(|e| e.to_string())?;
        fs::write(macos.join("gitcraft"), text).map_err(|e| e.to_string())
    };
    let target = dir.join("gitcraft.app");
    let new = dir.join("parked/gitcraft.app");
    make(&target, "old")?;
    make(&new, "new")?;
    replace_keeping(&target, &new, "1.0.0", plain_move)?;
    let program = |at: &Path| read(&at.join("Contents/MacOS/gitcraft"));
    let after_update = program(&target);
    swap_back(&target, "1.1.0")?;
    let after_back = program(&target);
    let kept = find(&target).and_then(|p| p.version);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(after_update, "new");
    assert_eq!(after_back, "old");
    assert_eq!(kept.as_deref(), Some("1.1.0"));
    Ok(())
}

#[test]
fn the_pin_holds_for_the_version_gone_back_to() {
    assert_eq!(settle_pin("", "0.3.3"), "");
    assert_eq!(settle_pin("0.3.3", "0.3.3"), "0.3.3");
    // Gone back to a version that wasn't named: the one that starts.
    assert_eq!(settle_pin(PIN_THIS, "0.3.3"), "0.3.3");
    // Another version was installed by hand since: no longer pinned.
    assert_eq!(settle_pin("0.3.3", "0.3.5"), "");
}

#[test]
fn a_newer_kept_version_is_going_forward() {
    let kept = |v: Option<&str>| Previous {
        version: v.map(str::to_owned),
        path: PathBuf::new(),
    };
    assert!(kept(Some("999.0.0")).is_newer());
    assert!(!kept(Some("0.0.1")).is_newer());
    assert!(!kept(Some(crate::update::CURRENT)).is_newer());
    assert!(!kept(None).is_newer());
}

/// When the running version can't be kept (its rename fails), the version file of the copy
/// kept before is gone too: no copy is ever offered under another version's name.
#[test]
fn a_kept_copy_that_stays_put_is_never_mislabelled() -> Result<(), String> {
    let (dir, target) = program("locked", "gitcraft", "v0.3.4")?;
    fs::write(version_file(&target), "0.3.2").map_err(|e| e.to_string())?;
    keep(&target, &dir.join("missing.old"), "0.3.3");
    let after = (find(&target), version_file(&target).exists());
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(after, (None, false));
    Ok(())
}
