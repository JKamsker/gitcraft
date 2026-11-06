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
    // On macOS beside the app bundle, not inside it; elsewhere `.app` means nothing.
    let in_bundle = Path::new("/Volumes/Stick/gitcraft.app/Contents/MacOS/gitcraft");
    let expected = if cfg!(target_os = "macos") {
        "/Volumes/Stick"
    } else {
        "/Volumes/Stick/gitcraft.app/Contents/MacOS"
    };
    assert_eq!(
        program_dir_of(in_bundle, None),
        Some(PathBuf::from(expected))
    );
    assert_eq!(
        program_dir_of(Path::new("/home/me/tools.app/gitcraft/gitcraft"), None),
        Some(PathBuf::from("/home/me/tools.app/gitcraft"))
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

#[test]
fn the_bundle_a_program_is_in() {
    assert_eq!(
        bundle_of(Path::new("/A/gitcraft.app/Contents/MacOS/gitcraft")),
        Some(Path::new("/A/gitcraft.app"))
    );
    assert_eq!(bundle_of(Path::new("/A/x.app/gitcraft")), None);
    assert_eq!(
        bundle_of(Path::new("/A/x.app/Contents/Resources/gitcraft")),
        None
    );
    assert_eq!(bundle_of(Path::new("gitcraft")), None);
}

/// A `data` folder of the user's own, next to gitcraft in `~/Downloads`, say, isn't asked for
/// portable mode; one gitcraft already keeps its state in is.
#[test]
fn only_an_empty_data_folder_or_gitcraft_s_own_turns_it_on() -> Result<(), String> {
    let dir = scratch("unrelated")?;
    let data = dir.join(DATA);
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    std::fs::write(data.join("photos.zip"), "x").map_err(|e| e.to_string())?;
    let unrelated = detect(&dir);
    std::fs::write(data.join("app.ron"), "()").map_err(|e| e.to_string())?;
    let own = detect(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(unrelated, Mode::Installed);
    assert_eq!(own, Mode::Portable(data));
    Ok(())
}

/// macOS runs a quarantined download from a random read-only copy: said, with what to do.
#[test]
fn a_translocated_copy_says_what_to_do() -> Result<(), String> {
    let base = scratch("translocated")?;
    let dir = base.join("AppTranslocation").join("1234").join("d");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(MARKER), "").map_err(|e| e.to_string())?;
    let mode = detect(&dir);
    let made = dir.join(DATA).exists();
    let _ = std::fs::remove_dir_all(&base);
    match mode {
        Mode::Unusable(why) => assert!(why.contains("xattr -dr com.apple.quarantine"), "{why}"),
        other => return Err(format!("{other:?}")),
    }
    assert!(!made);
    Ok(())
}
