use super::runtime::{profile_file, write_file};
use super::*;

#[test]
fn a_profile_config_in_the_portable_folder_is_read_by_git() -> Result<(), String> {
    let stick = std::env::temp_dir().join(format!(
        "gitcraft-portable-profile Stick é-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&stick);
    std::fs::create_dir_all(&stick).map_err(|e| e.to_string())?;
    std::fs::write(stick.join(crate::portable::MARKER), "").map_err(|e| e.to_string())?;
    let result = (|| {
        let crate::portable::Mode::Portable(data) = crate::portable::detect(&stick) else {
            return Err("portable mode wasn't detected".to_owned());
        };
        let profile = UserProfile {
            author_name: "Zoë Stick".to_owned(),
            author_email: "zoe@example.com".to_owned(),
            ..UserProfile::default()
        };
        let path = profile_file(&data, profile.id);
        write_file(&path, &config_text(&profile, &[]))?;
        let file = path.to_string_lossy().into_owned();
        let name = crate::git::run(None, &["config", "--file", &file, "user.name"])?;
        Ok((path.starts_with(&data), name))
    })();
    let _ = std::fs::remove_dir_all(&stick);
    let (inside, name) = result?;
    assert!(inside);
    assert_eq!(name.trim(), "Zoë Stick");
    Ok(())
}
