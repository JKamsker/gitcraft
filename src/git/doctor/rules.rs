//! Editing ignore rules: the files they are in (`.gitignore` at the top and in folders,
//! `.git/info/exclude`, `core.excludesFile`), and a preview of what a change to one would ignore
//! or stop ignoring before it is saved.

use std::path::{Path, PathBuf};

use crate::git::run;

/// A file of ignore rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RulesFile {
    /// As the list names it: `.gitignore`, `src/.gitignore`, `.git/info/exclude`…
    pub label: String,
    /// Where it is, as this app reads and writes files (on the repository's machine).
    pub location: PathBuf,
    /// The folder its rules are relative to, from the repository's top (`""` for the top and
    /// for files that aren't in a folder of the repository).
    pub base: String,
    /// What it is for, in a few words.
    pub kind: &'static str,
}

/// Every file of ignore rules the repository uses, the top `.gitignore` first (listed even when
/// it doesn't exist yet, so it can be made).
pub fn files(repo: &Path) -> Result<Vec<RulesFile>, String> {
    let listed = run(
        Some(repo),
        &[
            "ls-files",
            "-z",
            "-c",
            "-o",
            "--exclude-standard",
            "--",
            ":(glob)**/.gitignore",
        ],
    )
    .map_err(|e| format!("Could not list the .gitignore files: {e}"))?;
    let mut out = vec![in_repo(repo, ".gitignore")];
    let mut nested: Vec<&str> = listed
        .split('\0')
        .filter(|p| !p.is_empty() && *p != ".gitignore")
        .collect();
    nested.sort_unstable();
    nested.dedup();
    out.extend(nested.into_iter().map(|p| in_repo(repo, p)));
    if let Ok(printed) = run(Some(repo), &["rev-parse", "--git-path", "info/exclude"]) {
        let printed = printed.trim_end_matches(['\n', '\r']);
        out.push(RulesFile {
            label: printed.to_owned(),
            location: crate::remote::resolve(repo, printed),
            base: String::new(),
            kind: "This clone only, never committed",
        });
    }
    if let Ok(printed) = run(
        Some(repo),
        &["config", "--path", "--get", "core.excludesFile"],
    ) {
        let printed = printed.trim_end_matches(['\n', '\r']);
        if !printed.is_empty() {
            out.push(RulesFile {
                label: printed.to_owned(),
                location: crate::remote::resolve(repo, printed),
                base: String::new(),
                kind: "Your own (core.excludesFile), every repository",
            });
        }
    }
    Ok(out)
}

fn in_repo(repo: &Path, rel: &str) -> RulesFile {
    let base = rel
        .rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_owned());
    RulesFile {
        label: rel.to_owned(),
        location: crate::remote::join(repo, rel),
        kind: if base.is_empty() {
            "The repository's, committed"
        } else {
            "This folder's and below, committed"
        },
        base,
    }
}

/// A rules file's text, empty when it doesn't exist yet.
pub fn read(file: &RulesFile) -> Result<String, String> {
    match crate::remote::read_to_string(&file.location) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("Could not read {}: {e}", file.label)),
    }
}

/// Saves `text` as the file's contents, when it still holds `expected` (what was read): an
/// edit made meanwhile elsewhere is not written over. Never through a symbolic link.
pub fn save(file: &RulesFile, expected: &str, text: &str) -> Result<(), String> {
    match crate::remote::try_is_link(&file.location) {
        Ok(false) => {}
        Ok(true) => return Err(format!("{} is a symbolic link: not changed", file.label)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Could not check {}: {e}", file.label)),
    }
    if read(file)? != expected {
        return Err(format!(
            "{} changed since it was read: read it again before saving",
            file.label
        ));
    }
    if let Some(parent) = file.location.parent() {
        crate::remote::create_dir_all(parent)
            .map_err(|e| format!("Could not make the folder of {}: {e}", file.label))?;
    }
    crate::remote::write(&file.location, text.as_bytes())
        .map_err(|e| format!("Could not write {}: {e}", file.label))
}

/// The rules of a file in folder `base`, rewritten to mean the same from the repository's
/// top, as `--exclude-from` reads them: a pattern with a `/` (but at its end) is relative to
/// `base`, one without matches at any depth below it.
pub fn from_top(text: &str, base: &str) -> String {
    if base.is_empty() {
        return text.to_owned();
    }
    let base = escape_glob(base);
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            out.push_str(line);
        } else {
            let (neg, body) = match line.strip_prefix('!') {
                Some(rest) => ("!", rest),
                None => ("", line),
            };
            let anchored = body.trim_end_matches('/').contains('/');
            if anchored {
                out.push_str(&format!("{neg}/{base}/{}", body.trim_start_matches('/')));
            } else {
                out.push_str(&format!("{neg}/{base}/**/{body}"));
            }
        }
        out.push('\n');
    }
    out
}

/// A folder name as a pattern matching only itself.
fn escape_glob(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '*' | '?' | '[' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// What saving a change to a rules file would do, by that file's rules alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preview {
    /// Untracked files it would start ignoring.
    pub newly_ignored: Vec<String>,
    /// Untracked files it would stop ignoring.
    pub no_longer_ignored: Vec<String>,
    /// Tracked files the new rules match: git keeps tracking them, the rules don't apply.
    pub tracked_matching: Vec<String>,
}

/// The difference between `old` and `new` as the rules of `file`. Other rules files are left
/// out: one of them may still ignore (or un-ignore) a path listed here.
pub fn preview(repo: &Path, file: &RulesFile, old: &str, new: &str) -> Result<Preview, String> {
    let before = matched(repo, &from_top(old, &file.base), "-o")?;
    let after = matched(repo, &from_top(new, &file.base), "-o")?;
    let tracked = matched(repo, &from_top(new, &file.base), "-c")?;
    Ok(Preview {
        newly_ignored: after
            .iter()
            .filter(|p| !before.contains(*p))
            .cloned()
            .collect(),
        no_longer_ignored: before
            .iter()
            .filter(|p| !after.contains(*p))
            .cloned()
            .collect(),
        tracked_matching: tracked.into_iter().collect(),
    })
}

/// The files (`-o`: untracked, `-c`: tracked) `rules` match, read by git from a temporary file
/// on the repository's machine.
fn matched(
    repo: &Path,
    rules: &str,
    which: &str,
) -> Result<std::collections::BTreeSet<String>, String> {
    if rules
        .lines()
        .all(|l| l.trim().is_empty() || l.starts_with('#'))
    {
        return Ok(Default::default());
    }
    let file = crate::git::save_patch_for(repo, rules)
        .map_err(|e| format!("Could not prepare the preview: {e}"))?;
    let from = format!("--exclude-from={file}");
    let out = run(Some(repo), &["ls-files", "-z", which, "-i", &from])
        .map_err(|e| format!("Could not preview the rules: {e}"))?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_rules_read_from_the_top() {
        let text = "# comment\n*.log\n!keep.log\n/only-here\nbuild/\ndeep/x.txt\n\n";
        assert_eq!(
            from_top(text, "sub dir"),
            "# comment\n/sub dir/**/*.log\n!/sub dir/**/keep.log\n/sub dir/only-here\n\
             /sub dir/**/build/\n/sub dir/deep/x.txt\n\n"
        );
        assert_eq!(from_top(text, ""), text);
        assert_eq!(from_top("a\r\n", "[x]"), "/\\[x]/**/a\n");
        assert_eq!(from_top("", "d"), "");
    }
}
