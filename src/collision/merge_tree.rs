//! What merging two branches would conflict on, from `git merge-tree --write-tree` (git 2.38 or
//! later): computed in git's object store, nothing checked out, no working tree touched.

use std::path::Path;

/// The files merging two branches leaves conflicted (none: it merges cleanly).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pair {
    pub files: Vec<String>,
}

impl Pair {
    pub fn clean(&self) -> bool {
        self.files.is_empty()
    }
}

/// What `git merge-tree --write-tree --name-only --no-messages -z` printed: the tree, then the
/// conflicted files, each ended by a NUL.
pub fn parse(out: &str) -> Pair {
    let mut parts = out.split('\0');
    let _tree = parts.next();
    let mut files: Vec<String> = Vec::new();
    for file in parts {
        // An empty part ends the list (the messages, none here, would follow).
        if file.is_empty() {
            break;
        }
        if !files.iter().any(|f| f == file) {
            files.push(file.to_owned());
        }
    }
    Pair { files }
}

/// Merges `a` and `b` in memory and says what conflicts.
pub fn simulate(repo: &Path, a: &str, b: &str) -> Result<Pair, String> {
    for rev in [a, b] {
        if rev.starts_with('-') {
            return Err(format!("{rev} is not a branch"));
        }
    }
    let args = [
        "-c",
        "core.quotePath=false",
        "merge-tree",
        "--write-tree",
        "--name-only",
        "--no-messages",
        "-z",
        a,
        b,
    ];
    // 1: it conflicts, and says where; or a revision isn't there (nothing printed then).
    let out =
        crate::git::run_ok_codes(Some(repo), &args, &[0, 1]).map_err(|e| explain(&e, a, b))?;
    if out.trim().is_empty() {
        return Err(format!(
            "Could not merge {a} with {b} in memory: one of them is not a branch or commit"
        ));
    }
    Ok(parse(&out))
}

/// What a failed `merge-tree` means, for the matrix's cell.
fn explain(err: &str, a: &str, b: &str) -> String {
    if err.contains("write-tree") && (err.contains("unknown option") || err.contains("usage")) {
        "The collision matrix needs git 2.38 or later (git merge-tree --write-tree)".to_owned()
    } else if err.contains("unrelated histories") {
        format!("{a} and {b} share no history")
    } else if err.trim().is_empty() {
        format!("Could not merge {a} with {b} in memory")
    } else {
        format!("Could not merge {a} with {b} in memory: {}", err.trim())
    }
}
