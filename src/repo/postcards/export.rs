//! Create-only publication in a held local directory. Never follows a replaced parent or overwrites a name.
use std::io::Write;
#[cfg(not(unix))]
use std::path::PathBuf;
use std::path::{Component, Path};
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(crate) struct Destination {
    held: Vec<std::fs::File>,
    #[cfg(not(unix))]
    parent: PathBuf,
    name: String,
}
impl Destination {
    pub fn capture(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("Choose an absolute local destination".into());
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Destination filename is not UTF-8")?
            .to_owned();
        let folded = name.to_lowercase();
        let stem = folded.split('.').next().unwrap_or_default();
        if name.is_empty() || name.len() > 255 || name.ends_with(['.',' '])
            || name.chars().any(|c|c.is_control() || matches!(c,'\\'|'/'|':'|'<'|'>'|'"'|'|'|'?'|'*'|'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}'))
            || matches!(stem,"con"|"prn"|"aux"|"nul")
            || (stem.len()==4 && (stem.starts_with("com")||stem.starts_with("lpt")) && matches!(stem.as_bytes()[3],b'1'..=b'9'))
        {
            return Err("Invalid export filename".into());
        }
        let parent = path.parent().ok_or("Missing export folder")?.to_path_buf();
        let held = hold(&parent)?;
        Ok(Self {
            held,
            #[cfg(not(unix))]
            parent,
            name,
        })
    }
    pub fn publish(&self, bytes: &[u8], active: &dyn Fn() -> bool) -> Result<(), String> {
        let _held = &self.held;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("Export exceeds 64 MiB".into());
        }
        if !active() {
            return Err("Export cancelled".into());
        }
        // The final name is opened exclusively relative to the held parent. A failure can
        // leave only this newly created file incomplete; cleanup never removes a raced name.
        #[cfg(unix)]
        let mut file = {
            use rustix::fs::{Mode, OFlags, openat};
            let parent = self.held.last().ok_or("Missing export folder")?;
            std::fs::File::from(
                openat(
                    parent,
                    self.name.as_str(),
                    OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::WRONLY
                        | OFlags::CLOEXEC
                        | OFlags::NOFOLLOW,
                    Mode::from_bits_truncate(0o600),
                )
                .map_err(|e| {
                    format!("Could not create PNG; existing destinations are preserved: {e}")
                })?,
            )
        };
        #[cfg(not(unix))]
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.parent.join(&self.name))
            .map_err(|e| {
                format!("Could not create PNG; existing destinations are preserved: {e}")
            })?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| {
                format!("The new PNG may be incomplete; existing files were preserved: {e}")
            })
    }
}
#[cfg(unix)]
fn hold(path: &Path) -> Result<Vec<std::fs::File>, String> {
    use rustix::fs::{Mode, OFlags, open, openat};
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW;
    let mut held = vec![std::fs::File::from(
        open("/", flags, Mode::empty()).map_err(|e| e.to_string())?,
    )];
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                let parent = held.last().ok_or("Missing held export parent")?;
                held.push(std::fs::File::from(
                    openat(parent, name, flags, Mode::empty())
                        .map_err(|e| format!("Export folder is unavailable or linked: {e}"))?,
                ));
            }
            _ => return Err("Export folder must have a physical path without traversal".into()),
        }
    }
    Ok(held)
}
#[cfg(windows)]
fn hold(path: &Path) -> Result<Vec<std::fs::File>, String> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("Export folder has traversal".into());
    }
    let mut paths: Vec<_> = path.ancestors().filter(|p| p.has_root()).collect();
    paths.reverse();
    let mut held = Vec::new();
    for ancestor in paths {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(ancestor)
            .map_err(|e| e.to_string())?;
        let m = file.metadata().map_err(|e| e.to_string())?;
        if !m.is_dir() || m.file_attributes() & 0x400 != 0 {
            return Err("Export folder is linked or unavailable".into());
        }
        held.push(file);
    }
    Ok(held)
}
#[cfg(not(any(unix, windows)))]
fn hold(_: &Path) -> Result<Vec<std::fs::File>, String> {
    Err("Held-directory export is unavailable on this platform".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_never_overwrites_and_cancellation_leaves_no_output() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "gitcraft-postcard-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
        let result = (|| {
            let destination = Destination::capture(&root.join("release.png"))?;
            destination.publish(b"first", &|| true)?;
            assert!(destination.publish(b"second", &|| true).is_err());
            assert_eq!(
                std::fs::read(root.join("release.png")).map_err(|e| e.to_string())?,
                b"first"
            );
            assert!(
                Destination::capture(&root.join("cancelled.png"))?
                    .publish(b"cancelled", &|| false)
                    .is_err()
            );
            assert!(!root.join("cancelled.png").exists());
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(root);
        result
    }
    #[cfg(unix)]
    #[test]
    fn swapped_parent_cannot_redirect_publication() -> Result<(), String> {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!(
            "gitcraft-postcard-swap-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("chosen"))
            .and_then(|_| std::fs::create_dir(root.join("outside")))
            .map_err(|e| e.to_string())?;
        let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
        let result = (|| {
            let destination = Destination::capture(&root.join("chosen/release.png"))?;
            std::fs::rename(root.join("chosen"), root.join("held"))
                .and_then(|_| symlink(root.join("outside"), root.join("chosen")))
                .map_err(|e| e.to_string())?;
            destination.publish(b"card", &|| true)?;
            assert!(!root.join("outside/release.png").exists());
            assert_eq!(
                std::fs::read(root.join("held/release.png")).map_err(|e| e.to_string())?,
                b"card"
            );
            assert!(Destination::capture(&root.join("chosen/another.png")).is_err());
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(root);
        result
    }
}
