//! What the commit box starts from and adds to a message, through git: the repository's
//! `commit.template`, its `prepare-commit-msg` hook, `Co-authored-by:` trailers, its authors
//! (as `.mailmap` names them) and the subjects of its latest commits.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use super::{FS, run, run_ok_codes, run_timed};
use crate::remote;

/// How long `prepare-commit-msg` may take before it is stopped.
pub const HOOK_LIMIT: Duration = Duration::from_secs(10);

/// The file the hook is given the message in, in the git folder: not git's own
/// `COMMIT_EDITMSG`, which holds what a failed `git commit` was given (and may be open in an
/// editor).
const MESSAGE_FILE: &str = "GITCRAFT_EDITMSG";

/// How long a message filter (`stripspace`, `interpret-trailers`) may take.
const FILTER_LIMIT: Duration = Duration::from_secs(30);

/// How many commits are looked at for authors and earlier subjects.
const AUTHOR_COMMITS: &str = "--max-count=5000";
const SUBJECT_COMMITS: &str = "--max-count=400";

/// The text of the repository's `commit.template`, without its comment lines (the commit box
/// commits with `-m`, which keeps them); `None` when none is set. The path is read as git reads
/// it: `~` is that machine's home, a relative path is from the top of the working tree.
pub fn commit_template(repo: &Path) -> Result<Option<String>, String> {
    let printed = run_ok_codes(
        Some(repo),
        &["config", "--path", "--get", "commit.template"],
        &[0, 1],
    )?;
    let printed = printed.trim_end_matches(['\n', '\r']);
    if printed.trim().is_empty() {
        return Ok(None);
    }
    let file = remote::resolve(repo, printed);
    let text = remote::read_to_string(&file)
        .map_err(|e| format!("commit.template is set to {printed}, which can't be read: {e}"))?;
    let starts_blank = text
        .lines()
        .next()
        .is_some_and(|first| first.trim().is_empty());
    let text = strip_comments(repo, &text)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    // A template that starts with a blank line leaves the summary to write: what follows is
    // the description (git's stripspace would make its first line the summary).
    Ok(Some(if starts_blank {
        format!("\n{text}")
    } else {
        text
    }))
}

/// `text` as git cleans a message it opened an editor for: comment lines (`core.commentChar`)
/// out, trailing spaces and runs of blank lines gone.
pub fn strip_comments(repo: &Path, text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    run_timed(
        repo,
        &["stripspace", "--strip-comments"],
        Some(text.as_bytes()),
        FILTER_LIMIT,
    )
}

/// What git tells `prepare-commit-msg` the message comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageSource {
    /// Nothing: an empty message.
    Empty,
    /// The repository's `commit.template`.
    Template,
    /// A message given to git (`-m`): what was typed in the box.
    Message,
    /// A merge in progress, whose message git prepared.
    Merge,
    /// Amending: the commit whose message is reused.
    Commit(String),
}

impl MessageSource {
    /// The words after the message file, as git passes them.
    pub fn args(&self) -> Vec<&str> {
        match self {
            Self::Empty => Vec::new(),
            Self::Template => vec!["template"],
            Self::Message => vec!["message"],
            Self::Merge => vec!["merge"],
            Self::Commit(rev) => vec!["commit", rev.as_str()],
        }
    }
}

/// Runs the repository's `prepare-commit-msg` hook on `seed`, as `git commit` would before it
/// opens the editor, and returns the message it left, cleaned as the editor's would be. `None`
/// when there is no such hook. Stopped after [`HOOK_LIMIT`].
pub fn prepare_commit_msg(
    repo: &Path,
    seed: &str,
    source: &MessageSource,
) -> Result<Option<String>, String> {
    let paths = run(
        Some(repo),
        &[
            "rev-parse",
            "--git-path",
            "hooks/prepare-commit-msg",
            "--git-path",
            MESSAGE_FILE,
        ],
    )?;
    let mut lines = paths.lines();
    let (Some(hook), Some(msg)) = (lines.next(), lines.next()) else {
        return Err("git didn't say where the repository's hooks are".to_owned());
    };
    let hook_file = remote::resolve(repo, hook);
    if !remote::try_is_file(&hook_file).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let msg_file = remote::resolve(repo, msg);
    let mut seed = seed.to_owned();
    if !seed.is_empty() && !seed.ends_with('\n') {
        seed.push('\n');
    }
    remote::write(&msg_file, seed.as_bytes())
        .map_err(|e| format!("Couldn't write {msg} for prepare-commit-msg: {e}"))?;
    let mut args = vec![
        "hook",
        "run",
        "--ignore-missing",
        "prepare-commit-msg",
        "--",
        msg,
    ];
    args.extend(source.args());
    run_timed(repo, &args, None, HOOK_LIMIT).map_err(hook_error)?;
    let left = remote::read_to_string(&msg_file)
        .map_err(|e| format!("Couldn't read what prepare-commit-msg left in {msg}: {e}"))?;
    strip_comments(repo, &left).map(Some)
}

/// What a failed `git hook run` says, in words a commit box can show.
fn hook_error(err: String) -> String {
    if err.contains("is not a git command") || err.contains("unknown subcommand") {
        "Running prepare-commit-msg needs git 2.36 or newer".to_owned()
    } else {
        format!("prepare-commit-msg failed: {}", first_lines(&err, 3))
    }
}

/// The first `n` lines of `text`, `…` after them when there are more.
fn first_lines(text: &str, n: usize) -> String {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.by_ref().take(n).collect();
    let more = lines.next().is_some();
    format!("{}{}", head.join("\n"), if more { "\n…" } else { "" })
}

/// Someone who made commits in the repository, as `.mailmap` names them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
    /// How many of the commits looked at are theirs.
    pub commits: usize,
}

impl Author {
    /// `Name <email>`, as a trailer has it.
    pub fn ident(&self) -> String {
        format!("{} <{}>", self.name, self.email)
    }
}

/// The authors of the repository's latest commits (of every branch), most commits first.
pub fn authors(repo: &Path) -> Result<Vec<Author>, String> {
    let out = run(
        Some(repo),
        &[
            "log",
            "--all",
            "--no-merges",
            AUTHOR_COMMITS,
            super::UTF8,
            &format!("--format=%aN{FS}%aE"),
        ],
    )?;
    Ok(parse_authors(&out))
}

/// Authors from `%aN<FS>%aE` lines, each once (the same email in any case is the same person),
/// most commits first, then by name.
pub fn parse_authors(out: &str) -> Vec<Author> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut authors: Vec<Author> = Vec::new();
    for line in out.lines() {
        let Some((name, email)) = line.split_once(FS) else {
            continue;
        };
        let (name, email) = (name.trim(), email.trim());
        if name.is_empty() || email.is_empty() {
            continue;
        }
        match seen.get(&email.to_lowercase()) {
            Some(&i) => authors[i].commits += 1,
            None => {
                seen.insert(email.to_lowercase(), authors.len());
                authors.push(Author {
                    name: name.to_owned(),
                    email: email.to_owned(),
                    commits: 1,
                });
            }
        }
    }
    authors.sort_by(|a, b| {
        b.commits
            .cmp(&a.commits)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    authors
}

/// `message` with the trailer `key: value` added at its end by `git interpret-trailers`
/// (after a blank line, among the trailers already there; not again when the same one is).
pub fn add_trailer(repo: &Path, message: &str, key: &str, value: &str) -> Result<String, String> {
    let mut input = message.trim_end().to_owned();
    input.push('\n');
    let trailer = format!("{key}: {value}");
    // The box commits with `-m`, which keeps `#` lines: they are text here, not comments
    // (`#123 fixed` would otherwise end up after the trailer), and `---` is no divider.
    let comment = format!("core.commentChar={}", unused_char(&input));
    run_timed(
        repo,
        &[
            "-c",
            &comment,
            "interpret-trailers",
            "--no-divider",
            "--if-exists",
            "addIfDifferent",
            "--trailer",
            &trailer,
        ],
        Some(input.as_bytes()),
        FILTER_LIMIT,
    )
    .map(|out| out.trim_end().to_owned())
    .map_err(|e| format!("Couldn't add the {key} trailer: {e}"))
}

/// A character `text` doesn't have, for git to take as its comment character.
fn unused_char(text: &str) -> char {
    [
        '\u{1}', '\u{2}', '\u{3}', '\u{4}', '\u{5}', '\u{6}', '\u{7}', '`', '|', '~',
    ]
    .into_iter()
    .find(|c| !text.contains(*c))
    .unwrap_or('\u{1}')
}

/// The subjects of the latest commits on HEAD (none on a branch with no commit yet).
pub fn recent_subjects(repo: &Path) -> Vec<String> {
    run(
        Some(repo),
        &[
            "log",
            "--no-merges",
            SUBJECT_COMMITS,
            super::UTF8,
            "--format=%s",
            super::END_OPTS,
            "HEAD",
            "--",
        ],
    )
    .map(|out| out.lines().map(str::to_owned).collect())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authors_are_counted_once_per_email() {
        let out = format!(
            "Ana{FS}ana@x.org\nBob{FS}bob@x.org\nana{FS}ANA@x.org\n{FS}\nnoemail{FS}\n\
             Zoë{FS}zoe@x.org\nZoë{FS}zoe@x.org\n"
        );
        let authors = parse_authors(&out);
        let names: Vec<(&str, usize)> = authors
            .iter()
            .map(|a| (a.name.as_str(), a.commits))
            .collect();
        assert_eq!(names, [("Ana", 2), ("Zoë", 2), ("Bob", 1)]);
        assert_eq!(authors[0].ident(), "Ana <ana@x.org>");
        assert!(parse_authors("").is_empty());
    }

    #[test]
    fn sources_are_passed_as_git_does() {
        assert!(MessageSource::Empty.args().is_empty());
        assert_eq!(MessageSource::Template.args(), ["template"]);
        assert_eq!(MessageSource::Merge.args(), ["merge"]);
        assert_eq!(MessageSource::Message.args(), ["message"]);
        assert_eq!(
            MessageSource::Commit("HEAD".into()).args(),
            ["commit", "HEAD"]
        );
    }

    #[test]
    fn an_old_git_is_named() {
        let err = hook_error("git: 'hook' is not a git command. See 'git --help'.".into());
        assert!(err.contains("2.36"), "{err}");
        assert!(hook_error("exit 3".into()).starts_with("prepare-commit-msg failed"));
        let long = hook_error("a\nb\nc\nd\ne".into());
        assert!(long.ends_with("a\nb\nc\n…"), "{long}");
    }
}
