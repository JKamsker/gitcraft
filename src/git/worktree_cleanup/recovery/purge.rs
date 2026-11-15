//! Purge owns open directory handles, never an unchecked recursive path. Both passes
//! enforce boundaries; the first refuses an existing mount before deleting anything.
#[cfg(target_os = "linux")]
mod mounts;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(unix)]
pub(super) use unix::Purge;
#[cfg(windows)]
pub(super) use windows::Purge;

fn boundary<T: Eq>(device: T, child_device: T, attributes: u32) -> Result<(), String> {
    if device != child_device || attributes & 0x400 != 0 {
        return Err("Purge refused a mount boundary or Windows reparse point".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn injectable_device_and_reparse_boundaries_fail_closed() {
        assert!(super::boundary(1, 1, 0).is_ok());
        assert!(super::boundary(1, 2, 0).is_err());
        for attributes in [0x400, 0x410, 0x420] {
            assert!(super::boundary(1, 1, attributes).is_err());
        }
    }
}
