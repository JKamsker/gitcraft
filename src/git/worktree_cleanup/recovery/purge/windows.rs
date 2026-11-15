//! Hold the ancestry without write/delete sharing so path replacement cannot redirect
//! traversal. Reparse points (including junctions and volume mounts) are refused.
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub(in super::super) struct Purge {
    directory: PathBuf,
    ancestors: Vec<File>,
    entry: File,
}

fn open_dir(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        // Deny write sharing too: a concurrent handle cannot set a reparse point
        // in place on an empty held directory. Existing write handles cause refusal.
        .share_mode(0x1)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)
        .map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    super::boundary(0, 0, meta.file_attributes())?;
    if !meta.is_dir() {
        return Err("Recovery ancestor is not a directory".into());
    }
    Ok(file)
}

fn identity(file: &File) -> Result<same_file::Handle, String> {
    same_file::Handle::from_file(file.try_clone().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

impl Purge {
    pub fn open(root: &Path, directory: &Path) -> Result<Self, String> {
        if !root.is_absolute() || directory.parent() != Some(root) {
            return Err("Invalid recovery ancestry".into());
        }
        let mut ancestors = Vec::new();
        for path in root.ancestors().collect::<Vec<_>>().into_iter().rev() {
            ancestors.push(open_dir(path)?);
        }
        let entry = open_dir(directory)?;
        Ok(Self {
            directory: directory.to_owned(),
            ancestors,
            entry,
        })
    }

    pub fn remove(self, before: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        if identity(&self.entry)? != identity(&open_dir(&self.directory)?)? {
            return Err("Recovery entry identity changed".into());
        }
        walk(&self.directory, false, true, 0)?;
        if identity(&self.entry)? != identity(&open_dir(&self.directory)?)? {
            return Err("Recovery entry identity changed".into());
        }
        before()?;
        walk(&self.directory, true, true, 0)?;
        std::fs::remove_file(self.directory.join("record.json")).map_err(|e| e.to_string())?;
        drop(self.entry);
        // After releasing the entry lock, remove only an empty directory. A replacement
        // cannot induce recursive deletion; all its ancestors remain locked until return.
        let result = std::fs::remove_dir(&self.directory).map_err(|e| e.to_string());
        drop(self.ancestors);
        result
    }
}

fn walk(path: &Path, delete: bool, top: bool, depth: usize) -> Result<(), String> {
    if depth > 256 {
        return Err("Purge nesting limit exceeded".into());
    }
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if top && entry.file_name() == "record.json" {
            continue;
        }
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        super::boundary(0, 0, meta.file_attributes())?;
        if meta.is_dir() {
            let held = open_dir(&path)?;
            walk(&path, delete, false, depth + 1)?;
            drop(held);
            if delete {
                std::fs::remove_dir(&path).map_err(|e| e.to_string())?;
            }
        } else if delete {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
