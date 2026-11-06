//! Git notes: text attached to a commit without rewriting it, kept in namespaces (refs under
//! `refs/notes/`) that are fetched and pushed like branches, but only when asked for.

use std::path::Path;

use super::{END_OPTS, run, run_input};

/// The namespace git uses when nothing else is set (`core.notesRef`).
pub const DEFAULT_NOTES: &str = "commits";

const NOTES_PREFIX: &str = "refs/notes/";

/// The notes namespaces of a repository, as short names (`commits`, `review/ci`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotesNamespaces {
    /// The one git uses by default first, whether it has notes yet or not, then the others.
    pub names: Vec<String>,
    /// Those that have notes here (a ref).
    pub existing: Vec<String>,
}

/// The namespaces there are notes in, and the one git uses by default.
pub fn notes_namespaces(repo: &Path) -> Result<NotesNamespaces, String> {
    let default = run(Some(repo), &["notes", "get-ref"])
        .map(|r| short_namespace(r.trim()).to_owned())
        .unwrap_or_else(|_| DEFAULT_NOTES.to_owned());
    let out = run(
        Some(repo),
        &["for-each-ref", "--format=%(refname)", NOTES_PREFIX],
    )
    .map_err(|e| format!("Could not list the notes namespaces: {e}"))?;
    Ok(namespace_list(&default, &out))
}

/// `default` first, then the others `refs` (full ref names, one a line) lists.
pub(super) fn namespace_list(default: &str, refs: &str) -> NotesNamespaces {
    let existing: Vec<String> = refs
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|name| short_namespace(name).to_owned())
        .collect();
    let mut names = vec![default.to_owned()];
    for name in &existing {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    NotesNamespaces { names, existing }
}

/// `commits` of `refs/notes/commits`.
pub fn short_namespace(name: &str) -> &str {
    name.strip_prefix(NOTES_PREFIX).unwrap_or(name)
}

/// The full ref of the namespace `name` (short, or already full), or why it can't be one.
pub fn notes_ref(name: &str) -> Result<String, String> {
    let short = short_namespace(name.trim());
    let bad = |why: &str| Err(format!("“{short}” can't be a notes namespace: {why}"));
    if short.is_empty() {
        return bad("it is empty");
    }
    if short.starts_with('/') || short.ends_with('/') {
        return bad("it can't start or end with “/”");
    }
    // As git: a space or a control character, not other spaces (it is given as `--ref=`, so
    // a leading `-` is fine too).
    if short.chars().any(|c| c == ' ' || c.is_control()) {
        return bad("it can't have spaces");
    }
    if short.chars().any(|c| "~^:?*[\\".contains(c)) {
        return bad("it can't have any of ~ ^ : ? * [ \\");
    }
    if short.contains("..") || short.contains("//") || short.contains("@{") {
        return bad("it can't have “..”, “//” or “@{”");
    }
    if short
        .split('/')
        .any(|part| part.starts_with('.') || part.ends_with(".lock"))
    {
        return bad("no part of it can start with “.” or end with “.lock”");
    }
    if short.ends_with('.') {
        return bad("it can't end with “.”");
    }
    Ok(format!("{NOTES_PREFIX}{short}"))
}

/// The note on `hash` in `namespace`, or `None` when it has none there.
pub fn read_note(repo: &Path, namespace: &str, hash: &str) -> Result<Option<String>, String> {
    let r = notes_ref(namespace)?;
    let listed = super::run_ok_codes(
        Some(repo),
        &["notes", &format!("--ref={r}"), "list", END_OPTS, hash],
        &[0, 1],
    )
    .map_err(|e| format!("Could not read the note: {e}"))?;
    let blob = listed.trim();
    if blob.is_empty() {
        return Ok(None);
    }
    let text = run(Some(repo), &["cat-file", "blob", blob])
        .map_err(|e| format!("Could not read the note: {e}"))?;
    Ok(Some(text))
}

/// Sets the note on `hash` in `namespace` to `text`, adding it or replacing the one there.
/// An empty `text` removes the note, as git would.
pub fn write_note(repo: &Path, namespace: &str, hash: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return remove_note(repo, namespace, hash);
    }
    let r = notes_ref(namespace)?;
    // From the input: a note of any length, never cut by a command line's limit.
    run_input(
        Some(repo),
        &["notes", &format!("--ref={r}"), "add", "-f", "-F", "-", hash],
        text,
    )
    .map(|_| ())
    .map_err(|e| format!("Could not save the note: {e}"))
}

/// Removes the note on `hash` in `namespace`, if it has one.
pub fn remove_note(repo: &Path, namespace: &str, hash: &str) -> Result<(), String> {
    let r = notes_ref(namespace)?;
    run(
        Some(repo),
        &[
            "notes",
            &format!("--ref={r}"),
            "remove",
            "--ignore-missing",
            hash,
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("Could not delete the note: {e}"))
}

/// The git arguments that fetch `namespace` from `remote` into the namespace of the same
/// name here. Not forced: notes made here that the remote doesn't have stop it, rather than
/// being thrown away.
pub fn fetch_notes_args(remote: &str, namespace: &str) -> Result<Vec<String>, String> {
    let r = notes_ref(namespace)?;
    Ok(vec![
        "fetch".to_owned(),
        END_OPTS.to_owned(),
        remote.to_owned(),
        format!("{r}:{r}"),
    ])
}

/// The git arguments that push `namespace` to `remote`.
pub fn push_notes_args(remote: &str, namespace: &str) -> Result<Vec<String>, String> {
    let r = notes_ref(namespace)?;
    Ok(vec![
        "push".to_owned(),
        END_OPTS.to_owned(),
        remote.to_owned(),
        format!("{r}:{r}"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_are_checked_like_ref_names() {
        assert_eq!(notes_ref("commits").as_deref(), Ok("refs/notes/commits"));
        assert_eq!(
            notes_ref("refs/notes/review/ci").as_deref(),
            Ok("refs/notes/review/ci")
        );
        assert_eq!(notes_ref(" büro ").as_deref(), Ok("refs/notes/büro"));
        assert_eq!(notes_ref("-x").as_deref(), Ok("refs/notes/-x"));
        assert_eq!(notes_ref("a\u{a0}b").as_deref(), Ok("refs/notes/a\u{a0}b"));
        for bad in [
            "", "  ", "a b", "a\tb", "a..b", "a:b", "x.lock", ".hidden", "a/", "/a", "a//b", "a~1",
            "a^", "a*", "a@{1}", "a.", "a\\b",
        ] {
            assert!(notes_ref(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_default_namespace_comes_first_and_once() {
        let list = namespace_list(
            "commits",
            "refs/notes/ci\nrefs/notes/commits\nrefs/notes/review/x\n",
        );
        assert_eq!(list.names, ["commits", "ci", "review/x"]);
        assert_eq!(list.existing, ["ci", "commits", "review/x"]);
        let none = namespace_list("mine", "");
        assert_eq!(none.names, ["mine"]);
        assert!(none.existing.is_empty());
    }

    #[test]
    fn fetching_and_pushing_name_the_namespace_on_both_sides() -> Result<(), String> {
        assert_eq!(
            fetch_notes_args("origin", "ci")?,
            [
                "fetch",
                "--end-of-options",
                "origin",
                "refs/notes/ci:refs/notes/ci"
            ]
        );
        assert_eq!(
            push_notes_args("up stream", "commits")?,
            [
                "push",
                "--end-of-options",
                "up stream",
                "refs/notes/commits:refs/notes/commits"
            ]
        );
        assert!(push_notes_args("origin", "a b").is_err());
        Ok(())
    }
}
