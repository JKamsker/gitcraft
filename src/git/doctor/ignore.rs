//! Why a file is ignored and what attributes it has: the rule responsible
//! (`check-ignore -v`), its attributes (`check-attr -a`) with what they mean (line endings,
//! text or binary, LFS, merge and diff drivers, and whether the drivers they name are set up),
//! and its line endings in the index and the working tree (`ls-files --eol`).

use std::path::Path;

use crate::git::{run, run_ok_codes};

/// The ignore rule that decides about a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The file it is in, as git names it (`.gitignore`, `sub/.gitignore`, `.git/info/exclude`,
    /// `/home/me/.config/git/ignore`).
    pub source: String,
    pub line: u32,
    pub pattern: String,
}

impl Rule {
    /// A `!pattern`: it un-ignores what an earlier rule ignored.
    pub fn negated(&self) -> bool {
        self.pattern.starts_with('!')
    }
}

/// One attribute git gives a path, as `check-attr` says it (`set`, `unset` or a value).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub value: String,
}

/// A path's line endings as `ls-files --eol` reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Eol {
    /// In the index (`lf`, `crlf`, `mixed`, `none`, `-text` for binary; empty when not there).
    pub index: String,
    /// In the working tree, as above.
    pub worktree: String,
    /// What the attributes ask for (`text=auto eol=lf`…), empty when nothing.
    pub attr: String,
}

/// A driver an attribute names that the configuration here doesn't define.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingDriver {
    /// `merge`, `diff` or `filter`.
    pub attr: String,
    pub name: String,
    /// A filter marked `filter.<name>.required`: git refuses to add or check the file out.
    pub required: bool,
}

/// Everything the doctor says about one path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Explained {
    pub path: String,
    /// It is a folder: the rule decides for everything in it.
    pub folder: bool,
    /// Nothing of that name is in the working tree (the rules are read as for a file).
    pub missing: bool,
    pub tracked: bool,
    /// The last rule that matches it, `None` when none does.
    pub rule: Option<Rule>,
    pub attrs: Vec<Attr>,
    pub eol: Option<Eol>,
    pub missing_drivers: Vec<MissingDriver>,
}

impl Explained {
    /// Whether git ignores it: a rule matched that isn't a negation, and git doesn't track it
    /// (ignore rules never apply to tracked files).
    pub fn ignored(&self) -> bool {
        !self.tracked && self.rule.as_ref().is_some_and(|r| !r.negated())
    }
}

/// Explains `path` (as typed: relative to the repository's top, or absolute inside it).
pub fn explain(repo: &Path, typed: &str) -> Result<Explained, String> {
    let path = clean_path(typed, &crate::remote::git_path(repo))?;
    let rule = rule_for(repo, &path)?;
    let attrs = attributes(repo, &path)?;
    let on_disk = crate::remote::join(repo, &path);
    let folder = crate::remote::try_is_dir(&on_disk).unwrap_or(false);
    let (tracked, eol) = if folder {
        (false, None)
    } else {
        let eol = line_endings(repo, &path)?;
        (eol.as_ref().is_some_and(|e| !e.index.is_empty()), eol)
    };
    let missing = !folder && eol.as_ref().is_none_or(|e| e.worktree.is_empty());
    let missing_drivers = missing_drivers(repo, &attrs);
    Ok(Explained {
        path,
        folder,
        missing,
        tracked,
        rule,
        attrs,
        eol,
        missing_drivers,
    })
}

/// A path as typed: trimmed of line ends, `\` as `/`, no leading `./`; an absolute path inside
/// the repository (whose top is `top`, as git names it) is made relative to it. Empty, `.`, a
/// line break, `..`, or an absolute path elsewhere are refused.
pub fn clean_path(typed: &str, top: &str) -> Result<String, String> {
    let trimmed = typed.trim_matches(['\n', '\r']);
    if trimmed.contains(['\n', '\r', '\0']) {
        return Err(crate::i18n::text("backend-a-path-can-t-hold-a-line-break-9fb70f").to_owned());
    }
    let mut path = trimmed.replace('\\', "/");
    let top = top.replace('\\', "/");
    let top = top.trim_end_matches('/');
    if !top.is_empty()
        && let Some(rest) = strip_top(&path, top).and_then(|r| r.strip_prefix('/'))
    {
        path = rest.to_owned();
    } else if path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') {
        return Err(crate::i18n::text(
            "backend-that-path-is-outside-the-repository-type-it-from-the-repository-s-88d6be",
        )
        .to_owned());
    }
    while let Some(rest) = path.strip_prefix("./") {
        path = rest.to_owned();
    }
    let path = path.trim_end_matches('/').to_owned();
    if path.trim().is_empty() || path == "." {
        return Err(crate::i18n::text(
            "backend-type-the-path-of-a-file-or-folder-in-the-repository-c824bc",
        )
        .to_owned());
    }
    if path.split('/').any(|part| part == "..") {
        return Err(
            crate::i18n::text("backend-the-path-must-be-inside-the-repository-no-6fd5ca")
                .to_owned(),
        );
    }
    Ok(path)
}

/// `path` without its leading `top`; on Windows (a drive letter) letter case doesn't count, as
/// there `C:/Code/r` and `c:/code/r` are the same folder.
fn strip_top<'a>(path: &'a str, top: &str) -> Option<&'a str> {
    let windows = top.as_bytes().get(1) == Some(&b':');
    let head = path.get(..top.len())?;
    let same = if windows {
        head.eq_ignore_ascii_case(top)
    } else {
        head == top
    };
    same.then(|| &path[top.len()..])
}

/// The rule deciding about `path`, from `check-ignore -v -n --no-index` (which also says it for
/// a tracked file, which git itself would no longer ignore).
fn rule_for(repo: &Path, path: &str) -> Result<Option<Rule>, String> {
    let out = run_ok_codes(
        Some(repo),
        &[
            "-c",
            "core.quotePath=false",
            "check-ignore",
            "-v",
            "-n",
            "--no-index",
            "--",
            // `./` keeps a name starting with `:` from being read as pathspec magic (which
            // check-ignore refuses to take literally).
            &format!("./{path}"),
        ],
        &[0, 1],
    )
    .map_err(|e| {
        crate::i18n::message(
            "backend-could-not-check-the-ignore-rules-e-fd6eab",
            &[("detail", (e).to_string().into())],
        )
    })?;
    Ok(out.lines().next().and_then(parse_check_ignore))
}

/// Reads one line of `check-ignore -v -n`: `source:line:pattern TAB path`, or `::` and the path
/// when no rule matches. The path is quoted by git when it holds a tab, so the last tab is the
/// separator; the line number, digits between two colons, ends the source (which may itself
/// hold a colon, as `C:/…`).
pub fn parse_check_ignore(line: &str) -> Option<Rule> {
    let (head, _path) = line.rsplit_once('\t')?;
    if head == "::" {
        return None;
    }
    let bytes = head.as_bytes();
    let mut i = 0;
    while let Some(off) = head[i..].find(':') {
        let colon = i + off;
        let digits_end = bytes[colon + 1..]
            .iter()
            .position(|b| !b.is_ascii_digit())
            .map(|n| colon + 1 + n);
        if let Some(end) = digits_end
            && end > colon + 1
            && bytes[end] == b':'
        {
            return Some(Rule {
                // Quoted by git when it holds a quote or a control character.
                source: super::flags::unquote(&head[..colon]),
                line: head[colon + 1..end].parse().ok()?,
                pattern: head[end + 1..].to_owned(),
            });
        }
        i = colon + 1;
    }
    None
}

/// Ignored files git doesn't track (`ls-files -o -i --exclude-standard`), at most `max`, and
/// how many there are in all.
pub fn ignored_files(repo: &Path, max: usize) -> Result<(Vec<String>, usize), String> {
    let out = run(
        Some(repo),
        &[
            "ls-files",
            "-o",
            "-i",
            "--exclude-standard",
            "--directory",
            "-z",
        ],
    )
    .map_err(|e| {
        crate::i18n::message(
            "backend-could-not-list-the-ignored-files-e-011113",
            &[("detail", (e).to_string().into())],
        )
    })?;
    let all = collapse(out.split('\0').filter(|p| !p.is_empty()));
    let total = all.len();
    Ok((all.into_iter().take(max).collect(), total))
}

/// Paths as `ls-files --directory` lists them, without those inside a folder listed whole
/// (git lists both a folder and some files in it).
pub fn collapse<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut sorted: Vec<&str> = paths.collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out: Vec<String> = Vec::with_capacity(sorted.len());
    let mut folder: Option<&str> = None;
    for path in sorted {
        if folder.is_some_and(|f| path.starts_with(f)) {
            continue;
        }
        if path.ends_with('/') {
            folder = Some(path);
        }
        out.push(path.to_owned());
    }
    out
}

#[cfg(test)]
mod tests;

mod attributes;
use attributes::attributes;
#[allow(unused_imports)] // Compatibility re-export.
pub use attributes::drivers_missing;
use attributes::line_endings;
pub use attributes::meaning;
use attributes::missing_drivers;
#[allow(unused_imports)] // Compatibility re-export.
pub use attributes::parse_check_attr;
#[allow(unused_imports)] // Compatibility re-export.
pub use attributes::parse_eol;
