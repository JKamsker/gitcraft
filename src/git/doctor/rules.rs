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
    /// It isn't a file of the repository: saving asks first.
    pub outside: bool,
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
            outside: false,
        });
    }
    out.extend(excludes_file(repo));
    Ok(out)
}

/// The excludes file of `core.excludesFile` (labelled by the scope that set it), or git's
/// default one (`$XDG_CONFIG_HOME/git/ignore`, `~/.config/git/ignore`) when it isn't set.
fn excludes_file(repo: &Path) -> Option<RulesFile> {
    let set = run(
        Some(repo),
        &[
            "config",
            "--show-scope",
            "--path",
            "--get",
            "core.excludesFile",
        ],
    )
    .ok();
    let line = set.as_deref().map(|l| l.trim_end_matches(['\n', '\r']));
    let (scope, printed) = match line.and_then(|l| l.split_once('\t')) {
        Some((scope, path)) if !path.is_empty() => (Some(scope.to_owned()), path.to_owned()),
        _ => (None, default_excludes(repo)?),
    };
    let kind = match scope.as_deref() {
        None => "Your own (git's default place), every repository",
        Some("global" | "system") => "Your own (core.excludesFile), every repository",
        Some(_) => "Named by this repository's config (core.excludesFile)",
    };
    Some(RulesFile {
        label: printed.clone(),
        location: crate::remote::resolve(repo, &printed),
        base: String::new(),
        kind,
        outside: true,
    })
}

/// Where git looks for the excludes file when `core.excludesFile` isn't set, on the
/// repository's machine.
fn default_excludes(repo: &Path) -> Option<String> {
    let script = "printf '%s' \"${XDG_CONFIG_HOME:-$HOME/.config}/git/ignore\"";
    match crate::remote::parse(repo) {
        Some(t) => crate::remote::exec(
            &t.host,
            &crate::remote::Request {
                script: script.to_owned(),
                ..Default::default()
            },
        )
        .ok()
        .filter(|out| out.code == 0)
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .filter(|p| p.starts_with('/')),
        None => {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .or_else(|| std::env::var_os("USERPROFILE"))
                        .map(|h| PathBuf::from(h).join(".config"))
                })?;
            Some(
                config
                    .join("git")
                    .join("ignore")
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }
}

fn in_repo(repo: &Path, rel: &str) -> RulesFile {
    let base = rel
        .rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_owned());
    RulesFile {
        label: rel.to_owned(),
        location: crate::remote::join(repo, rel),
        kind: if base.is_empty() {
            "The repository's, shared once committed"
        } else {
            "This folder and below, shared once committed"
        },
        base,
        outside: false,
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
            "{} changed on disk since you opened it. Read it again (your changes stay in the \
             editor) before saving.",
            file.label
        ));
    }
    if let Some(parent) = file.location.parent() {
        crate::remote::create_dir_all(parent)
            .map_err(|e| format!("Could not make the folder of {}: {e}", file.label))?;
    }
    // Checked again right before writing: here `write` follows a link (elsewhere it checks
    // and writes in one script).
    if crate::remote::try_is_link(&file.location).unwrap_or(false) {
        return Err(format!("{} is a symbolic link: not changed", file.label));
    }
    let text = same_line_endings(expected, text);
    crate::remote::write(&file.location, text.as_bytes())
        .map_err(|e| format!("Could not write {}: {e}", file.label))
}

/// `text` with the line endings `original` has: a CRLF file stays CRLF (the editor's new lines
/// are LF), an LF one stays LF.
pub fn same_line_endings(original: &str, text: &str) -> String {
    let lf = text.replace("\r\n", "\n");
    if original.contains("\r\n") {
        lf.replace('\n', "\r\n")
    } else {
        lf
    }
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
            // As git reads it: unescaped trailing spaces dropped, then one trailing `/`.
            let body = trim_trailing_spaces(body);
            let anchored = body.strip_suffix('/').unwrap_or(body).contains('/');
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

/// `pattern` without its trailing spaces, but those escaped with `\\`.
fn trim_trailing_spaces(pattern: &str) -> &str {
    let mut end = pattern.len();
    while pattern[..end].ends_with(' ') {
        let before = &pattern[..end - 1];
        let backslashes = before.len() - before.trim_end_matches('\\').len();
        if backslashes % 2 == 1 {
            break;
        }
        end -= 1;
    }
    &pattern[..end]
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
    let old_rules = RulesTemp::new(repo, &from_top(old, &file.base))?;
    let new_rules = RulesTemp::new(repo, &from_top(new, &file.base))?;
    let before = matched(repo, &old_rules, "-o")?;
    let after = matched(repo, &new_rules, "-o")?;
    let tracked = matched(repo, &new_rules, "-c")?;
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

/// Rules written to a temporary file on the repository's machine for `--exclude-from`,
/// removed once dropped. No file when there is no rule.
struct RulesTemp<'a> {
    repo: &'a Path,
    file: Option<String>,
}

impl<'a> RulesTemp<'a> {
    fn new(repo: &'a Path, rules: &str) -> Result<Self, String> {
        let empty = rules
            .lines()
            .all(|l| l.trim().is_empty() || l.starts_with('#'));
        let file = if empty {
            None
        } else {
            Some(
                crate::git::save_patch_for(repo, rules)
                    .map_err(|e| format!("Could not prepare the preview: {e}"))?,
            )
        };
        Ok(Self { repo, file })
    }
}

impl Drop for RulesTemp<'_> {
    fn drop(&mut self) {
        let Some(file) = &self.file else {
            return;
        };
        match crate::remote::parse(self.repo) {
            Some(t) => {
                let _ = crate::remote::exec(
                    &t.host,
                    &crate::remote::Request {
                        script: format!("rm -f -- {}", crate::remote::sh_quote(file)),
                        ..Default::default()
                    },
                );
            }
            None => {
                let _ = std::fs::remove_file(file);
            }
        }
    }
}

/// The files the rules match: `-o` untracked ones (a folder ignored whole as one entry), `-c`
/// tracked ones.
fn matched(
    repo: &Path,
    rules: &RulesTemp,
    which: &str,
) -> Result<std::collections::BTreeSet<String>, String> {
    let Some(file) = &rules.file else {
        return Ok(Default::default());
    };
    let from = format!("--exclude-from={file}");
    let mut args = vec!["ls-files", "-z", which, "-i", from.as_str()];
    if which == "-o" {
        args.push("--directory");
    }
    let out = run(Some(repo), &args).map_err(|e| format!("Could not preview the rules: {e}"))?;
    Ok(
        super::ignore::collapse(out.split('\0').filter(|p| !p.is_empty()))
            .into_iter()
            .collect(),
    )
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
        // Trailing spaces go before the slash is looked at; an escaped one stays.
        assert_eq!(from_top("foo/ \n", "s"), "/s/**/foo/\n");
        assert_eq!(from_top("a\\ \n", "s"), "/s/**/a\\ \n");
    }

    #[test]
    fn line_endings_are_kept() {
        assert_eq!(
            same_line_endings("a\r\nb\r\n", "a\r\nb\nc\n"),
            "a\r\nb\r\nc\r\n"
        );
        assert_eq!(same_line_endings("a\n", "a\r\nb\n"), "a\nb\n");
        assert_eq!(same_line_endings("", "x\n"), "x\n");
    }
}
