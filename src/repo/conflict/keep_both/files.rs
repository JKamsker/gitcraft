use super::*;

pub(super) fn create_new(file: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    let Some(target) = crate::remote::parse(file) else {
        return create_new_here(file, bytes, executable);
    };
    let f = crate::remote::sh_path(&target.path);
    // `set -C`: the shell opens the file only if nothing is there (O_EXCL).
    let script = format!(
        "if [ -e {f} ] || [ -L {f} ]; then exit {EXISTS}; fi\nset -C\ncat > {f} || {{ rm -f -- {f}; exit 1; }}\n{}",
        if executable {
            format!("chmod +x {f} || {{ rm -f -- {f}; exit 1; }}\n")
        } else {
            String::new()
        }
    );
    let out = crate::remote::exec(
        &target.host,
        &crate::remote::Request {
            script,
            stdin: Some(bytes.to_vec()),
            ..crate::remote::Request::default()
        },
    )?;
    match out.code {
        0 => Ok(()),
        EXISTS => Err(crate::i18n::text(APPEARED).to_owned()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()),
    }
}

pub(super) fn create_new_here(file: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    let mut opened = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => crate::i18n::text(APPEARED).to_owned(),
            _ => e.to_string(),
        })?;
    let filled = fill(&mut opened, bytes, executable);
    if filled.is_err() {
        // Not left half written.
        drop(opened);
        let _ = std::fs::remove_file(file);
    }
    filled
}

/// Writes `bytes` to the file just created, made executable when asked.
pub(super) fn fill(
    opened: &mut std::fs::File,
    bytes: &[u8],
    executable: bool,
) -> Result<(), String> {
    use std::io::Write;
    opened.write_all(bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        let mode = opened
            .metadata()
            .map_err(|e| e.to_string())?
            .permissions()
            .mode();
        opened
            .set_permissions(std::fs::Permissions::from_mode(mode | 0o111))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}

/// Removes the file Keep both wrote at `file` (when the other could not be written).
pub(super) fn remove(file: &Path) -> Result<(), String> {
    let Some(target) = crate::remote::parse(file) else {
        return std::fs::remove_file(file).map_err(|e| e.to_string());
    };
    let out = crate::remote::exec(
        &target.host,
        &crate::remote::Request {
            script: format!("rm -f -- {}", crate::remote::sh_path(&target.path)),
            ..crate::remote::Request::default()
        },
    )?;
    if out.code == 0 {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

// Whether `plan.path` is still in the conflict `plan` was made for, both sides as they were
// (blocking: in the background).
