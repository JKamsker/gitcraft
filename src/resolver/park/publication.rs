//! Copy raw bytes beside the destination, then atomically publish on its filesystem.
use super::*;

pub(super) fn publish(
    path: &Path,
    bytes: &[u8],
    source: &Path,
    evidence: (&Path, &str),
    validate: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let parent = path.parent().ok_or("Missing parent")?;
    // Reserve a fresh owned directory. Existing files/symlinks are never adopted.
    let mut n = 0u64;
    let staging = loop {
        let staging = parent.join(format!(".gitcraft-restore-{n}"));
        let builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        match builder.create(&staging) {
            Ok(()) => break staging,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e.to_string()),
        }
    };
    let temp = staging.join("bytes");
    durability::copy(source, &temp, evidence.0, evidence.1)?;
    if std::fs::read(&temp).map_err(|e| e.to_string())? != bytes {
        return Err("Recovery source bytes changed during publication".into());
    }
    journal::sync_dir(&staging)?;
    journal::sync_dir(parent)?;
    effects::boundary("destination-temp-durable")?;
    validate()?;
    std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
    effects::boundary("destination-published")?;
    journal::sync_dir(parent)?;
    journal::sync_dir(&staging)?;
    std::fs::remove_dir(&staging).map_err(|e| e.to_string())?;
    journal::sync_dir(parent)
}
