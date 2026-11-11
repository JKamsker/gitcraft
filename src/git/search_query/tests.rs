//! The parser, its mapping to `git log` options, and searches on real repositories through
//! [`prepare`] and [`run`], as the graph's search runs them.

use std::path::{Path, PathBuf};

use super::parse::{QueryError, SearchQuery, parse};
use super::*;
use crate::git::run as git;

// ---- On real repositories ---------------------------------------------------------------

/// A repository removed when dropped.
struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Repo {
    fn new(name: &str) -> Result<Repo, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-sq-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Repo(dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Alice Smith"],
            &["config", "user.email", "alice@example.com"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        git(Some(&self.0), args)
    }

    fn write(&self, file: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(file);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// Commits everything as `author` (`Name <email>`) on `date`, with `message`; its hash.
    fn commit(&self, message: &str, author: &str, date: &str) -> Result<String, String> {
        self.git(&["add", "-A"])?;
        self.git(&[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            message,
            "--author",
            author,
            "--date",
            date,
        ])?;
        self.head()
    }

    fn head(&self) -> Result<String, String> {
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_owned())
    }

    fn search(&self, query: &str, code: bool) -> Result<Vec<String>, String> {
        let plan = prepare(query, code).map_err(|e| e.message)?;
        run(self.path(), None, &plan)
    }
}

const ALICE: &str = "Alice Smith <alice@example.com>";
const BOB: &str = "Bob <bob@old.example>";
const ZOE: &str = "Zoë Ünal <zoe@example.com>";

/// main: init (Alice) → login fix (Bob) → feature merged (Alice, no fast-forward) →
/// résumé (Zoë, with a `.mailmap` naming Bob “Robert Bé”).
struct History {
    repo: Repo,
    init: String,
    login: String,
    feature: String,
    merge: String,
    resume: String,
}

fn history(name: &str) -> Result<History, String> {
    let repo = Repo::new(name)?;
    repo.write("src/main.rs", "fn main() {}\n")?;
    repo.write("docs/Read me.md", "# Read me\n")?;
    let init = repo.commit("Initial commit", ALICE, "2020-01-01T12:00:00Z")?;
    repo.write("src/main.rs", "fn main() {\n    login();\n}\n")?;
    let login = repo.commit(
        "Fix login bug\n\nThe session expired.",
        BOB,
        "2022-03-01T12:00:00Z",
    )?;
    repo.git(&["checkout", "-q", "-b", "feature"])?;
    repo.write("src/lib.rs", "pub fn feature_flag() -> bool { true }\n")?;
    let feature = repo.commit("Add feature flag", ALICE, "2023-01-10T12:00:00Z")?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.git(&["merge", "-q", "--no-ff", "-m", "Merge feature", "feature"])?;
    let merge = repo.head()?;
    repo.write("docs/café.md", "naïve text\n")?;
    repo.write(
        ".mailmap",
        "Robert Bé <bob@new.example> <bob@old.example>\n",
    )?;
    let resume = repo.commit("Ünïcödé résumé", ZOE, "2023-06-15T12:00:00Z")?;
    Ok(History {
        repo,
        init,
        login,
        feature,
        merge,
        resume,
    })
}

#[test]
fn author_and_committer_on_a_real_repository() -> Result<(), String> {
    let h = history("people")?;
    assert_eq!(
        h.repo.search("author:alice", false)?,
        [h.merge.as_str(), h.feature.as_str(), h.init.as_str()]
    );
    assert_eq!(h.repo.search("author:ALICE@EXAMPLE.COM", false)?.len(), 3);
    assert_eq!(
        h.repo.search("author:\"Zoë Ünal\"", false)?,
        [h.resume.as_str()]
    );
    // The name `.mailmap` gives, and the one in the commit, both find Bob's commit.
    assert_eq!(
        h.repo.search("author:\"Robert Bé\"", false)?,
        [h.login.as_str()]
    );
    assert_eq!(h.repo.search("author:bob", false)?, [h.login.as_str()]);
    // Several authors: any of them.
    assert_eq!(
        h.repo.search("author:zoë author:robert", false)?,
        [h.resume.as_str(), h.login.as_str()]
    );
    // Everything was committed by the repository's user.
    assert_eq!(h.repo.search("committer:alice", false)?.len(), 5);
    assert!(h.repo.search("committer:zoë", false)?.is_empty());
    assert!(h.repo.search("author:nobody", false)?.is_empty());
    Ok(())
}

#[test]
fn path_message_and_free_text_on_a_real_repository() -> Result<(), String> {
    let h = history("paths")?;
    assert_eq!(
        h.repo.search("path:src/main.rs", false)?,
        [h.login.as_str(), h.init.as_str()]
    );
    assert_eq!(
        h.repo.search("path:\"docs/Read me.md\"", false)?,
        [h.init.as_str()]
    );
    assert_eq!(
        h.repo.search("path:docs/café.md", false)?,
        [h.resume.as_str()]
    );
    assert_eq!(h.repo.search("path:src", false)?.len(), 3);
    assert_eq!(
        h.repo.search("path:src/lib.rs path:docs/café.md", false)?,
        [h.resume.as_str(), h.feature.as_str()]
    );
    assert_eq!(
        h.repo.search("path:*.md", false)?,
        [h.resume.as_str(), h.init.as_str()]
    );
    assert!(h.repo.search("path:missing.txt", false)?.is_empty());
    assert_eq!(h.repo.search("message:session", false)?, [h.login.as_str()]);
    // Letter case is ignored for A to Z (git's -i). Whether git also folds other letters
    // depends on its platform and locale, so that is not asserted.
    assert_eq!(h.repo.search("message:RéSuMé", false)?, [h.resume.as_str()]);
    // Several messages must all match.
    assert_eq!(
        h.repo.search("message:fix message:bug", false)?,
        [h.login.as_str()]
    );
    assert!(
        h.repo
            .search("message:fix message:feature", false)?
            .is_empty()
    );
    // Free text: in the message or the author, narrowed by the operators.
    assert_eq!(
        h.repo.search("feature path:src", false)?,
        [h.feature.as_str()]
    );
    assert_eq!(h.repo.search("bob path:src", false)?, [h.login.as_str()]);
    assert_eq!(
        h.repo.search("\"login bug\" author:bob", false)?,
        [h.login.as_str()]
    );
    assert!(h.repo.search("bob author:alice", false)?.is_empty());
    Ok(())
}

#[test]
fn contents_regex_and_merges_on_a_real_repository() -> Result<(), String> {
    let h = history("changes")?;
    assert_eq!(
        h.repo.search("contents:login()", false)?,
        [h.login.as_str()]
    );
    assert_eq!(h.repo.search("contents:naïve", false)?, [h.resume.as_str()]);
    // With a name or message to match, git ignores the case of A to Z in changes too.
    assert_eq!(
        h.repo.search("contents:NAïVE author:zoë", false)?,
        [h.resume.as_str()]
    );
    assert_eq!(
        h.repo.search("regex:feature_[a-z]+", false)?,
        [h.feature.as_str()]
    );
    assert_eq!(
        // Git uses the platform POSIX regex engine; \s is a GNU extension.
        h.repo.search("regex:fn[[:space:]]+main", false)?,
        [h.login.as_str(), h.init.as_str()]
    );
    assert_eq!(
        h.repo.search("contents:login() author:alice", false)?,
        Vec::<String>::new()
    );
    // Code: free text is what the commits added or removed.
    assert_eq!(
        h.repo.search("feature_flag path:src", true)?,
        [h.feature.as_str()]
    );
    assert_eq!(h.repo.search("merges:", false)?, [h.merge.as_str()]);
    assert_eq!(
        h.repo.search("-merges: author:alice", false)?,
        [h.feature.as_str(), h.init.as_str()]
    );
    assert_eq!(
        h.repo.search("merge -merges:", false)?,
        Vec::<String>::new()
    );
    let e = h.repo.search("regex:[unclosed", false).err();
    assert!(e.is_some_and(|e| e.starts_with("git could not search")));
    Ok(())
}

#[test]
fn dates_on_a_real_repository() -> Result<(), String> {
    let repo = Repo::new("dates")?;
    for (summary, date) in [
        ("old", "2020-01-01T12:00:00Z"),
        ("mid", "2023-06-01T12:00:00Z"),
        ("recent", "now"),
    ] {
        repo.write("f.txt", summary)?;
        repo.commit(summary, ALICE, date)?;
    }
    // git filters by the commit date: give each commit its author's (new hashes, then).
    repo.git(&["rebase", "-q", "--committer-date-is-author-date", "--root"])?;
    let hashes = repo.git(&["log", "--format=%H %s"])?;
    let hash = |summary: &str| {
        hashes
            .lines()
            .find_map(|l| l.strip_suffix(&format!(" {summary}")))
            .unwrap_or_default()
            .to_owned()
    };
    let (old, mid, recent) = (hash("old"), hash("mid"), hash("recent"));
    assert!(!old.is_empty() && !mid.is_empty() && !recent.is_empty());
    assert_eq!(
        repo.search("after:2023", false)?,
        [recent.as_str(), mid.as_str()]
    );
    assert_eq!(repo.search("before:2023-06-01", false)?, [old.as_str()]);
    assert_eq!(
        repo.search("after:2023-06 before:2024", false)?,
        [mid.as_str()]
    );
    assert_eq!(repo.search("after:2w", false)?, [recent.as_str()]);
    assert_eq!(repo.search("after:today", false)?, [recent.as_str()]);
    assert_eq!(
        repo.search("before:1y", false)?,
        [mid.as_str(), old.as_str()]
    );
    Ok(())
}

#[test]
fn the_revisions_the_graph_shows_limit_the_search() -> Result<(), String> {
    let h = history("revisions")?;
    let feature_only = ["feature".to_owned()];
    let plan = prepare("author:alice", false).map_err(|e| e.message)?;
    assert_eq!(
        run(h.repo.path(), Some(&feature_only), &plan)?,
        [h.feature.as_str(), h.init.as_str()]
    );
    assert!(run(h.repo.path(), Some(&[]), &plan)?.is_empty());
    let plan = prepare("path:src", false).map_err(|e| e.message)?;
    assert_eq!(run(h.repo.path(), Some(&feature_only), &plan)?.len(), 3);
    Ok(())
}

#[test]
fn people_come_from_history_and_mailmap() -> Result<(), String> {
    let h = history("mailmap")?;
    let authors = people(h.repo.path(), false)?;
    let names: Vec<(&str, &str, usize)> = authors
        .iter()
        .map(|p| (p.name.as_str(), p.email.as_str(), p.commits))
        .collect();
    assert_eq!(
        names,
        [
            ("Alice Smith", "alice@example.com", 3),
            ("Robert Bé", "bob@new.example", 1),
            ("Zoë Ünal", "zoe@example.com", 1),
        ]
    );
    let committers = people(h.repo.path(), true)?;
    assert_eq!(committers.len(), 1);
    assert_eq!(committers[0].commits, 5);
    let empty = Repo::new("mailmap-empty")?;
    assert!(people(empty.path(), false)?.is_empty());
    Ok(())
}

#[test]
fn a_plain_query_is_the_search_it_always_was() -> Result<(), String> {
    let h = history("plain")?;
    assert_eq!(h.repo.search("login", false)?, [h.login.as_str()]);
    assert_eq!(h.repo.search("Zoë", false)?, [h.resume.as_str()]);
    assert_eq!(h.repo.search(&h.init[..8], false)?, [h.init.as_str()]);
    assert_eq!(h.repo.search("login();", true)?, [h.login.as_str()]);
    // Quotes and colons in plain text are only characters.
    assert!(h.repo.search("\"login bug\"", false)?.is_empty());
    assert_eq!(h.repo.search("Merge feature", false)?, [h.merge.as_str()]);
    Ok(())
}

#[test]
fn case_and_hashes_with_operators_match_the_plain_search() -> Result<(), String> {
    let h = history("case-hash")?;
    // Code is case-sensitive with or without operators that only narrow.
    assert!(h.repo.search("LOGIN()", true)?.is_empty());
    assert!(h.repo.search("LOGIN() path:src", true)?.is_empty());
    assert!(h.repo.search("contents:LOGIN()", false)?.is_empty());
    assert_eq!(h.repo.search("login() path:src", true)?, [h.login.as_str()]);
    // A hash in the free text finds its commit first, when the operators match it.
    let short = &h.login[..7];
    let found = h.repo.search(&format!("{short} path:src"), false)?;
    assert_eq!(found.first().map(String::as_str), Some(h.login.as_str()));
    assert!(
        h.repo
            .search(&format!("{short} path:docs"), false)?
            .is_empty()
    );
    assert!(
        h.repo
            .search(&format!("{short} author:alice"), false)?
            .is_empty()
    );
    Ok(())
}

mod grammar;
