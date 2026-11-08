//! The history the views over time read (evolution heatmap, change coupling, churn timeline):
//! the last commits of a revision with the lines each changed in each file, as
//! `git log --numstat` says. Merges count nothing of their own (`git log` shows no numstat for
//! them without `-m`), so no change is counted twice.

use std::path::Path;

use crate::git;

/// One file a commit changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    /// Its name before, when the commit renamed it.
    pub old_path: Option<String>,
    /// Lines added and deleted; `None` for a binary file.
    pub added: Option<u32>,
    pub deleted: Option<u32>,
}

impl FileChange {
    /// Lines changed, binary files counting as one.
    pub fn lines(&self) -> u32 {
        match (self.added, self.deleted) {
            (Some(a), Some(d)) => a + d,
            _ => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistCommit {
    pub hash: String,
    pub parents: Vec<String>,
    /// Author time, seconds since the epoch.
    pub time: i64,
    pub author: String,
    pub subject: String,
    pub files: Vec<FileChange>,
}

impl HistCommit {
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
}

/// The commits read, newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    /// What was read: a branch, `HEAD`, a range.
    pub rev: String,
    /// The commit `rev` was at.
    pub tip: String,
    pub commits: Vec<HistCommit>,
    /// At most so many commits were read.
    pub limit: usize,
    /// The limit was reached: older history exists and isn't counted.
    pub truncated: bool,
}

impl History {
    /// What the views say of how much of the history this is.
    pub fn extent(&self) -> String {
        let n = self.commits.len();
        let noun = if n == 1 { "commit" } else { "commits" };
        if self.truncated {
            format!("last {n} {noun} of {}", self.rev)
        } else {
            format!("all {n} {noun} of {}", self.rev)
        }
    }

    pub fn provenance(&self) -> super::Provenance {
        let mut p = super::Provenance::new("git log --numstat", self.extent())
            .tip(self.rev.clone(), self.tip.clone());
        if self.truncated {
            p = p.partial(format!(
                "Only the last {} commits are counted; older history isn't",
                self.limit
            ));
        }
        p
    }
}

const RECORD: char = '\u{1e}';
const FIELD: char = '\u{1f}';

/// The last `limit` commits of `rev` (a branch, `HEAD`, `a..b`), with what each changed. Renames
/// are followed (`-M`), as one change of the new path.
pub fn read(repo: &Path, rev: &str, limit: usize) -> Result<History, String> {
    if rev.starts_with('-') {
        return Err("not a revision".to_owned());
    }
    let tip_rev = rev
        .rsplit("..")
        .next()
        .filter(|r| !r.is_empty())
        .unwrap_or("HEAD");
    let tip = git::resolve(repo, tip_rev)?;
    // One more than asked: whether there is older history.
    let count = format!("--max-count={}", limit + 1);
    let out = git::run(
        Some(repo),
        &[
            "log",
            "--no-color",
            "--no-ext-diff",
            "--numstat",
            "-z",
            "-M",
            "--format=%x1e%H%x1f%P%x1f%at%x1f%an%x1f%s",
            &count,
            rev,
            "--",
        ],
    )?;
    let mut commits = parse(&out);
    let truncated = commits.len() > limit;
    commits.truncate(limit);
    Ok(History {
        rev: rev.to_owned(),
        tip,
        commits,
        limit,
        truncated,
    })
}

/// What `git log --numstat -z` with the format of [`read`] printed.
pub fn parse(out: &str) -> Vec<HistCommit> {
    out.split(RECORD)
        .filter(|r| !r.is_empty())
        .filter_map(parse_record)
        .collect()
}

fn parse_record(record: &str) -> Option<HistCommit> {
    let (header, rest) = record.split_once('\0').unwrap_or((record, ""));
    let mut fields = header.trim_end_matches('\n').split(FIELD);
    let hash = fields.next()?.to_owned();
    if hash.is_empty() {
        return None;
    }
    let parents = fields
        .next()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let time = fields.next().and_then(|t| t.parse().ok()).unwrap_or(0);
    let author = fields.next().unwrap_or("").to_owned();
    let subject = fields.next().unwrap_or("").to_owned();
    let mut files = Vec::new();
    let mut tokens = rest.trim_start_matches('\n').split('\0');
    while let Some(token) = tokens.next() {
        let token = token.trim_start_matches('\n');
        if token.is_empty() {
            continue;
        }
        let mut parts = token.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (added, deleted) = (added.parse().ok(), deleted.parse().ok());
        // A rename: the path is empty, and the old and new names follow.
        let (path, old_path) = if path.is_empty() {
            let old = tokens.next().unwrap_or("").to_owned();
            let new = tokens.next().unwrap_or("").to_owned();
            (new, Some(old))
        } else {
            (path.to_owned(), None)
        };
        files.push(FileChange {
            path,
            old_path,
            added,
            deleted,
        });
    }
    Some(HistCommit {
        hash,
        parents,
        time,
        author,
        subject,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_records_are_parsed_with_renames_and_binaries() {
        let out = "\u{1e}c2\u{1f}c1\u{1f}20\u{1f}Ann\u{1f}two\0\n1\t0\t\0a.txt\0b c.txt\0\
                   \u{1e}c1\u{1f}\u{1f}10\u{1f}Ann\u{1f}one\0\n2\t0\ta.txt\0-\t-\tbin\0";
        let commits = parse(out);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "c2");
        assert_eq!(commits[0].parents, ["c1"]);
        assert_eq!(
            commits[0].files,
            [FileChange {
                path: "b c.txt".to_owned(),
                old_path: Some("a.txt".to_owned()),
                added: Some(1),
                deleted: Some(0),
            }]
        );
        assert!(commits[1].parents.is_empty());
        assert_eq!(commits[1].time, 10);
        assert_eq!(commits[1].files.len(), 2);
        assert_eq!(commits[1].files[1].added, None);
        assert_eq!(commits[1].files[1].lines(), 1);
    }

    #[test]
    fn a_merge_without_changes_of_its_own_has_no_files() {
        let out = "\u{1e}m\u{1f}a b\u{1f}5\u{1f}Ann\u{1f}Merge\0\n";
        let commits = parse(out);
        assert!(commits[0].is_merge());
        assert!(commits[0].files.is_empty());
    }

    #[test]
    fn a_real_history_is_read_up_to_its_limit() -> Result<(), String> {
        let repo = git::fixture::Repo::new("insights-history")?;
        repo.commit("one", &[("a.txt", Some("a\n"))])?;
        repo.commit(
            "two",
            &[("src/b.rs", Some("b\n")), ("a.txt", Some("a\nb\n"))],
        )?;
        let third = repo.commit("three", &[("src/b.rs", Some("c\n"))])?;
        let all = read(repo.path(), "HEAD", 10)?;
        assert_eq!(all.commits.len(), 3);
        assert!(!all.truncated);
        assert_eq!(all.tip, third);
        assert_eq!(all.commits[1].files.len(), 2);
        let last = read(repo.path(), "HEAD", 2)?;
        assert_eq!(last.commits.len(), 2);
        assert!(last.truncated);
        assert!(last.provenance().partial.is_some());
        assert!(read(repo.path(), "--all", 2).is_err());
        Ok(())
    }
}
