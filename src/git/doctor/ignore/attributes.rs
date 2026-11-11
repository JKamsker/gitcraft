//! Attribute and line-ending inspection with configured driver explanations.

use super::*;

/// Every attribute git gives `path` (`check-attr -a -z`).
pub(super) fn attributes(repo: &Path, path: &str) -> Result<Vec<Attr>, String> {
    let out = run(Some(repo), &["check-attr", "-a", "-z", "--", path]).map_err(|e| {
        crate::i18n::message(
            "backend-could-not-read-the-attributes-e-9bbcd0",
            &[("detail", (e).to_string().into())],
        )
    })?;
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
pub(super) fn line_endings(repo: &Path, path: &str) -> Result<Option<Eol>, String> {
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
    .map_err(|e| {
        crate::i18n::message(
            "backend-could-not-read-the-line-endings-e-8b6f2e",
            &[("detail", (e).to_string().into())],
        )
    })?;
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
pub(super) const BUILTIN_DIFF: &[&str] = &[
    "ada", "bash", "bibtex", "cpp", "csharp", "css", "dts", "elixir", "fortran", "fountain",
    "golang", "html", "java", "kotlin", "markdown", "matlab", "objc", "pascal", "perl", "php",
    "python", "r", "ruby", "rust", "scheme", "tex",
];

/// Merge, diff and filter drivers named by `attrs` that the configuration doesn't define: git
/// then merges or diffs as usual, or, for a required filter, refuses the file. One read of the
/// configuration for all of them.
pub(super) fn missing_drivers(repo: &Path, attrs: &[Attr]) -> Vec<MissingDriver> {
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
pub(super) fn builtin_value(value: &str) -> bool {
    matches!(value, "set" | "unset" | "unspecified")
}

/// The merge drivers git has itself.
pub(super) fn builtin_merge(value: &str) -> bool {
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
        Some("set") => out.push(
            crate::i18n::text(
                "backend-text-git-normalises-its-line-endings-to-lf-when-storing-it-e5c6d9",
            )
            .to_owned(),
        ),
        Some("unset") if get("binary") != Some("set") => out.push(
            crate::i18n::text(
                "backend-text-git-never-converts-its-line-endings-it-is-still-diffed-and-m-0869b0",
            )
            .to_owned(),
        ),
        Some("auto") => out.push(
            crate::i18n::text(
                "backend-text-auto-git-decides-from-its-contents-whether-it-is-text-and-no-f26011",
            )
            .to_owned(),
        ),
        _ => {}
    }
    match get("eol") {
        Some("lf") => out.push(
            crate::i18n::text("backend-text-checked-out-with-lf-line-endings-20e453").to_owned(),
        ),
        Some("crlf") => out.push(
            crate::i18n::text("backend-text-checked-out-with-crlf-line-endings-947339").to_owned(),
        ),
        _ => {}
    }
    if get("binary") == Some("set") {
        out.push(
            crate::i18n::text("backend-binary-no-diff-no-merge-no-line-ending-conversion-7c78ce")
                .to_owned(),
        );
    }
    match get("filter") {
        Some("lfs") => out.push(
            crate::i18n::text(
                "backend-stored-in-git-lfs-the-repository-keeps-a-pointer-lfs-the-contents-e81097",
            )
            .to_owned(),
        ),
        Some(v) if !builtin_value(v) => out.push(crate::i18n::message(
            "backend-run-through-the-v-filter-when-added-and-checked-out-87161e",
            &[("value", (v).to_string().into())],
        )),
        _ => {}
    }
    let binary = get("binary") == Some("set");
    match get("merge") {
        Some("unset") if !binary => out.push(
            crate::i18n::text(
                "backend-merge-any-change-on-both-sides-is-a-conflict-your-version-is-left-521217",
            )
            .to_owned(),
        ),
        Some("union") => out.push(
            crate::i18n::text(
                "backend-merge-union-both-sides-lines-are-kept-in-no-set-order-and-without-1de3ba",
            )
            .to_owned(),
        ),
        Some(v) if !builtin_merge(v) => out.push(crate::i18n::message(
            "backend-merged-with-the-v-merge-driver-22c9b0",
            &[("value", (v).to_string().into())],
        )),
        _ => {}
    }
    match get("diff") {
        Some("unset") if !binary => out.push(
            crate::i18n::text("backend-diff-diffs-say-only-binary-files-differ-da8247").to_owned(),
        ),
        Some(v) if BUILTIN_DIFF.contains(&v) => out.push(crate::i18n::message(
            "backend-diffed-with-git-s-own-v-patterns-function-names-in-hunks-e85843",
            &[("value", (v).to_string().into())],
        )),
        Some(v) if !builtin_value(v) => out.push(crate::i18n::message(
            "backend-diffed-with-the-v-diff-driver-5f19be",
            &[("value", (v).to_string().into())],
        )),
        _ => {}
    }
    if let Some(enc) = get("working-tree-encoding").filter(|v| !builtin_value(v)) {
        out.push(crate::i18n::message(
            "backend-kept-as-enc-in-the-working-tree-utf-8-in-the-repository-4ef2b1",
            &[("enc", (enc).to_string().into())],
        ));
    }
    if get("export-ignore") == Some("set") {
        out.push(crate::i18n::text("backend-left-out-of-archives-git-archive-a96899").to_owned());
    }
    out
}
