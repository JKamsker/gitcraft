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
        return Err("A path can't hold a line break".to_owned());
    }
    let mut path = trimmed.replace('\\', "/");
    let top = top.replace('\\', "/");
    let top = top.trim_end_matches('/');
    if !top.is_empty()
        && let Some(rest) = strip_top(&path, top).and_then(|r| r.strip_prefix('/'))
    {
        path = rest.to_owned();
    } else if path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') {
        return Err(
            "That path is outside the repository: type it from the repository's top".to_owned(),
        );
    }
    while let Some(rest) = path.strip_prefix("./") {
        path = rest.to_owned();
    }
    let path = path.trim_end_matches('/').to_owned();
    if path.trim().is_empty() || path == "." {
        return Err("Type the path of a file or folder in the repository".to_owned());
    }
    if path.split('/').any(|part| part == "..") {
        return Err("The path must be inside the repository (no ..)".to_owned());
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

/// `path`'s line endings (`ls-files --eol`, a file's name taken literally); `None` when git
/// says nothing about it (no such file). Untracked and ignored files too (`-o`, no excludes).
fn line_endings(repo: &Path, path: &str) -> Result<Option<Eol>, String> {
    let out = run(
        Some(repo),
        &[
            crate::git::LITERAL_PATHS,
            "ls-files",
            "--eol",
            "-c",
            "-o",
            "-z",
            "--",
            path,
        ],
    )
    .map_err(|e| format!("Could not read the line endings: {e}"))?;
    Ok(out
        .split('\0')
        .filter(|entry| entry.split_once('\t').is_some_and(|(_, p)| p == path))
        .find_map(parse_eol))
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

/// The diff drivers git has itself (userdiff's built-in patterns).
const BUILTIN_DIFF: &[&str] = &[
    "ada", "bash", "bibtex", "cpp", "csharp", "css", "dts", "elixir", "fortran", "fountain",
    "golang", "html", "java", "kotlin", "markdown", "matlab", "objc", "pascal", "perl", "php",
    "python", "r", "ruby", "rust", "scheme", "tex",
];

/// Merge, diff and filter drivers named by `attrs` that the configuration doesn't define: git
/// then merges or diffs as usual, or, for a required filter, refuses the file. One read of the
/// configuration for all of them.
fn missing_drivers(repo: &Path, attrs: &[Attr]) -> Vec<MissingDriver> {
    let named: Vec<&Attr> = attrs
        .iter()
        .filter(|a| match a.name.as_str() {
            "merge" => !builtin_merge(&a.value),
            "diff" => !builtin_value(&a.value) && !BUILTIN_DIFF.contains(&a.value.as_str()),
            "filter" => !builtin_value(&a.value),
            _ => false,
        })
        .collect();
    if named.is_empty() {
        return Vec::new();
    }
    let config = run_ok_codes(
        Some(repo),
        &["config", "-z", "--get-regexp", r"^(merge|diff|filter)\."],
        &[0, 1],
    )
    .unwrap_or_default();
    drivers_missing(&named, &config)
}

/// [`missing_drivers`] from the configuration read (`config -z --get-regexp`: `key LF value`
/// per entry; keys lowercase but for their middle part).
pub fn drivers_missing(named: &[&Attr], config: &str) -> Vec<MissingDriver> {
    let entries: Vec<(&str, &str)> = config
        .split('\0')
        .filter(|e| !e.is_empty())
        .map(|e| e.split_once('\n').unwrap_or((e, "")))
        .collect();
    named
        .iter()
        .filter_map(|a| {
            let prefix = format!("{}.{}.", a.name, a.value);
            let keys: Vec<(&str, &str)> = entries
                .iter()
                .filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?, *v)))
                .collect();
            let wanted: &[&str] = match a.name.as_str() {
                "merge" => &["driver"],
                "filter" => &["clean", "smudge", "process"],
                // Any key defines a diff driver (textconv, funcname, binary, wordregex…).
                _ => &[],
            };
            let defined = if wanted.is_empty() {
                !keys.is_empty()
            } else {
                keys.iter()
                    .any(|(k, v)| wanted.contains(k) && !v.trim().is_empty())
            };
            let required = a.name == "filter"
                && keys.iter().any(|(k, v)| {
                    *k == "required" && matches!(v.trim(), "true" | "yes" | "on" | "1")
                });
            (!defined).then(|| MissingDriver {
                attr: a.name.clone(),
                name: a.value.clone(),
                required,
            })
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
        Some("unset") if get("binary") != Some("set") => out.push(
            "-text: git never converts its line endings (it is still diffed and merged as text)."
                .to_owned(),
        ),
        Some("auto") => out.push(
            "text=auto: git decides from its contents whether it is text, and normalises text."
                .to_owned(),
        ),
        _ => {}
    }
    match get("eol") {
        Some("lf") => out.push("Text, checked out with LF line endings.".to_owned()),
        Some("crlf") => out.push("Text, checked out with CRLF line endings.".to_owned()),
        _ => {}
    }
    if get("binary") == Some("set") {
        out.push("binary: no diff, no merge, no line-ending conversion.".to_owned());
    }
    match get("filter") {
        Some("lfs") => out.push(
            "Stored in Git LFS: the repository keeps a pointer, LFS the contents.".to_owned(),
        ),
        Some(v) if !builtin_value(v) => out.push(format!(
            "Run through the {v} filter when added and checked out."
        )),
        _ => {}
    }
    let binary = get("binary") == Some("set");
    match get("merge") {
        Some("unset") if !binary => out.push(
            "-merge: any change on both sides is a conflict; your version is left in the working \
             tree."
                .to_owned(),
        ),
        Some("union") => out.push(
            "merge=union: both sides' lines are kept, in no set order and without conflicts: \
             check the result."
                .to_owned(),
        ),
        Some(v) if !builtin_merge(v) => out.push(format!("Merged with the {v} merge driver.")),
        _ => {}
    }
    match get("diff") {
        Some("unset") if !binary => {
            out.push("-diff: diffs say only \"Binary files differ\".".to_owned())
        }
        Some(v) if BUILTIN_DIFF.contains(&v) => out.push(format!(
            "Diffed with git's own {v} patterns (function names in hunks)."
        )),
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
        let top = "/home/me/my repo";
        assert_eq!(
            clean_path("./src\\a b.rs\n", top),
            Ok("src/a b.rs".to_owned())
        );
        assert_eq!(clean_path("src/", top), Ok("src".to_owned()));
        // Absolute inside the repository: made relative; elsewhere: refused.
        assert_eq!(
            clean_path("/home/me/my repo/é.txt", top),
            Ok("é.txt".to_owned())
        );
        assert!(clean_path("/home/me/other/x", top).is_err());
        assert!(clean_path("/home/me/my repository/x", top).is_err());
        assert!(clean_path("C:\\x\\y", top).is_err());
        assert_eq!(
            clean_path("C:\\code\\r\\a.txt", "C:/code/r"),
            Ok("a.txt".to_owned())
        );
        // On Windows letter case doesn't count; elsewhere it does.
        assert_eq!(
            clean_path("c:\\Code\\R\\a.txt", "C:/code/r"),
            Ok("a.txt".to_owned())
        );
        assert!(clean_path("/home/me/My Repo/a.txt", top).is_err());
        // A name with characters wider than a byte where the top would end is no panic.
        assert!(clean_path("/home/me/my rép/x", top).is_err());
        for refused in ["", "  ", ".", "./", "a\nb", "../outside", "a/../../b"] {
            assert!(clean_path(refused, top).is_err(), "{refused:?}");
        }
        // A name that only starts with dots is a name; one starting with `:` too.
        assert_eq!(clean_path("..hidden", top), Ok("..hidden".to_owned()));
        assert_eq!(clean_path(":x", top), Ok(":x".to_owned()));
    }

    #[test]
    fn drivers_are_checked_against_the_configuration() {
        let attr = |n: &str, v: &str| Attr {
            name: n.into(),
            value: v.into(),
        };
        let named = [
            attr("merge", "ours-driver"),
            attr("diff", "word"),
            attr("filter", "crypt"),
            attr("filter", "lfs"),
        ];
        let refs: Vec<&Attr> = named.iter().collect();
        let config = "merge.ours-driver.name\nours\0diff.word.wordregex\n.\0\
                      filter.crypt.required\ntrue\0filter.lfs.clean\ngit-lfs clean -- %f\0";
        let missing = drivers_missing(&refs, config);
        // The merge driver has a name but no driver command; the diff driver is defined by
        // any key; the required filter has nothing to run; LFS is set up.
        assert_eq!(
            missing,
            [
                MissingDriver {
                    attr: "merge".into(),
                    name: "ours-driver".into(),
                    required: false,
                },
                MissingDriver {
                    attr: "filter".into(),
                    name: "crypt".into(),
                    required: true,
                },
            ]
        );
        assert_eq!(drivers_missing(&refs, "").len(), 4);
    }

    #[test]
    fn folders_listed_whole_hide_their_files() {
        let listed = [
            "e/",
            "e/only.log",
            "a.log",
            "build/",
            "build/x/",
            "buildx.log",
            "a.log",
        ];
        assert_eq!(
            collapse(listed.into_iter()),
            ["a.log", "build/", "buildx.log", "e/"]
        );
        assert!(collapse(std::iter::empty()).is_empty());
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
        assert!(words.iter().any(|w| w.starts_with("-merge")));
        assert!(words.iter().any(|w| w.starts_with("-text")));
        // binary sets -diff -merge -text too: said once.
        let binary = meaning(&[
            attr("binary", "set"),
            attr("diff", "unset"),
            attr("merge", "unset"),
            attr("text", "unset"),
        ]);
        assert_eq!(binary.len(), 1, "{binary:?}");
        assert!(meaning(&[attr("diff", "rust")])[0].contains("git's own rust"));
        assert!(meaning(&[]).is_empty());
    }
}
