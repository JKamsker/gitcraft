//! Mountinfo catches same-device mounts before deletion; openat2 repeats the boundary
//! check atomically during traversal. Kernel octal escapes preserve non-UTF-8 paths.
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

pub(super) fn check(directory: &Path) -> Result<(), String> {
    let bytes = std::fs::read("/proc/self/mountinfo").map_err(|e| e.to_string())?;
    check_bytes(directory, &bytes)
}

fn check_bytes(directory: &Path, bytes: &[u8]) -> Result<(), String> {
    for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let encoded = line
            .split(|b| *b == b' ')
            .nth(4)
            .ok_or("Invalid mountinfo")?;
        let mut path = Vec::new();
        let mut i = 0;
        while i < encoded.len() {
            if encoded[i] == b'\\' {
                let digits = encoded.get(i + 1..i + 4).ok_or("Invalid mount escape")?;
                if !digits.iter().all(|d| (b'0'..=b'7').contains(d)) {
                    return Err("Invalid mount escape".into());
                }
                let value = u16::from(digits[0] - b'0') * 64
                    + u16::from(digits[1] - b'0') * 8
                    + u16::from(digits[2] - b'0');
                path.push(u8::try_from(value).map_err(|e| e.to_string())?);
                i += 4;
            } else {
                path.push(encoded[i]);
                i += 1;
            }
        }
        let mount = PathBuf::from(std::ffi::OsString::from_vec(path));
        if mount.starts_with(directory) {
            return Err(format!(
                "Purge refused mounted content: {}",
                mount.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn same_device_bind_mount_and_escaped_paths_are_refused() {
        let info =
            b"31 20 8:1 /source /recovery/tree-one/tree/bind\\040mount rw - ext4 /dev/x rw\n";
        assert!(super::check_bytes(std::path::Path::new("/recovery/tree-one"), info).is_err());
        assert!(super::check_bytes(std::path::Path::new("/recovery/tree-two"), info).is_ok());
        assert!(
            super::check_bytes(
                std::path::Path::new("/recovery/tree-one/tree/bind mount"),
                info
            )
            .is_err()
        );
    }
}
