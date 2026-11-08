use crate::portability::{FILE_LIMIT, Setup};
use std::{
    io::{Read, Write},
    path::Path,
};
pub(super) fn load(path: &Path, active: &dyn Fn() -> bool) -> Result<Setup, String> {
    if !active() {
        return Err("Import cancelled".into());
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > FILE_LIMIT as u64
    {
        return Err("Choose a regular setup file of at most 8 MiB".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(FILE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if !active() {
        return Err("Import cancelled".into());
    }
    Setup::decode(&bytes)
}
pub(super) fn save(path: &Path, file: &Setup, active: &dyn Fn() -> bool) -> Result<(), String> {
    let bytes = file.encode()?;
    if !active() {
        return Err("Export cancelled".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options
        .open(path)
        .map_err(|e| format!("Choose a new unused setup filename: {e}"))?;
    out.write_all(&bytes)
        .and_then(|_| out.sync_all())
        .map_err(|e| format!("Writing setup failed; inspect the partial file: {e}"))
}
