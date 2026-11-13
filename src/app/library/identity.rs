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

/// The folder whose children are scanned as `repo`'s nearby repositories (its siblings):
/// its parent, unless that is too broad to look in uninvited. Never the file system or a
/// drive's root, a top-level folder (`/Users`, `/home`, `C:\Users`), the home folder or one
/// above it, the temporary folder, nor a place on another machine.
pub(in crate::app) fn nearby_parent(
    repo: &Path,
    home: Option<&Path>,
    temp: Option<&Path>,
) -> Option<PathBuf> {
    if is_far(repo) || !repo.is_absolute() {
        return None;
    }
    let parent = repo.parent()?;
    let named = parent
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if named < 2 && !shallow_ok(parent, home) {
        return None;
    }
    let key = PathKey::of(parent);
    let at_or_above = |other: &Path| {
        let other = PathKey::of(other);
        other == key
            || other
                .0
                .starts_with(&format!("{}{}", key.0, std::path::MAIN_SEPARATOR))
    };
    if home.is_some_and(at_or_above) || temp.is_some_and(|t| PathKey::of(t) == key) {
        return None;
    }
    Some(parent.to_owned())
}

/// A folder right under a root (`/work`, `C:\src`) is looked in only when it isn't one the
/// system keeps users, programs or mounted drives in.
fn shallow_ok(parent: &Path, home: Option<&Path>) -> bool {
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
    let holds_home = home
        .and_then(Path::parent)
        .is_some_and(|h| PathKey::of(h) == PathKey::of(parent));
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
        let home = p("/home/me");
        let near = |s: &str| nearby_parent(Path::new(s), Some(&home), Some(Path::new("/tmp")));
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
        let home = p(r"C:\Users\me");
        let near = |s: &str| nearby_parent(Path::new(s), Some(&home), None);
        assert_eq!(near(r"C:\src\app"), Some(p(r"C:\src")));
        assert_eq!(near(r"D:\work\projects\app"), Some(p(r"D:\work\projects")));
        assert_eq!(near(r"C:\app"), None);
        assert_eq!(near(r"C:\Users\me\app"), None);
        assert_eq!(near(r"c:\users\ME\app"), None);
        assert_eq!(near(r"C:\Users\app"), None);
        assert_eq!(near(r"\\server\share\x\app"), None);
    }
}
