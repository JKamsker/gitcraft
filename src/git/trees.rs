//! Commits made without touching the working tree or the index: trees of patches applied in a
//! scratch index, and commits of those trees (`commit-tree`). Rewrites (splitting a commit,
//! moving changes between commits, absorbing staged fixes) prepare their commits this way, then
//! a rebase replays them, so that nothing on disk changes until everything has been checked.

use std::path::Path;

use super::{END_OPTS, NO_SIGNATURE, UTF8, run, run_env};

/// Who wrote a commit, and when, as git takes them back (`GIT_AUTHOR_*`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
    /// `<seconds> <zone>`, git's raw date.
    pub date: String,
}

impl Author {
    fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("GIT_AUTHOR_NAME", self.name.clone()),
            ("GIT_AUTHOR_EMAIL", self.email.clone()),
            ("GIT_AUTHOR_DATE", self.date.clone()),
        ]
    }
}

/// The author of `commit`.
pub fn author_of(repo: &Path, commit: &str) -> Result<Author, String> {
    let out = run(
        Some(repo),
        &[
            "log",
            "-1",
            NO_SIGNATURE,
            UTF8,
            "--date=raw",
            "--format=%an%x00%ae%x00%ad",
            END_OPTS,
            commit,
        ],
    )?;
    let mut fields = out.trim_end_matches('\n').split('\0');
    let mut next = || fields.next().unwrap_or_default().to_owned();
    let author = Author {
        name: next(),
        email: next(),
        date: next(),
    };
    if author.date.is_empty() {
        return Err(format!("could not read who wrote {commit}"));
    }
    Ok(author)
}

/// The tree of `rev`.
pub fn tree_of(repo: &Path, rev: &str) -> Result<String, String> {
    let spec = format!("{rev}^{{tree}}");
    Ok(run(
        Some(repo),
        &["rev-parse", "--verify", "--quiet", END_OPTS, &spec],
    )
    .map_err(|_| format!("{rev} has no tree"))?
    .trim()
    .to_owned())
}

/// What `git apply` reads back exactly: every file whole (`--binary`, full blob names), no
/// renames (a rename picked in part is a removal and an addition), whatever the user's config
/// says about colours, prefixes, external diff programs or text conversion.
const PATCH: [&str; 9] = [
    "--binary",
    "--full-index",
    "--no-renames",
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--ignore-submodules=none",
];

/// The patch that turns `from` (a commit or a tree) into `to`, every file of it.
pub fn patch_between(repo: &Path, from: &str, to: &str) -> Result<String, String> {
    let mut args = vec!["diff"];
    args.extend(PATCH);
    args.extend([END_OPTS, from, to, "--"]);
    run(Some(repo), &args)
}

/// The paths [`patch_between`] has, in its order.
pub fn paths_between(repo: &Path, from: &str, to: &str) -> Result<Vec<String>, String> {
    let out = run(
        Some(repo),
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--ignore-submodules=none",
            END_OPTS,
            from,
            to,
            "--",
        ],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The tree `base` becomes with the patches (files where git runs, see `save_patch_for`)
/// applied one after the other, in a scratch index: the working tree and the index are left
/// alone. Fails, saying which, when one doesn't apply.
pub fn tree_after(repo: &Path, base: &str, patches: &[String]) -> Result<String, String> {
    let scratch = format!("{}.index", super::save_patch_for(repo, "")?);
    let env = [("GIT_INDEX_FILE", scratch.clone())];
    let result = (|| {
        run_env(repo, &env, &["read-tree", END_OPTS, base])?;
        for (n, patch) in patches.iter().enumerate() {
            run_env(
                repo,
                &env,
                &["apply", "--cached", "--whitespace=nowarn", patch],
            )
            .map_err(|e| {
                if patches.len() == 1 {
                    format!("its patch does not apply: {e}")
                } else {
                    format!("patch {} does not apply: {e}", n + 1)
                }
            })?;
        }
        Ok(run_env(repo, &env, &["write-tree"])?.trim().to_owned())
    })();
    match crate::remote::parse(repo) {
        None => {
            let _ = std::fs::remove_file(&scratch);
        }
        // Where git ran: an index is as big as the repository's file list.
        Some(target) => {
            let _ = crate::remote::exec(
                &target.host,
                &crate::remote::Request {
                    script: format!("rm -f {}", crate::remote::sh_quote(&scratch)),
                    ..crate::remote::Request::default()
                },
            );
        }
    }
    result
}

/// Makes a commit of `tree` on `parent` with `message`, by `author` (or the user when `None`),
/// and returns it. It is on no branch: a rebase picks it.
pub fn commit_tree(
    repo: &Path,
    tree: &str,
    parent: &str,
    message: &str,
    author: Option<&Author>,
) -> Result<String, String> {
    let file = super::save_patch_for(repo, message)?;
    let env = author.map(Author::env).unwrap_or_default();
    Ok(run_env(
        repo,
        &env,
        &["commit-tree", tree, "-p", parent, "-F", &file],
    )?
    .trim()
    .to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-trees-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Zoë Ünal"],
            &["config", "user.email", "z@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            assert!(run(Some(&dir), args).is_ok());
        }
        dir
    }

    #[test]
    fn a_tree_from_a_patch_leaves_the_index_and_files_alone() -> Result<(), String> {
        let dir = repo("apply");
        let write = |p: &str, t: &str| std::fs::write(dir.join(p), t).map_err(|e| e.to_string());
        write("a b.txt", "1\n2\n3\n")?;
        run(Some(&dir), &["add", "-A"])?;
        run(Some(&dir), &["commit", "-q", "-m", "one"])?;
        write("a b.txt", "1\nzwei\n3\n")?;
        run(Some(&dir), &["commit", "-q", "-am", "two"])?;
        let patch = patch_between(&dir, "HEAD~1", "HEAD")?;
        let file = crate::git::save_patch_for(&dir, &patch)?;
        let tree = tree_after(&dir, "HEAD~1", std::slice::from_ref(&file))?;
        assert_eq!(tree, tree_of(&dir, "HEAD")?);
        // Applied on top of what it makes, it no longer fits.
        assert!(tree_after(&dir, "HEAD", &[file]).is_err());
        let author = author_of(&dir, "HEAD")?;
        assert_eq!(author.name, "Zoë Ünal");
        let made = commit_tree(
            &dir,
            &tree,
            "HEAD~1",
            "made\n\nbody ünïcode\n",
            Some(&author),
        )?;
        let message = run(Some(&dir), &["log", "-1", "--format=%an|%B", &made])?;
        let status = run(Some(&dir), &["status", "--porcelain"])?;
        let paths = paths_between(&dir, "HEAD~1", "HEAD")?;
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(message.trim_end(), "Zoë Ünal|made\n\nbody ünïcode");
        assert_eq!(status, "");
        assert_eq!(paths, ["a b.txt"]);
        Ok(())
    }
}
