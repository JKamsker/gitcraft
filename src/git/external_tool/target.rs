//! What a diff tool is asked to show, as the arguments `git difftool` (and `git diff`) take.

/// The two sides of one file a diff tool shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffTarget {
    /// The working tree against what is staged.
    Unstaged { path: String },
    /// What is staged against the last commit; `old_path`: where a rename comes from.
    Staged {
        path: String,
        old_path: Option<String>,
    },
    /// A file from one commit to another (`to`), or to the working tree (`to: None`).
    Revisions {
        from: String,
        to: Option<String>,
        path: String,
        old_path: Option<String>,
    },
}

impl DiffTarget {
    /// The file shown.
    pub fn path(&self) -> &str {
        match self {
            DiffTarget::Unstaged { path }
            | DiffTarget::Staged { path, .. }
            | DiffTarget::Revisions { path, .. } => path,
        }
    }

    /// What follows `git difftool` / `git diff` to compare just this file. Renames are followed
    /// (`-M`, both names given), so a renamed file shows against what it was.
    pub fn args(&self) -> Result<Vec<String>, String> {
        let mut args = Vec::new();
        let (path, old_path) = match self {
            DiffTarget::Unstaged { path } => (path, None),
            DiffTarget::Staged { path, old_path } => {
                args.extend(["--cached".to_owned(), "-M".to_owned()]);
                (path, old_path.as_ref())
            }
            DiffTarget::Revisions {
                from,
                to,
                path,
                old_path,
            } => {
                args.push("-M".to_owned());
                for rev in std::iter::once(from).chain(to) {
                    // `difftool` passes revisions on to `diff`; one like an option would be read
                    // as one.
                    if rev.is_empty() || rev.starts_with('-') {
                        return Err(crate::i18n::message(
                            "backend-rev-isn-t-a-revision-to-compare-3dd7e7",
                            &[("rev", format!("{:?}", rev).into())],
                        ));
                    }
                    args.push(rev.clone());
                }
                (path, old_path.as_ref())
            }
        };
        args.push("--".to_owned());
        args.push(path.clone());
        args.extend(old_path.filter(|old| *old != path).cloned());
        Ok(args)
    }
}
