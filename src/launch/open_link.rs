//! A link to a forge's page, made sense of here: the clone of its repository among those
//! gitcraft knows (open tabs, recent repositories), by their remotes' URLs, and what to show
//! there. Off the window's thread: it asks git.

use std::path::PathBuf;

use super::link::{ForgeLink, Target};
use super::resolve::{Resolved, Then};
use crate::git;

/// Where `link` opens: in the first of `clones` with a remote at its repository, or, when
/// none has one, an offer to clone it.
pub(crate) fn resolve_link(link: &ForgeLink, clones: &[PathBuf]) -> Result<Resolved, String> {
    let Some(root) = clones.iter().find(|clone| has_remote(clone, link)) else {
        return Ok(Resolved {
            root: PathBuf::new(),
            then: Then::Clone(link.clone_url()),
        });
    };
    let then = match &link.target {
        Target::Repo => Then::Nothing,
        Target::PullRequest(n) => Then::PullRequest(*n),
        Target::Commit(sha) => Then::Commit(git::resolve(root, sha).map_err(|_| {
            format!(
                "Commit {sha} isn't in {}: fetch, then open the link again",
                root.display()
            )
        })?),
        Target::File { path, line, .. } => Then::Blame {
            path: path.clone(),
            line: *line,
        },
    };
    Ok(Resolved {
        root: root.clone(),
        then,
    })
}

/// Whether one of `clone`'s remotes is `link`'s repository (a clone that can't be read isn't).
fn has_remote(clone: &std::path::Path, link: &ForgeLink) -> bool {
    git::remotes(clone).is_ok_and(|remotes| remotes.iter().any(|r| link.is_remote(&r.url)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::link::parse_web_url;

    fn repo(name: &str, remote: &str) -> Result<PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-link {name} ü-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("src/a b.rs"), "x\n").map_err(|e| e.to_string())?;
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["remote", "add", "origin", remote],
            &["add", "-A"],
            &["commit", "-q", "-m", "one"],
        ] {
            git::run(Some(&dir), args)?;
        }
        Ok(dir)
    }

    /// Among two clones, the one whose remote is the link's; and on it the link's commit,
    /// pull request or file.
    #[test]
    fn a_link_opens_in_its_clone() -> Result<(), String> {
        let other = repo("other", "https://github.com/o/other.git")?;
        let mine = repo("mine", "git@github.com:O/R.git")?;
        let missing = std::env::temp_dir().join("gitcraft-link-no-such-folder");
        let clones = [missing, other.clone(), mine.clone()];
        let head = git::run(Some(&mine), &["rev-parse", "HEAD"])?
            .trim()
            .to_owned();
        let short = head.get(..10).unwrap_or(&head).to_owned();
        let open =
            |url: &str| -> Result<Resolved, String> { resolve_link(&parse_web_url(url)?, &clones) };
        let pr = open("https://github.com/o/r/pull/12");
        let commit = open(&format!("https://github.com/o/r/commit/{short}"));
        let file = open("https://github.com/o/r/blob/main/src/a%20b.rs#L1");
        let unknown = open("https://github.com/o/r/commit/deadbeefdeadbeef");
        let none = open("https://codeberg.org/someone/else/pulls/1");
        let _ = std::fs::remove_dir_all(&other);
        let _ = std::fs::remove_dir_all(&mine);
        let pr = pr?;
        assert_eq!(pr.root, mine);
        assert_eq!(pr.then, Then::PullRequest(12));
        assert_eq!(commit?.then, Then::Commit(head));
        assert_eq!(
            file?.then,
            Then::Blame {
                path: "src/a b.rs".into(),
                line: Some(1)
            }
        );
        assert!(
            unknown
                .err()
                .is_some_and(|e| e.contains("fetch, then open the link again"))
        );
        assert_eq!(
            none?.then,
            Then::Clone("https://codeberg.org/someone/else.git".into())
        );
        Ok(())
    }
}
