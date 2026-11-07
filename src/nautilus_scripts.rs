//! _History in gitcraft_ and _Blame in gitcraft_ in the Files (Nautilus) menu of files, on
//! Linux: two scripts in `~/.local/share/nautilus/scripts`, the menu's Scripts submenu, that
//! start gitcraft with `log <file>` / `blame -- <file>`. (Dolphin gets them from the service
//! menu the packages install, `packaging/linux/gitcraft-servicemenu.desktop`.)

use std::path::{Path, PathBuf};

/// The two scripts: (file name, which is the menu entry, and the command line's command).
const SCRIPTS: [(&str, &str); 2] = [
    ("History in gitcraft", "log"),
    ("Blame in gitcraft", "blame --"),
];

fn scripts_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|h| h.is_absolute())
                .map(|h| h.join(".local/share"))
        })
        .map(|d| d.join("nautilus/scripts"))
}

/// The program the scripts start: the AppImage, or this gitcraft. `None` off Linux.
pub fn program() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    crate::desktop_entry::appimage().or_else(|| std::env::current_exe().ok())
}

/// Whether both scripts are there.
pub fn installed() -> bool {
    scripts_dir().is_some_and(|d| SCRIPTS.iter().all(|(n, _)| d.join(n).is_file()))
}

/// Adds (or rewrites) the scripts, starting `program`.
pub fn install(program: &Path) -> Result<(), String> {
    write_all(&scripts_dir().ok_or("No home folder")?, program)
}

/// Removes the scripts.
pub fn uninstall() -> Result<(), String> {
    let dir = scripts_dir().ok_or("No home folder")?;
    for (name, _) in SCRIPTS {
        let path = dir.join(name);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(())
}

fn write_all(dir: &Path, program: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, command) in SCRIPTS {
        let path = dir.join(name);
        std::fs::write(&path, script(program, command))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Files runs a script in the folder shown, the files chosen as its arguments: gitcraft is
/// started for each (one running takes them all, see `crate::instance`).
fn script(program: &Path, command: &str) -> String {
    let program = crate::remote::sh_quote(&program.to_string_lossy());
    format!("#!/bin/sh\nfor f in \"$@\"; do\n\t{program} {command} \"$f\" &\ndone\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_starts_gitcraft_for_each_file() {
        assert_eq!(
            script(Path::new("/home/me/Apps/git craft's.AppImage"), "blame --"),
            "#!/bin/sh\nfor f in \"$@\"; do\n\t'/home/me/Apps/git craft'\\''s.AppImage' blame -- \
             \"$f\" &\ndone\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn writes_runnable_scripts_and_removes_them() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("gitcraft-nautilus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let written = write_all(&dir, Path::new("/usr/bin/gitcraft"));
        let mode = std::fs::metadata(dir.join("History in gitcraft"))
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0);
        let text = std::fs::read_to_string(dir.join("Blame in gitcraft")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        written?;
        assert_eq!(mode, 0o755);
        assert!(
            text.contains("'/usr/bin/gitcraft' blame -- \"$f\" &"),
            "{text}"
        );
        Ok(())
    }
}
