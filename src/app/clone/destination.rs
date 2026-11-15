//! Destination I/O belongs to the clone worker, including slow or unavailable mounts.
use std::path::Path;
pub(super) fn check(dest: &Path) -> Result<(), String> {
    if crate::remote::is_remote(dest) {
        return Ok(());
    }
    match std::fs::metadata(dest) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Could not inspect {}: {error}", dest.display())),
        Ok(_) => {}
    }
    match std::fs::read_dir(dest) {
        Ok(mut entries) => {
            if entries.next().is_none() {
                Ok(())
            } else {
                Err(format!("{} already exists and isn't empty", dest.display()))
            }
        }
        Err(error) => Err(format!("Could not inspect {}: {error}", dest.display())),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clone_destination_inspection_rejects_occupied_without_modifying_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        assert!(check(folder.path()).is_ok());
        let file = folder.path().join("keep");
        std::fs::write(&file, "unchanged")?;
        assert!(check(folder.path()).is_err());
        assert_eq!(std::fs::read_to_string(file)?, "unchanged");
        assert!(check(&folder.path().join("new")).is_ok());
        Ok(())
    }
}
