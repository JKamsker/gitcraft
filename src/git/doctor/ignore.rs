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

/// Everything the doctor says about one path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Explained {
    pub path: String,
    pub tracked: bool,
    /// The last rule that matches it, `None` when none does.
    pub rule: Option<Rule>,
    pub attrs: Vec<Attr>,
    pub eol: Option<Eol>,
    /// Drivers an attribute names that the configuration doesn't define: (attribute, driver).
    pub missing_drivers: Vec<(String, String)>,
}

impl Explained {
    /// Whether git ignores it: a rule matched that isn't a negation, and git doesn't track it
    /// (ignore rules never apply to tracked files).
    pub fn ignored(&self) -> bool {
        !self.tracked && self.rule.as_ref().is_some_and(|r| !r.negated())
    }
}

/// Explains `path` (relative to the repository's top, `/` between names).
pub fn explain(repo: &Path, path: &str) -> Result<Explained, String> {
    let path = clean_path(path)?;
    let rule = rule_for(repo, &path)?;
    let attrs = attributes(repo, &path)?;
    let tracked = crate::git::is_tracked(repo, &path);
    let eol = if tracked
        || crate::remote::try_is_file(&crate::remote::join(repo, &path)).unwrap_or(false)
    {
        line_endings(repo, &path)?
    } else {
        None
    };
    let missing_drivers = missing_drivers(repo, &attrs);
    Ok(Explained {
        path,
        tracked,
        rule,
        attrs,
        eol,
        missing_drivers,
    })
}

/// A path as typed: trimmed of line ends, `\` as `/`, no leading `./` or `/`. A path with a line
/// break, or empty, or outside the repository (`..`) is refused.
pub fn clean_path(typed: &str) -> Result<String, String> {
    let trimmed = typed.trim_matches(['\n', '\r']);
    if trimmed.contains(['\n', '\r', '\0']) {
        return Err("A path can't hold a line break".to_owned());
    }
    let mut path = trimmed.replace('\\', "/");
    while let Some(rest) = path.strip_prefix("./").or_else(|| path.strip_prefix('/')) {
        path = rest.to_owned();
    }
    if path.trim().is_empty() {
        return Err("Type the path of a file in the repository".to_owned());
    }
    if path.split('/').any(|part| part == "..") {
        return Err("The path must be inside the repository (no ..)".to_owned());
    }
    Ok(path)
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
            path,
        ],
        &[0, 1],
    )
    .map_err(|e| format!("Could not check the ignore rules: {e}"))?;
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
                source: head[..colon].to_owned(),
                line: head[colon + 1..end].parse().ok()?,
                pattern: head[end + 1..].to_owned(),
            });
        }
        i = colon + 1;
    }
    None
}

/// Every attribute git gives `path` (`check-attr -a -z`).
fn attributes(repo: &Path, path: &str) -> Result<Vec<Attr>, String> {
    let out = run(Some(repo), &["check-attr", "-a", "-z", "--", path])
        .map_err(|e| format!("Could not read the attributes: {e}"))?;
    Ok(parse_check_attr(&out))
}

/// Reads `check-attr -z`: `path NUL attribute NUL value NUL` per attribute.
pub fn parse_check_attr(out: &str) -> Vec<Attr> {
    let mut fields = out.split('\0');
    let mut list = Vec::new();
    while let (Some(_path), Some(name), Some(value)) = (fields.next(), fields.next(), fields.next())
    {
        if name.is_empty() {
            continue;
        }
        list.push(Attr {
            name: name.to_owned(),
            value: value.to_owned(),
        });
    }
    list
}

/// `path`'s line endings (`ls-files --eol`); `None` when git says nothing about it.
fn line_endings(repo: &Path, path: &str) -> Result<Option<Eol>, String> {
    // Untracked files too (`-o`), as long as they aren't ignored.
    let out = run(
        Some(repo),
        &["ls-files", "--eol", "-c", "-o", "-z", "--", path],
    )
    .map_err(|e| format!("Could not read the line endings: {e}"))?;
    Ok(out.split('\0').find_map(parse_eol))
}

/// Reads one entry of `ls-files --eol -z`: `i/<eol> w/<eol> attr/<attrs> TAB path`.
pub fn parse_eol(entry: &str) -> Option<Eol> {
    let (info, _path) = entry.split_once('\t')?;
    let mut eol = Eol::default();
    let attr_at = info.find("attr/");
    let (heads, attr) = match attr_at {
        Some(at) => (&info[..at], info[at + 5..].trim()),
        None => (info, ""),
    };
    for word in heads.split_whitespace() {
        if let Some(i) = word.strip_prefix("i/") {
            eol.index = i.to_owned();
        } else if let Some(w) = word.strip_prefix("w/") {
            eol.worktree = w.to_owned();
        }
    }
    eol.attr = attr.to_owned();
    Some(eol)
}

/// Merge, diff and filter drivers named by `attrs` that no `merge.<name>.driver`,
/// `diff.<name>.*` or `filter.<name>.*` defines: git then quietly does without them.
fn missing_drivers(repo: &Path, attrs: &[Attr]) -> Vec<(String, String)> {
    attrs
        .iter()
        .filter_map(|a| {
            let keys: &[&str] = match a.name.as_str() {
                "merge" if !builtin_merge(&a.value) => &["driver"],
                "diff" if !builtin_value(&a.value) => &["textconv", "command", "xfuncname"],
                "filter" if !builtin_value(&a.value) => &["clean", "smudge", "process"],
                _ => return None,
            };
            let defined = keys.iter().any(|k| {
                let key = format!("{}.{}.{k}", a.name, a.value);
                run(Some(repo), &["config", "--get", "--", &key])
                    .is_ok_and(|v| !v.trim().is_empty())
            });
            (!defined).then(|| (a.name.clone(), a.value.clone()))
        })
        .collect()
}

/// `set` / `unset` / `unspecified`: no driver named.
fn builtin_value(value: &str) -> bool {
    matches!(value, "set" | "unset" | "unspecified")
}

/// The merge drivers git has itself.
fn builtin_merge(value: &str) -> bool {
    builtin_value(value) || matches!(value, "text" | "binary" | "union")
}

/// What the attributes mean, in words: one line per thing they decide.
pub fn meaning(attrs: &[Attr]) -> Vec<String> {
    let get = |name: &str| {
        attrs
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value.as_str())
    };
    let mut out = Vec::new();
    match get("text") {
        Some("set") => {
            out.push("Text: git normalises its line endings to LF when storing it.".to_owned())
        }
        Some("unset") => out.push("Binary: git never converts its line endings.".to_owned()),
        Some("auto") => out.push(
            "text=auto: git decides from its contents whether it is text, and normalises text."
                .to_owned(),
        ),
        _ => {}
    }
    match get("eol") {
        Some("lf") => out.push("Checked out with LF line endings.".to_owned()),
        Some("crlf") => out.push("Checked out with CRLF line endings.".to_owned()),
        _ => {}
    }
    if get("binary") == Some("set") {
        out.push("binary: no diff, no merge, no line-ending conversion.".to_owned());
    }
    if get("filter") == Some("lfs") {
        out.push("Stored in Git LFS: the repository keeps a pointer, LFS the contents.".to_owned());
    }
    match get("merge") {
        Some("unset") => out.push(
            "merge unset: any change on both sides is a conflict, the file is never merged."
                .to_owned(),
        ),
        Some("union") => {
            out.push("merge=union: both sides' lines are kept, without conflicts.".to_owned())
        }
        Some(v) if !builtin_merge(v) => out.push(format!("Merged with the {v} merge driver.")),
        _ => {}
    }
    match get("diff") {
        Some("unset") => out.push("diff unset: diffs say only \"Binary files differ\".".to_owned()),
        Some(v) if !builtin_value(v) => out.push(format!("Diffed with the {v} diff driver.")),
        _ => {}
    }
    if let Some(enc) = get("working-tree-encoding").filter(|v| !builtin_value(v)) {
        out.push(format!(
            "Kept as {enc} in the working tree, UTF-8 in the repository."
        ));
    }
    if get("export-ignore") == Some("set") {
        out.push("Left out of archives (git archive).".to_owned());
    }
    out
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
    .map_err(|e| format!("Could not list the ignored files: {e}"))?;
    let all: Vec<&str> = out.split('\0').filter(|p| !p.is_empty()).collect();
    let total = all.len();
    Ok((
        all.into_iter().take(max).map(str::to_owned).collect(),
        total,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_check_ignore_lines() {
        assert_eq!(
            parse_check_ignore("sub/.gitignore:2:!keep.log\tsub/keep.log"),
            Some(Rule {
                source: "sub/.gitignore".into(),
                line: 2,
                pattern: "!keep.log".into(),
            })
        );
        assert!(parse_check_ignore("::\ta.log").is_none());
        // A Windows path, a pattern with colons and a quoted path with a tab in it.
        let rule = parse_check_ignore("C:/Users/me/.config/git/ignore:12:a:b:*.x\t\"we\\tird\"");
        assert_eq!(
            rule,
            Some(Rule {
                source: "C:/Users/me/.config/git/ignore".into(),
                line: 12,
                pattern: "a:b:*.x".into(),
            })
        );
        assert!(parse_check_ignore("").is_none());
        assert!(parse_check_ignore("no tab here").is_none());
    }

    #[test]
    fn reads_attributes_and_line_endings() {
        let attrs =
            parse_check_attr("a.png\0binary\0set\0a.png\0diff\0unset\0a.png\0text\0unset\0");
        assert_eq!(attrs.len(), 3);
        assert_eq!(attrs[0].name, "binary");
        assert!(parse_check_attr("").is_empty());
        let eol = parse_eol("i/lf    w/crlf  attr/text=auto eol=lf \tdir/é b.txt");
        assert_eq!(
            eol,
            Some(Eol {
                index: "lf".into(),
                worktree: "crlf".into(),
                attr: "text=auto eol=lf".into(),
            })
        );
        let untracked = parse_eol("        w/lf    attr/                 \tnew.txt");
        assert_eq!(
            untracked.map(|e| (e.index, e.worktree)),
            Some((String::new(), "lf".into()))
        );
    }

    #[test]
    fn typed_paths_are_cleaned_or_refused() {
        assert_eq!(clean_path("./src\\a b.rs\n"), Ok("src/a b.rs".to_owned()));
        assert_eq!(clean_path("/é.txt"), Ok("é.txt".to_owned()));
        assert!(clean_path("").is_err());
        assert!(clean_path("  ").is_err());
        assert!(clean_path("a\nb").is_err());
        assert!(clean_path("../outside").is_err());
        // A name that only starts with dots is a name.
        assert_eq!(clean_path("..hidden"), Ok("..hidden".to_owned()));
    }

    #[test]
    fn meanings_in_words() {
        let attr = |n: &str, v: &str| Attr {
            name: n.into(),
            value: v.into(),
        };
        let words = meaning(&[
            attr("filter", "lfs"),
            attr("merge", "unset"),
            attr("text", "unset"),
        ]);
        assert!(words.iter().any(|w| w.contains("LFS")));
        assert!(words.iter().any(|w| w.contains("never merged")));
        assert!(words.iter().any(|w| w.starts_with("Binary")));
        assert!(meaning(&[]).is_empty());
    }
}
