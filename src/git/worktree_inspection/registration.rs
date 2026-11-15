//! Registration identity without replacing Git's own path spelling.
use std::path::Path;

pub(in crate::git) fn same_registered_path(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        // Reuse recovery capture's existing-directory identity check first.
        crate::remote::same_dir(a, b)
            || windows_key(&resolved_ancestor(a).to_string_lossy())
                == windows_key(&resolved_ancestor(b).to_string_lossy())
    }
    #[cfg(not(windows))]
    crate::remote::same_dir(a, b)
}

/// Missing registrations still have existing parents: resolve short names there,
/// retaining the absent suffix rather than requiring the worktree to exist.
#[cfg(windows)]
fn resolved_ancestor(path: &Path) -> std::path::PathBuf {
    for ancestor in path.ancestors() {
        if let Ok(real) = ancestor.canonicalize()
            && let Ok(suffix) = path.strip_prefix(ancestor)
        {
            return real.join(suffix);
        }
    }
    path.to_owned()
}

#[cfg(any(windows, test))]
fn windows_key(path: &str) -> String {
    let path = path.replace('\\', "/").to_lowercase();
    let path = path.strip_prefix("//?/").unwrap_or(&path);
    let path = if let Some(unc) = path.strip_prefix("unc/") {
        format!("//{unc}")
    } else {
        path.to_owned()
    };
    path.trim_end_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_registration_keys_ignore_prefix_separators_case_and_trailing_slashes() {
        for (a, b) in [
            (r"\\?\C:\Users\Runner\work\", "c:/users/runner/work"),
            (
                r"C:\Users\RUNNER~1\Temp\tree",
                "c:/users/runner~1/temp/tree/",
            ),
            (r"\\?\UNC\Host\Share\tree\", "//host/share/tree"),
        ] {
            assert_eq!(super::windows_key(a), super::windows_key(b));
        }
        assert_ne!(
            super::windows_key("C:/work/a"),
            super::windows_key("C:/work/b")
        );
        assert_ne!(super::windows_key("C:/work"), super::windows_key("D:/work"));
    }
}
