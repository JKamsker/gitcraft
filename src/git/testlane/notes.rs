//! Test results as git-test stores them, so that its command line and gitcraft share them: a
//! note `good` or `bad` on the *tree* a commit has (not the commit: a rebase that keeps a
//! commit's files keeps its result), under `refs/notes/tests/<name>`, and the command in
//! `test.<name>.command`.

use std::collections::HashMap;
use std::path::Path;

use crate::git::run;

/// The name git-test gives a test when none is given, and the one gitcraft uses.
pub const DEFAULT_TEST: &str = "default";

/// How many times a note is written again when its ref is locked (the CLI writing at once).
const LOCK_RETRIES: u32 = 4;

/// What a test said of a tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verdict {
    Good,
    Bad,
}

impl Verdict {
    /// The word git-test writes in the note.
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Good => "good",
            Verdict::Bad => "bad",
        }
    }

    /// The verdict a note's text names, as git-test reads it (`good` / `bad`, with what is
    /// around them trimmed); `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "good" => Some(Verdict::Good),
            "bad" => Some(Verdict::Bad),
            _ => None,
        }
    }
}

/// The notes ref of the test `name`, as `git notes --ref` takes it.
pub fn notes_ref(name: &str) -> String {
    format!("tests/{name}")
}

/// The full ref holding the notes of the test `name`.
pub fn notes_full_ref(name: &str) -> String {
    format!("refs/notes/tests/{name}")
}

/// The command of the test `name` in the repository's config, if one is set.
pub fn test_command(repo: &Path, name: &str) -> Result<Option<String>, String> {
    let key = format!("test.{name}.command");
    // Exit 1: not set.
    let out = crate::git::run_ok_codes(Some(repo), &["config", "--get", &key], &[0, 1])?;
    // Spaces around it (a hand-edited config) don't make it another command.
    let command = out.trim();
    Ok((!command.is_empty()).then(|| command.to_owned()))
}

/// Sets the command of the test `name` in the repository's own config, as `git test add` does.
pub fn set_test_command(repo: &Path, name: &str, command: &str) -> Result<(), String> {
    let key = format!("test.{name}.command");
    run(Some(repo), &["config", "--local", &key, command]).map(|_| ())
}

/// Forgets every result of the test `name` (`git test add --forget` / `git test forget-results`).
pub fn forget_results(repo: &Path, name: &str) -> Result<(), String> {
    let full = notes_full_ref(name);
    // No such ref: nothing to forget.
    if run(Some(repo), &["rev-parse", "--verify", "-q", &full]).is_err() {
        return Ok(());
    }
    run(Some(repo), &["update-ref", "-d", &full]).map(|_| ())
}

/// Every result of the test `name`: verdict by tree. Notes whose text isn't a verdict are left
/// out (git-test refuses them; here they show as untested).
pub fn read_results(repo: &Path, name: &str) -> Result<HashMap<String, Verdict>, String> {
    let full = notes_full_ref(name);
    let Ok(tip) = run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{full}^{{tree}}")],
    ) else {
        return Ok(HashMap::new());
    };
    let listing = run(Some(repo), &["ls-tree", "-r", "-z", tip.trim()])?;
    let notes = parse_notes_tree(&listing);
    // Every `good` note is the same blob, as is every `bad` one: a read or two in all.
    let mut texts: HashMap<&str, Option<Verdict>> = HashMap::new();
    for blob in notes.values() {
        if !texts.contains_key(blob.as_str()) {
            let text = run(Some(repo), &["cat-file", "blob", blob])?;
            texts.insert(blob, Verdict::parse(&text));
        }
    }
    Ok(notes
        .iter()
        .filter_map(|(tree, blob)| Some((tree.clone(), texts.get(blob.as_str()).copied()??)))
        .collect())
}

/// The notes in what `ls-tree -r -z` prints of a notes tree: the object each annotates (its
/// path, the fan-out's `/` taken out) → the blob holding its text.
pub fn parse_notes_tree(listing: &str) -> HashMap<String, String> {
    listing
        .split('\0')
        .filter_map(|entry| {
            let (meta, path) = entry.trim_start_matches('\n').split_once('\t')?;
            let mut meta = meta.split(' ');
            let (_mode, kind, blob) = (meta.next()?, meta.next()?, meta.next()?);
            let object: String = path.chars().filter(|&c| c != '/').collect();
            let hex = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
            (kind == "blob" && hex(&object) && hex(blob) && matches!(object.len(), 40 | 64))
                .then(|| (object.to_ascii_lowercase(), blob.to_owned()))
        })
        .collect()
}

/// Who notes are written as when git knows nobody (no `user.name` / `user.email`): `git notes`
/// makes a commit, and would refuse.
const FALLBACK_IDENTITY: [&str; 4] = [
    "-c",
    "user.name=gitcraft",
    "-c",
    "user.email=gitcraft@localhost",
];

/// Whether git knows who it commits as in `repo`.
pub fn has_committer(repo: &Path) -> bool {
    run(Some(repo), &["var", "GIT_COMMITTER_IDENT"]).is_ok()
}

/// Writes `verdict` for `tree` under the test `name`, replacing what was there, as
/// `git test run` does. `fallback_identity`: write as gitcraft, git knowing nobody.
pub fn write_result(
    repo: &Path,
    name: &str,
    tree: &str,
    verdict: Verdict,
    fallback_identity: bool,
) -> Result<(), String> {
    let notes = format!("--ref={}", notes_ref(name));
    let mut args: Vec<&str> = Vec::new();
    if fallback_identity {
        args.extend(FALLBACK_IDENTITY);
    }
    args.extend(["notes", &notes, "add", "-f", "-m", verdict.word(), tree]);
    let mut tries = 0;
    loop {
        match run(Some(repo), &args) {
            Ok(_) => return Ok(()),
            Err(e) if tries < LOCK_RETRIES && is_lock_error(&e) => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(150 * u64::from(tries)));
            }
            Err(e) => {
                return Err(crate::i18n::message(
                    "backend-could-not-store-the-result-of-e-96fea2",
                    &[
                        ("tree", (short(tree)).to_string().into()),
                        ("detail", (e).to_string().into()),
                    ],
                ));
            }
        }
    }
}

/// Whether `err` says a ref was locked by another git at that moment.
fn is_lock_error(err: &str) -> bool {
    err.contains("cannot lock ref") || err.contains(".lock") || err.contains("Unable to create")
}

fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdicts_read_as_git_test_writes_them() {
        assert_eq!(Verdict::parse("good\n"), Some(Verdict::Good));
        assert_eq!(Verdict::parse("  bad \r\n"), Some(Verdict::Bad));
        assert_eq!(Verdict::parse("known-good"), None);
        assert_eq!(Verdict::parse(""), None);
        assert_eq!(Verdict::Good.word(), "good");
        assert_eq!(Verdict::Bad.word(), "bad");
    }

    #[test]
    fn the_refs_are_git_tests() {
        assert_eq!(notes_ref("default"), "tests/default");
        assert_eq!(notes_full_ref("full"), "refs/notes/tests/full");
    }

    #[test]
    fn a_notes_tree_lists_its_objects_with_and_without_fan_out() {
        let a = "a".repeat(40);
        let b = format!("ab/{}", "c".repeat(38));
        let listing = format!(
            "100644 blob 1111111111111111111111111111111111111111\t{a}\0\
             100644 blob 2222222222222222222222222222222222222222\t{b}\0\
             040000 tree 3333333333333333333333333333333333333333\tzz\0\
             100644 blob 4444444444444444444444444444444444444444\tnot-a-hash\0"
        );
        let notes = parse_notes_tree(&listing);
        assert_eq!(notes.len(), 2);
        assert_eq!(
            notes.get(&a).map(String::as_str),
            Some("1".repeat(40).as_str())
        );
        let joined = format!("ab{}", "c".repeat(38));
        assert_eq!(
            notes.get(&joined).map(String::as_str),
            Some("2".repeat(40).as_str())
        );
    }

    #[test]
    fn an_empty_listing_has_no_notes() {
        assert!(parse_notes_tree("").is_empty());
        assert!(parse_notes_tree("\0\0").is_empty());
    }

    #[test]
    fn lock_errors_are_told_apart() {
        assert!(is_lock_error(
            "error: cannot lock ref 'refs/notes/tests/default': is at x but expected y"
        ));
        assert!(!is_lock_error(
            "fatal: Failed to resolve 'x' as a valid ref."
        ));
    }
}
