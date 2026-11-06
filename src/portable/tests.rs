//! Where portable mode keeps gitcraft's files, and when it is on.

use super::*;

/// A fresh, empty folder for one test.
fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-portable-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[test]
fn off_without_the_marker_or_the_data_folder() -> Result<(), String> {
    let dir = scratch("off")?;
    // A file called `data` is not the folder.
    std::fs::write(dir.join(DATA), "").map_err(|e| e.to_string())?;
    let mode = detect(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(mode, Mode::Installed);
    Ok(())
}

#[test]
fn the_marker_makes_the_data_folder() -> Result<(), String> {
    let dir = scratch("marker")?;
    std::fs::write(dir.join(MARKER), "").map_err(|e| e.to_string())?;
    let mode = detect(&dir);
    let made = dir.join(DATA).is_dir();
    // The write test leaves nothing behind.
    let left = std::fs::read_dir(dir.join(DATA)).map_or(0, |d| d.count());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(mode, Mode::Portable(dir.join(DATA)));
    assert!(made);
    assert_eq!(left, 0);
    Ok(())
}

#[test]
fn a_data_folder_alone_is_enough_with_unicode_and_spaces() -> Result<(), String> {
    let dir = scratch("Stick ünï 💾")?;
    std::fs::create_dir_all(dir.join(DATA)).map_err(|e| e.to_string())?;
    let mode = detect(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(mode, Mode::Portable(dir.join(DATA)));
    Ok(())
}

/// A marker whose data folder can't be made (a file called `data` is in the way): said why,
/// and the usual place is used.
#[test]
fn an_unusable_folder_says_why() -> Result<(), String> {
    let dir = scratch("unusable")?;
    std::fs::write(dir.join(DATA), "").map_err(|e| e.to_string())?;
    std::fs::write(dir.join(MARKER), "").map_err(|e| e.to_string())?;
    let mode = detect(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    match mode {
        Mode::Unusable(why) => assert!(why.starts_with("Portable mode is off"), "{why}"),
        other => return Err(format!("{other:?}")),
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_read_only_data_folder_is_unusable() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("readonly")?;
    let data = dir.join(DATA);
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o555))
        .map_err(|e| e.to_string())?;
    // Root writes anywhere: then there is nothing to show.
    let writable = std::fs::write(data.join("probe"), "").is_ok();
    let mode = detect(&dir);
    let _ = std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755));
    let _ = std::fs::remove_dir_all(&dir);
    if writable {
        return Ok(());
    }
    match mode {
        Mode::Unusable(why) => {
            assert!(why.contains("Portable mode is off"), "{why}");
            assert!(why.contains(&data.display().to_string()), "{why}");
        }
        other => return Err(format!("{other:?}")),
    }
    Ok(())
}

#[test]
fn the_folder_gitcraft_runs_from() {
    assert_eq!(
        program_dir_of(Path::new("/stick/gitcraft/gitcraft"), None),
        Some(PathBuf::from("/stick/gitcraft"))
    );
    // Beside the app bundle, not inside it.
    assert_eq!(
        program_dir_of(
            Path::new("/Volumes/Stick/gitcraft.app/Contents/MacOS/gitcraft"),
            None
        ),
        Some(PathBuf::from("/Volumes/Stick"))
    );
    // Beside the AppImage, not in its mount.
    assert_eq!(
        program_dir_of(
            Path::new("/tmp/.mount_gitcrXYZ/usr/bin/gitcraft"),
            Some(Path::new("/media/me/stick/gitcraft.AppImage"))
        ),
        Some(PathBuf::from("/media/me/stick"))
    );
    // A bare name has no folder to look in.
    assert_eq!(program_dir_of(Path::new("gitcraft"), None), None);
}

#[test]
fn the_credential_store_is_optional_only_when_portable() {
    assert!(credential_store_allowed(false, false));
    assert!(credential_store_allowed(false, true));
    assert!(!credential_store_allowed(true, false));
    assert!(credential_store_allowed(true, true));
}
