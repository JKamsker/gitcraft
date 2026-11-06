//! Reading the patch the checks look at, through the git command layer (so a repository in
//! WSL or over SSH is read there).

use std::path::Path;

use super::{FileFlags, scan_patch};

/// What a list of changed files is of.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    /// A commit, against its first parent.
    Commit(String),
    /// From one commit to another, or to the working tree (`to: None`).
    Range { from: String, to: Option<String> },
    /// Everything uncommitted against HEAD, from the read of the working tree numbered so.
    Work(u64),
}

/// The patch as git prints it the same way whatever the user's config says.
const PATCH_ARGS: [&str; 5] = [
    "--no-color",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "-M",
];

/// The arguments that print `scope`'s patch.
fn args(scope: &Scope) -> Vec<&str> {
    // Unicode names as they are, not as `\303\251` (both are read, this is shorter).
    let mut args = vec!["-c", "core.quotePath=false"];
    match scope {
        Scope::Commit(hash) => {
            args.extend([
                "show",
                "--no-show-signature",
                "--format=",
                "--diff-merges=first-parent",
                "--root",
            ]);
            args.extend(PATCH_ARGS);
            args.extend(["--end-of-options", hash.as_str()]);
        }
        Scope::Range { from, to } => {
            args.push("diff");
            args.extend(PATCH_ARGS);
            args.extend(["--end-of-options", from.as_str()]);
            args.extend(to.as_deref());
        }
        Scope::Work(_) => {
            args.push("diff");
            args.extend(PATCH_ARGS);
            args.extend(["--end-of-options", "HEAD"]);
        }
    }
    args.push("--");
    args
}

/// The files of `scope` that weaken the checks.
pub fn scan(repo: &Path, scope: &Scope) -> Result<Vec<FileFlags>, String> {
    let text = crate::git::run(Some(repo), &args(scope))
        .map_err(|e| format!("Could not read the changes to look for weakened checks: {e}"))?;
    Ok(scan_patch(&text))
}

#[cfg(test)]
mod tests {
    use super::{Scope, args};

    #[test]
    fn revisions_come_after_the_end_of_options() {
        let scope = Scope::Range {
            from: "--output=x".into(),
            to: None,
        };
        let a = args(&scope);
        let end = a.iter().position(|x| *x == "--end-of-options");
        let rev = a.iter().position(|x| *x == "--output=x");
        assert!(end < rev);
        assert_eq!(a.last(), Some(&"--"));
    }
}
