//! One file: its contents at a revision, its history and blame, and ignoring it.

use std::path::Path;

use super::{END_OPTS, FS, LITERAL_PATHS, NO_SIGNATURE, RS, UTF8, not_an_option, run, run_bytes};

/// The contents of `path` at `rev` (`HEAD`, a hash, or `""` for the index), or from disk when
/// `rev` is `None`.
pub fn file_at(repo: &Path, rev: Option<&str>, path: &str) -> Result<Vec<u8>, String> {
    match rev {
        None => worktree_file(&crate::remote::join(repo, path)).map_err(|e| format!("{path}: {e}")),
        // The index as `:0:path`: `:path` of a file named `1:x` would be stage 1 of `x`.
        Some("") => run_bytes(Some(repo), &["show", END_OPTS, &format!(":0:{path}")]),
        Some(rev) => super::backend::dispatch(
            repo,
            super::backend::Read::FileAt,
            |repo| super::backend::file_at(repo, rev, path),
            || run_bytes(Some(repo), &["show", END_OPTS, &format!("{rev}:{path}")]),
        ),
    }
}

/// A commit's author, email and message (as `%an`, `%ae`, `%B`) and its note in each notes
/// ref asked for (as `%N`), none where it has none.
pub type Authorship = (String, String, String, Vec<Option<String>>);

/// [`Authorship`] of commit `hash`, read in-process, or an error where only git reads it
/// faithfully.
pub fn commit_authorship(repo: &Path, hash: &str, notes: &[&str]) -> Result<Authorship, String> {
    super::backend::dispatch(
        repo,
        super::backend::Read::Authorship,
        |repo| super::backend::authorship(repo, hash, notes),
        || Err("read by git".into()),
    )
}

/// The textconv program of `path`'s diff driver (none without one), read in-process, or an
/// error where only git reads it faithfully.
pub fn textconv_of(repo: &Path, path: &str) -> Result<Option<String>, String> {
    super::backend::dispatch(
        repo,
        super::backend::Read::Textconv,
        |repo| super::backend::textconv_of(repo, path),
        || Err("read by git".into()),
    )
}

/// The bytes a file had before a commit and in it: none for a side it isn't on.
pub type FileSides = (Option<Vec<u8>>, Option<Vec<u8>>);

/// The bytes `path` had before commit `hash` (in its first parent, under its old name when
/// renamed) and in it, none for a side it isn't on; read in-process, or an error where only git
/// would read them faithfully.
pub fn commit_file_sides(repo: &Path, hash: &str, path: &str) -> Result<FileSides, String> {
    super::backend::dispatch(
        repo,
        super::backend::Read::CommitFileSides,
        |repo| super::backend::commit_file_sides(repo, hash, path),
        || Err("read by git".into()),
    )
}

/// A file of the working tree as git sees it: a symbolic link is the path it holds (what git
/// stores and diffs), not what it points to, which can be anywhere and need not end. A merge
/// leaves the other side's link in place when it conflicts with a file (`x -> /dev/zero`):
/// read through, the app would never stop reading it. Anything else but a file is refused.
pub fn worktree_file(file: &Path) -> std::io::Result<Vec<u8>> {
    // In WSL or over SSH: the same, read there.
    if let Some(read) = crate::remote::worktree_file(file) {
        return read;
    }
    let meta = std::fs::symlink_metadata(file)?;
    if meta.file_type().is_symlink() {
        return Ok(std::fs::read_link(file)?
            .into_os_string()
            .into_encoded_bytes());
    }
    if !meta.is_file() {
        return Err(std::io::Error::other(crate::i18n::text(
            "backend-not-a-file-2cd310",
        )));
    }
    // Checked again on what was opened: a link put there since would be followed by `open`.
    let mut opened = std::fs::File::open(file)?;
    if !opened.metadata()?.is_file() {
        return Err(std::io::Error::other(crate::i18n::text(
            "backend-not-a-file-2cd310",
        )));
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut opened, &mut bytes)?;
    Ok(bytes)
}

/// Every file in the tree of commit `rev`, sorted by path.
pub fn tree_files(repo: &Path, rev: &str) -> Result<Vec<String>, String> {
    let out = run(
        Some(repo),
        &[
            "ls-tree",
            "-r",
            "-z",
            "--name-only",
            "--full-tree",
            END_OPTS,
            rev,
        ],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Adds `pattern` as a line of the repository's top `.gitignore` (created if missing). `false`
/// when it was already there.
pub fn add_to_gitignore(repo: &Path, pattern: &str) -> Result<bool, String> {
    let file = repo.join(".gitignore");
    // A `.gitignore` a repository made a link (to `~/.bashrc`…) is not written through: the
    // line added comes from a file name the repository chose too.
    match crate::remote::try_is_link(&file) {
        Ok(false) => {}
        Ok(true) => {
            return Err(crate::i18n::text(
                "backend-gitignore-is-a-symbolic-link-not-changed-0d6ca8",
            )
            .to_owned());
        }
        Err(e) => {
            return Err(crate::i18n::message(
                "backend-could-not-check-gitignore-e-27d776",
                &[("detail", (e).to_string().into())],
            ));
        }
    }
    let existing = match crate::remote::read_to_string(&file) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(crate::i18n::message(
                "backend-could-not-read-gitignore-e-3886bb",
                &[("detail", (e).to_string().into())],
            ));
        }
    };
    match gitignore_with(&existing, pattern) {
        Some(updated) => crate::remote::write(&file, updated.as_bytes())
            .map(|_| true)
            .map_err(|e| {
                crate::i18n::message(
                    "backend-could-not-write-gitignore-e-9b2199",
                    &[("detail", (e).to_string().into())],
                )
            }),
        None => Ok(false),
    }
}

/// `existing` with `pattern` appended on a line of its own (keeping its line endings), or `None`
/// when the line is already there.
fn gitignore_with(existing: &str, pattern: &str) -> Option<String> {
    if existing.lines().any(|l| l.trim() == pattern) {
        return None;
    }
    let nl = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut out = existing.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(nl);
    }
    out.push_str(pattern);
    out.push_str(nl);
    Some(out)
}

/// Whether git tracks `path` (so ignoring it also needs `git rm --cached`).
pub fn is_tracked(repo: &Path, path: &str) -> bool {
    let args = [LITERAL_PATHS, "ls-files", "--error-unmatch", "--", path];
    run(Some(repo), &args).is_ok()
}

/// One commit in a file's history, with the file's path at that commit (renames are followed).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileCommit {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub summary: String,
    pub path: String,
    /// In a history of some lines: the lines of the file the commit's change to them covers
    /// (as they are in the commit); empty otherwise.
    pub lines: Vec<(u32, u32)>,
}

/// Every commit that changed `path` up to `rev` (HEAD when `None`), newest first, following
/// renames.
pub fn file_history(repo: &Path, rev: Option<&str>, path: &str) -> Result<Vec<FileCommit>, String> {
    let format = format!("--format={RS}%H{FS}%an{FS}%ae{FS}%at{FS}%s");
    // `-z`: names as they are, not quoted (`"a\"b.txt"`) when they have quotes or tabs in them.
    let mut args = vec![
        LITERAL_PATHS,
        "log",
        NO_SIGNATURE,
        UTF8,
        "--follow",
        "--name-only",
        "-z",
        &format,
        END_OPTS,
    ];
    if let Some(rev) = rev {
        args.push(rev);
    }
    args.extend(["--", path]);
    let out = run(Some(repo), &args)?;
    Ok(parse_file_history(&out))
}

/// `log --name-only -z`: `RS fields\0`, then `\npath\0` for the file.
fn parse_file_history(out: &str) -> Vec<FileCommit> {
    out.split(RS)
        .filter_map(|record| {
            let mut lines = record.split('\0').map(|l| l.trim_start_matches('\n'));
            let mut f = lines.next()?.split(FS);
            let mut c = FileCommit {
                hash: f.next()?.to_owned(),
                author: f.next().unwrap_or_default().to_owned(),
                email: f.next().unwrap_or_default().to_owned(),
                time: f.next().and_then(|t| t.parse().ok()).unwrap_or(0),
                summary: f.next().unwrap_or_default().to_owned(),
                path: String::new(),
                lines: Vec::new(),
            };
            c.path = lines.rfind(|l| !l.is_empty())?.to_owned();
            Some(c)
        })
        .collect()
}

/// One line of `git blame`: who last changed it, in which commit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlameLine {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub summary: String,
    pub text: String,
    /// The line's number in that commit, and the file's name there.
    pub orig: u32,
    pub file: String,
}

impl BlameLine {
    /// Lines changed in the working tree and not committed yet.
    pub fn uncommitted(&self) -> bool {
        self.hash.bytes().all(|b| b == b'0')
    }
}

/// `git blame` of `path`, as of `rev` (the working tree when `None`).
pub fn blame(repo: &Path, rev: Option<&str>, path: &str) -> Result<Vec<BlameLine>, String> {
    blame_with(repo, rev, path, &[])
}

/// [`blame`] of lines `first` to `last` only, without the text conversions the repository's
/// attributes name: for what an agent asks (no program the repository configures runs for it).
pub fn blame_plain(
    repo: &Path,
    rev: Option<&str>,
    path: &str,
    (first, last): (u64, u64),
) -> Result<Vec<BlameLine>, String> {
    let range = format!("{first},{last}");
    blame_with(repo, rev, path, &["--no-textconv", "-L", &range])
}

fn blame_with<'a>(
    repo: &Path,
    rev: Option<&'a str>,
    path: &'a str,
    extra: &[&'a str],
) -> Result<Vec<BlameLine>, String> {
    // Names and summaries in UTF-8, whatever `i18n.logOutputEncoding` says (`blame` has no
    // `--encoding`).
    let mut args = vec!["-c", "i18n.logOutputEncoding=UTF-8", "blame", "--porcelain"];
    args.extend(extra);
    if let Some(rev) = rev {
        // `blame` takes no `--end-of-options`.
        args.push(not_an_option(rev)?);
    }
    args.extend(["--", path]);
    match run(Some(repo), &args) {
        // `blame.ignoreRevsFile` set for all repositories (`.git-blame-ignore-revs`) names a
        // file this one may not have, and git then blames nothing at all.
        Err(e) if e.contains("could not open object name list") => {
            args.insert(4 + extra.len(), "--no-ignore-revs-file");
            Ok(parse_blame(&run(Some(repo), &args)?))
        }
        out => Ok(parse_blame(&out?)),
    }
}

fn parse_blame(out: &str) -> Vec<BlameLine> {
    use std::collections::HashMap;
    // The porcelain format describes each commit once, the first time it shows up.
    let mut known: HashMap<String, BlameLine> = HashMap::new();
    let mut lines = Vec::new();
    let mut current: Option<BlameLine> = None;
    for line in out.lines() {
        if let Some(text) = line.strip_prefix('\t') {
            if let Some(mut c) = current.take() {
                known.entry(c.hash.clone()).or_insert_with(|| c.clone());
                c.text = text.to_owned();
                lines.push(c);
            }
            continue;
        }
        let Some(c) = current.as_mut() else {
            let hash = line.split(' ').next().unwrap_or_default();
            // SHA-1, or SHA-256 in a repository made with `--object-format=sha256`.
            if matches!(hash.len(), 40 | 64) && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                let mut c = known.get(hash).cloned().unwrap_or_else(|| BlameLine {
                    hash: hash.to_owned(),
                    ..BlameLine::default()
                });
                c.orig = line
                    .split(' ')
                    .nth(1)
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                current = Some(c);
            }
            continue;
        };
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "author" => c.author = value.to_owned(),
            "author-mail" => c.email = value.trim_matches(['<', '>']).to_owned(),
            "author-time" => c.time = value.parse().unwrap_or(0),
            "summary" => c.summary = value.to_owned(),
            "filename" => c.file = value.to_owned(),
            _ => {}
        }
    }
    lines
}

#[cfg(test)]
mod tests;
