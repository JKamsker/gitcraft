//! What the native protocol refuses: named streams, links, junctions, missing files.
use super::native_fixtures::{junction, write};
use super::*;

#[test]
fn named_data_streams_are_refused_like_the_helper_refuses_them() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let path = temp.0.join("streams");
    write(&path, b"main")?;
    write(&temp.0.join("streams:extra"), b"named")?;
    let named = crate::i18n::text("interaction-standalone-helper-named-streams");
    assert!(snapshot(&path).is_err_and(|e| e.contains(named)));
    assert!(script_snapshot(&path).is_err_and(|e| e.contains(named)));
    let target = temp.0.join("target");
    write(&target, b"candidate")?;
    let before = snapshot(&target)?;
    assert!(prepare(&path, &target).is_err_and(|e| e.contains(named)));
    assert_eq!(snapshot(&target)?, before, "a refused copy changes nothing");
    let missing = snapshot(&temp.0.join("missing"));
    assert!(missing.is_err_and(|e| !e.contains("SeSecurityPrivilege")));
    Ok(())
}

#[test]
fn a_junction_or_link_is_refused_as_source_and_target_and_never_followed() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let folder = temp.0.join("folder");
    std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let link = temp.0.join("junction");
    junction(&link, &folder)?;
    let file = temp.0.join("file");
    write(&file, b"plain")?;
    super::native_fixtures::icacls(&file, &["/grant:r", "*S-1-1-0:(R)"])?;
    let reparse = crate::i18n::text("interaction-standalone-policy-reparse-point");
    let folder_before = snapshot(&folder)?;
    let file_before = snapshot(&file)?;
    assert!(snapshot(&link).is_err_and(|e| e.contains(reparse)));
    assert!(prepare(&link, &file).is_err_and(|e| e.contains(reparse)));
    assert!(prepare(&file, &link).is_err_and(|e| e.contains(reparse)));
    assert_eq!(
        snapshot(&folder)?,
        folder_before,
        "the junction's target is untouched"
    );
    assert_eq!(snapshot(&file)?, file_before);
    // A file symbolic link needs a privilege or Developer Mode; check it where available.
    let symlink = temp.0.join("symlink");
    if std::os::windows::fs::symlink_file(&file, &symlink).is_ok() {
        assert!(snapshot(&symlink).is_err_and(|e| e.contains(reparse)));
        let other = temp.0.join("other");
        write(&other, b"plain")?;
        assert!(prepare(&other, &symlink).is_err_and(|e| e.contains(reparse)));
        assert_eq!(
            snapshot(&file)?,
            file_before,
            "the link's target is untouched"
        );
    }
    Ok(())
}
