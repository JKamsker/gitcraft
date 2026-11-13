//! A discovered repository's identity (one row per real checkout), and which folders are fit
//! to be scanned as a repository's _nearby_ folder.

use std::path::{Component, Path, PathBuf};

/// What two paths of the same checkout have in common: the canonical path the worker made
/// (see `repo_scan::top_folder`), compared without trailing separators and, where the file
/// system ignores case (Windows, macOS), without case.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::app) struct PathKey(String);

impl PathKey {
    pub fn of(path: &Path) -> Self {
        Self::with(path, cfg!(windows), cfg!(any(windows, target_os = "macos")))
    }

    /// [`Self::of`] with the platform's rules given: `windows` separators, `fold` case.
    pub fn with(path: &Path, windows: bool, fold: bool) -> Self {
        let mut s = path.to_string_lossy().into_owned();
        if windows {
            s = s.replace('/', "\\");
            if let Some(rest) = s.strip_prefix(r"\\?\")
                && !rest.starts_with("UNC")
            {
                s = rest.to_owned();
            }
        }
        let sep = if windows { '\\' } else { '/' };
        while s.len() > 1 && s.ends_with(sep) && !s.ends_with(&format!(":{sep}")) {
            s.pop();
        }
        if fold {
            s = s.to_lowercase();
        }
        Self(s)
    }
}

/// Whether `path` names a place on another machine, a share or WSL: never looked in
/// without being asked.
pub(in crate::app) fn is_far(path: &Path) -> bool {
    if crate::remote::is_remote(path) {
        return true;
    }
    let s = path.to_string_lossy();
    let lower = s.to_lowercase().replace('/', "\\");
    s.starts_with(r"\\")
        || s.starts_with("//")
        || lower.starts_with(r"\\?\unc\")
        || lower.contains(r"\wsl$\")
        || lower.contains(r"\wsl.localhost\")
}

/// Whether `inner` is `outer` or a folder in it (by [`PathKey`]).
fn at_or_in(inner: &Path, outer: &Path) -> bool {
    let (inner, outer) = (PathKey::of(inner), PathKey::of(outer));
    let sep = std::path::MAIN_SEPARATOR;
    inner == outer
        || inner.0.starts_with(&outer.0)
            && (outer.0.ends_with(sep) || inner.0[outer.0.len()..].starts_with(sep))
}

/// The folder whose children are scanned as `repo`'s nearby repositories (its siblings):
/// its parent, unless that is too broad to look in uninvited. Never the file system or a
/// drive's root, a top-level folder (`/Users`, `/home`, `C:\Users`), a home folder or one
/// above it, a temporary folder or one in it (practice repositories, test checkouts), where
/// drives are mounted (`/Volumes/<drive>`, `/mnt/<drive>`, `/media/<user>/<drive>`), nor a
/// place on another machine. `homes` and `temps` are given as written and canonical (as
/// `repo` is); nothing here touches the file system.
pub(in crate::app) fn nearby_parent(
    repo: &Path,
    homes: &[PathBuf],
    temps: &[PathBuf],
) -> Option<PathBuf> {
    if is_far(repo) || !repo.is_absolute() || repo.to_str().is_none() {
        return None;
    }
    let parent = repo.parent()?;
    let named = parent
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if named < 2 && !shallow_ok(parent, homes) || is_mount_root(parent) {
        return None;
    }
    if homes.iter().any(|h| at_or_in(h, parent)) || temps.iter().any(|t| at_or_in(parent, t)) {
        return None;
    }
    Some(parent.to_owned())
}

/// A drive mounted where the system mounts them: `/Volumes/<x>`, `/mnt/<x>`,
/// `/media/<x>`, `/media/<user>/<x>`, `/run/media/<user>/<x>`.
fn is_mount_root(dir: &Path) -> bool {
    let mut parts = dir.components();
    if parts.next() != Some(Component::RootDir) {
        return false;
    }
    let names: Vec<String> = parts
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    matches!(
        names.as_slice(),
        ["volumes" | "mnt" | "media", _] | ["media", _, _] | ["run", "media", _] | ["run", "media", _, _]
    )
}

/// A folder right under a root (`/work`, `C:\src`) is looked in only when it isn't one the
/// system keeps users, programs or mounted drives in.
fn shallow_ok(parent: &Path, homes: &[PathBuf]) -> bool {
    const BROAD: [&str; 18] = [
        "users",
        "home",
        "root",
        "tmp",
        "var",
        "etc",
        "opt",
        "usr",
        "mnt",
        "media",
        "volumes",
        "private",
        "system",
        "library",
        "applications",
        "windows",
        "program files",
        "program files (x86)",
    ];
    let Some(name) = parent.file_name() else {
        return false;
    };
    let name = name.to_string_lossy().to_lowercase();
    // The folder home is in (`/Users`, `/home`) is broad whatever it is called.
    let holds_home = homes
        .iter()
        .filter_map(|h| h.parent())
        .any(|h| PathKey::of(h) == PathKey::of(parent));
    !BROAD.contains(&name.as_str()) && !holds_home
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn keys_tell_the_same_checkout_apart_from_others() {
        let unix = |s: &str| PathKey::with(Path::new(s), false, false);
        let mac = |s: &str| PathKey::with(Path::new(s), false, true);
        let win = |s: &str| PathKey::with(Path::new(s), true, true);
        assert_eq!(unix("/work/app/"), unix("/work/app"));
        assert_ne!(unix("/work/App"), unix("/work/app"));
        assert_eq!(mac("/Work/App"), mac("/work/app"));
        assert_eq!(unix("/"), unix("/"));
        assert_eq!(win(r"C:\Src\App\"), win("c:/src/app"));
        assert_eq!(win(r"\\?\C:\src\app"), win(r"C:\src\app"));
        assert_ne!(win(r"\\?\UNC\box\share"), win(r"C:\box\share"));
        assert_eq!(win(r"C:\"), PathKey(r"c:\".into()));
        assert_eq!(
            unix("/my projects/Größe 日本"),
            unix("/my projects/Größe 日本/")
        );
        assert_eq!(mac("/x/GRÖSSE"), mac("/x/grösse"));
    }

    #[cfg(unix)]
    #[test]
    fn the_parent_of_an_opened_repository_is_looked_in_when_it_is_narrow() {
        let (homes, temps) = ([p("/home/me")], [p("/tmp"), p("/private/var/folders/x/T")]);
        let near = |s: &str| nearby_parent(Path::new(s), &homes, &temps);
        assert_eq!(near("/work/projects/app"), Some(p("/work/projects")));
        assert_eq!(near("/home/me/code/app"), Some(p("/home/me/code")));
        assert_eq!(near("/work/app"), Some(p("/work")));
        // Home itself, above it, the root, top-level system folders, temp.
        assert_eq!(near("/home/me/app"), None);
        assert_eq!(near("/home/app"), None);
        assert_eq!(near("/app"), None);
        assert_eq!(near("/"), None);
        assert_eq!(near("/mnt/app"), None);
        assert_eq!(near("/Volumes/app"), None);
        assert_eq!(near("/tmp/app"), None);
        // In a temporary folder, however deep (a practice repository), but not one only
        // named like it.
        assert_eq!(near("/tmp/gitcraft-practice-1/repository"), None);
        assert_eq!(near("/private/var/folders/x/T/gitcraft-practice-2/repository"), None);
        assert_eq!(near("/tmpfiles/work/app"), Some(p("/tmpfiles/work")));
        // Where drives are mounted: their own folders.
        for mounted in [
            "/Volumes/Backup/app",
            "/mnt/data/app",
            "/media/usb/app",
            "/media/me/usb/app",
            "/run/media/me/usb/app",
        ] {
            assert_eq!(near(mounted), None, "{mounted}");
        }
        assert_eq!(near("/Volumes/Backup/code/app"), Some(p("/Volumes/Backup/code")));
        assert_eq!(near("/run/media/me/usb/code/app"), Some(p("/run/media/me/usb/code")));
        assert_eq!(near("relative/app"), None);
        // Another machine, a share, WSL.
        assert_eq!(near("ssh://box/work/projects/app"), None);
        assert_eq!(near("//server/share/projects/app"), None);
        assert!(is_far(Path::new(r"\\wsl$\Ubuntu\home\me\app")));
        assert!(is_far(Path::new(r"\\wsl.localhost\Ubuntu\x")));
        assert!(is_far(Path::new(r"\\?\UNC\server\share\x")));
        assert!(!is_far(Path::new("/work/app")));
        // Unicode and spaces are fine.
        assert_eq!(near("/my work/Größe/日本"), Some(p("/my work/Größe")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_drive_roots_and_users_are_not_looked_in() {
        let homes = [p(r"C:\Users\me")];
        let temps = [p(r"C:\Users\me\AppData\Local\Temp")];
        let near = |s: &str| nearby_parent(Path::new(s), &homes, &temps);
        assert_eq!(near(r"C:\src\app"), Some(p(r"C:\src")));
        assert_eq!(near(r"D:\work\projects\app"), Some(p(r"D:\work\projects")));
        assert_eq!(near(r"C:\app"), None);
        assert_eq!(near(r"C:\Users\me\app"), None);
        assert_eq!(near(r"c:\users\ME\app"), None);
        assert_eq!(near(r"C:\Users\app"), None);
        assert_eq!(near(r"\\server\share\x\app"), None);
        assert_eq!(
            near(r"C:\Users\me\AppData\Local\Temp\gitcraft-practice-1\repository"),
            None
        );
    }
}
