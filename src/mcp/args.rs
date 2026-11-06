//! A tool call's arguments, read and checked: what an agent passes goes to git as it is, so it
//! must be what it says it is. A path stays inside the repository, a revision can't be taken
//! for an option, a number stays within what the tool allows.

use serde_json::{Map, Value};

/// The arguments of one call.
pub struct Args<'a> {
    map: &'a Map<String, Value>,
}

impl<'a> Args<'a> {
    pub fn new(map: &'a Map<String, Value>) -> Self {
        Self { map }
    }

    /// Fails on an argument the tool doesn't take: a typo would otherwise be ignored, and the
    /// call do something else than asked.
    pub fn only(&self, known: &[&str]) -> Result<(), String> {
        match self.map.keys().find(|k| !known.contains(&k.as_str())) {
            Some(k) => Err(format!(
                "unknown argument `{k}` (this tool takes: {})",
                if known.is_empty() {
                    "none".to_owned()
                } else {
                    known.join(", ")
                }
            )),
            None => Ok(()),
        }
    }

    pub fn opt_str(&self, name: &str) -> Result<Option<&'a str>, String> {
        match self.map.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(format!("`{name}` must be a string")),
        }
    }

    pub fn str(&self, name: &str) -> Result<&'a str, String> {
        self.opt_str(name)?
            .ok_or_else(|| format!("`{name}` is required"))
    }

    pub fn bool(&self, name: &str, default: bool) -> Result<bool, String> {
        match self.map.get(name) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(format!("`{name}` must be true or false")),
        }
    }

    /// A whole number from `min` to `max`, `default` when not given.
    pub fn uint(&self, name: &str, default: u64, min: u64, max: u64) -> Result<u64, String> {
        let n = match self.map.get(name) {
            None | Some(Value::Null) => return Ok(default),
            Some(Value::Number(n)) => n.as_u64(),
            Some(_) => None,
        };
        match n {
            Some(n) if (min..=max).contains(&n) => Ok(n),
            _ => Err(format!(
                "`{name}` must be a whole number from {min} to {max}"
            )),
        }
    }

    /// A list of paths in the repository, at least one.
    pub fn paths(&self, name: &str) -> Result<Vec<String>, String> {
        let Some(Value::Array(items)) = self.map.get(name) else {
            return Err(format!("`{name}` must be a list of paths"));
        };
        if items.is_empty() {
            return Err(format!("`{name}` must name at least one path"));
        }
        if items.len() > MAX_PATHS {
            return Err(format!("`{name}` can name at most {MAX_PATHS} paths"));
        }
        items
            .iter()
            .map(|item| match item {
                Value::String(s) => repo_path(s).map(str::to_owned),
                _ => Err(format!("`{name}` must be a list of paths")),
            })
            .collect()
    }
}

/// The most paths one call may name.
pub const MAX_PATHS: usize = 500;

/// Longest a name or revision may be.
const MAX_NAME: usize = 255;

/// A path relative to the top of the repository, inside it: no `..`, not absolute, no drive,
/// nothing git would read as a pathspec's magic (`:`) or an option (`-`).
pub fn repo_path(path: &str) -> Result<&str, String> {
    let bad = |why: &str| Err(format!("`{path}` {why}"));
    if path.is_empty() {
        return Err("a path can't be empty".to_owned());
    }
    if path.len() > 4096 || path.chars().any(char::is_control) {
        return bad("is not a path");
    }
    if path.starts_with(['/', '\\']) || path.as_bytes().get(1) == Some(&b':') {
        return bad("must be relative to the top of the repository");
    }
    if path.starts_with(['-', ':']) {
        return bad("can't start with `-` or `:`");
    }
    if path.split(['/', '\\']).any(|part| part == "..") {
        return bad("must stay inside the repository (no `..`)");
    }
    Ok(path)
}

/// A revision as an agent names it: a hash, a branch, `HEAD~2`, `v1.0^{commit}`. Never one git
/// would take for an option, nor a range or anything with spaces.
pub fn revision(rev: &str) -> Result<&str, String> {
    if rev.is_empty() || rev.len() > MAX_NAME {
        return Err("a revision must be 1 to 255 characters".to_owned());
    }
    if rev.starts_with('-') {
        return Err(format!("`{rev}` is not a revision (it starts with `-`)"));
    }
    if rev.contains("..") || rev.contains(':') {
        return Err(format!(
            "`{rev}` names more than one commit or a file: give one commit"
        ));
    }
    if rev.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("`{rev}` is not a revision"));
    }
    Ok(rev)
}

/// A branch name git would take (`git check-ref-format --branch`'s rules, checked here first
/// so that the window never runs git with one that isn't).
pub fn branch_name(name: &str) -> Result<&str, String> {
    let bad = |why: &str| Err(format!("`{name}` can't be a branch name: {why}"));
    if name.is_empty() || name.len() > MAX_NAME {
        return bad("it must be 1 to 255 characters");
    }
    if name.starts_with('-') {
        return bad("it starts with `-`");
    }
    if name == "@" || name == "HEAD" {
        return bad("git keeps that name for itself");
    }
    if name.chars().any(|c| {
        c.is_control() || c.is_whitespace() || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    }) {
        return bad("it has a space or one of ~ ^ : ? * [ \\");
    }
    if name.contains("..") || name.contains("@{") || name.contains("//") {
        return bad("it has `..`, `@{` or `//`");
    }
    if name.ends_with('/') || name.ends_with('.') || name.ends_with(".lock") {
        return bad("it ends with `/`, `.` or `.lock`");
    }
    if name.starts_with('/') || name.split('/').any(|part| part.starts_with('.')) {
        return bad("a part of it starts with `.` or `/`");
    }
    Ok(name)
}

/// A commit message: not empty, and not so long it is surely a mistake.
pub fn message(text: &str) -> Result<&str, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("the commit message can't be empty".to_owned());
    }
    if text.len() > 64 * 1024 {
        return Err("the commit message is longer than 64 KB".to_owned());
    }
    if text.contains('\0') {
        return Err("the commit message can't hold a NUL character".to_owned());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }

    #[test]
    fn paths_stay_inside_the_repository() {
        for good in [
            "a.txt",
            "src/main.rs",
            "dir/sub dir/f",
            "[ab].txt",
            ".github/x.yml",
        ] {
            assert_eq!(repo_path(good), Ok(good));
        }
        for bad in [
            "",
            "/etc/passwd",
            "\\\\server\\share",
            "C:\\Windows",
            "c:x",
            "../outside",
            "a/../../b",
            "a\\..\\b",
            "-rf",
            ":(top)x",
            "a\nb",
        ] {
            assert!(repo_path(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn revisions_are_never_options_or_ranges() {
        for good in [
            "HEAD",
            "HEAD~2",
            "main",
            "origin/main",
            "v1.0^{commit}",
            "abc123",
        ] {
            assert_eq!(revision(good), Ok(good));
        }
        for bad in [
            "",
            "--output=/tmp/x",
            "-n",
            "a..b",
            "a...b",
            "HEAD:file",
            "a b",
            "a\tb",
        ] {
            assert!(revision(bad).is_err(), "{bad:?} must be refused");
        }
        assert!(revision(&"a".repeat(256)).is_err());
    }

    #[test]
    fn branch_names_follow_gits_rules() {
        for good in ["main", "feature/login", "fix-12", "user/x.y"] {
            assert_eq!(branch_name(good), Ok(good));
        }
        for bad in [
            "", "-x", "HEAD", "@", "a b", "a..b", "a~1", "a^", "a:b", "a?", "a*", "a[", "a\\b",
            "a/", "a.", "a.lock", "/a", "a//b", "a/.b", ".a", "a@{1}",
        ] {
            assert!(branch_name(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn messages_must_say_something() {
        assert_eq!(message("  Fix it \n"), Ok("Fix it"));
        assert!(message(" \n ").is_err());
        assert!(message("a\0b").is_err());
        assert!(message(&"x".repeat(70_000)).is_err());
    }

    #[test]
    fn arguments_are_typed_and_known() {
        let args = map(json!({"limit": 5, "all": true, "rev": "main", "paths": ["a", "b"]}));
        let args = Args::new(&args);
        assert!(args.only(&["limit", "all", "rev", "paths"]).is_ok());
        assert!(
            args.only(&["limit"])
                .is_err_and(|e| e.contains("unknown argument"))
        );
        assert_eq!(args.uint("limit", 10, 1, 100), Ok(5));
        assert_eq!(args.uint("skip", 0, 0, 100), Ok(0));
        assert!(args.uint("limit", 10, 6, 100).is_err());
        assert_eq!(args.bool("all", false), Ok(true));
        assert!(args.bool("rev", false).is_err());
        assert_eq!(args.str("rev"), Ok("main"));
        assert!(args.str("missing").is_err());
        assert!(args.opt_str("limit").is_err());
        assert_eq!(
            args.paths("paths"),
            Ok(vec!["a".to_owned(), "b".to_owned()])
        );
        assert!(args.paths("rev").is_err());
    }

    #[test]
    fn a_list_of_paths_is_checked_path_by_path() {
        let refused = |value: Value| {
            let args = map(json!({ "paths": value }));
            Args::new(&args).paths("paths").is_err()
        };
        assert!(refused(json!([])));
        assert!(refused(json!(["ok", "../no"])));
        assert!(refused(json!([1])));
        assert!(refused(json!(vec!["a"; MAX_PATHS + 1])));
        assert!(!refused(json!(["a", "b/c"])));
    }

    #[test]
    fn numbers_out_of_range_or_of_the_wrong_kind_are_refused() {
        for value in [json!(-1), json!(1.5), json!("3"), json!(1000)] {
            let args = map(json!({ "n": value }));
            assert!(Args::new(&args).uint("n", 1, 0, 100).is_err());
        }
    }
}
