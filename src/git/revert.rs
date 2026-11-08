//! Undoing a piece of a past commit in the working tree: a file, a hunk or a few lines of it,
//! reverse-applied (`git apply -R`), with a 3-way merge when the file has changed since.

use std::path::Path;

use super::{DiffOptions, file_diff, run};

/// How a reverse-applied patch went in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reverted {
    /// As it was, into the working tree only.
    Clean,
    /// The file had changed since: merged in with a 3-way merge, which stages the result.
    Merged,
    /// The 3-way merge left conflicts in these files, marked in them for the user to settle.
    Conflicts(Vec<String>),
}

/// Commit `hash`'s changes to `path`, as a patch to reverse-apply. A binary file is refused:
/// its changes can't be reverse-applied as text.
pub fn commit_file_patch(repo: &Path, hash: &str, path: &str) -> Result<String, String> {
    let patch = file_diff(repo, hash, path, DiffOptions::default())?;
    check_revertable(&patch, path)?;
    Ok(patch)
}

/// Why `patch` (of `path`) can't be reverse-applied, if it can't.
pub fn check_revertable(patch: &str, path: &str) -> Result<(), String> {
    if patch.trim().is_empty() {
        return Err(crate::i18n::message(
            "backend-the-commit-doesn-t-change-path-nothing-to-revert-a2ecb5",
            &[("path", (path).to_string().into())],
        ));
    }
    if is_binary_patch(patch) {
        return Err(crate::i18n::message(
            "backend-can-t-revert-path-it-is-a-binary-file-use-view-file-on-the-commit-bf2587",
            &[("path", (path).to_string().into())],
        ));
    }
    if patch_header(patch).any(|l| l.starts_with("Subproject commit") || l == "new mode 160000")
        || patch
            .lines()
            .any(|l| l.starts_with("-Subproject commit ") || l.starts_with("+Subproject commit "))
    {
        return Err(crate::i18n::message(
            "backend-can-t-revert-path-it-is-a-submodule-check-out-the-commit-it-was-a-1ca1b1",
            &[("path", (path).to_string().into())],
        ));
    }
    Ok(())
}

/// The lines before the first hunk.
fn patch_header(patch: &str) -> impl Iterator<Item = &str> {
    patch.lines().take_while(|l| !l.starts_with("@@"))
}

/// Whether git wrote `patch` for a binary file (`Binary files … differ`, or a binary patch).
pub fn is_binary_patch(patch: &str) -> bool {
    patch_header(patch).any(|l| {
        (l.starts_with("Binary files ") && l.ends_with(" differ")) || l == "GIT binary patch"
    })
}

/// Reverse-applies the patch saved at `patch_file` (as git where the repository is reads it) to
/// the working tree. When it no longer applies as it is, a 3-way merge puts it in (and stages
/// the result), leaving conflicts marked in the file when the changes since overlap.
pub fn reverse_apply(repo: &Path, patch_file: &str) -> Result<Reverted, String> {
    if run(
        Some(repo),
        &[
            "apply",
            "--reverse",
            "--whitespace=nowarn",
            "--check",
            patch_file,
        ],
    )
    .is_ok()
    {
        run(
            Some(repo),
            &["apply", "--reverse", "--whitespace=nowarn", patch_file],
        )
        .map_err(|e| {
            crate::i18n::message(
                "backend-the-changes-could-not-be-reverted-e-48d48f",
                &[("detail", (e).to_string().into())],
            )
        })?;
        return Ok(Reverted::Clean);
    }
    match run(
        Some(repo),
        &[
            "apply",
            "--reverse",
            "--whitespace=nowarn",
            "--3way",
            patch_file,
        ],
    ) {
        Ok(_) => Ok(Reverted::Merged),
        Err(e) => three_way_outcome(&e),
    }
}

/// What `git apply --3way` saying `err` means: conflicts it left, or why it did nothing.
fn three_way_outcome(err: &str) -> Result<Reverted, String> {
    if err.contains("with conflicts") {
        let files: Vec<String> = err
            .lines()
            .filter_map(|l| l.strip_prefix("U "))
            .map(|f| super::line_log::unquote_path(f.trim()))
            .collect();
        return Ok(Reverted::Conflicts(files));
    }
    let named = |what: &str| {
        err.lines()
            .find_map(|l| l.strip_prefix("error: ")?.strip_suffix(what))
            .map(str::to_owned)
    };
    if let Some(file) = named(": does not match index") {
        return Err(crate::i18n::message(
            "backend-file-has-changed-since-that-commit-and-has-unstaged-changes-an-ea-948530",
            &[("file", (file).to_string().into())],
        ));
    }
    if let Some(file) = named(": does not exist in index") {
        return Err(crate::i18n::message(
            "backend-file-isn-t-tracked-any-more-so-the-commit-s-changes-to-it-can-t-b-d27ce9",
            &[("file", (file).to_string().into())],
        ));
    }
    if err.contains("lacks the necessary blob") {
        return Err(crate::i18n::text(
            "backend-the-file-has-changed-since-that-commit-and-the-version-needed-to-62f8a1",
        )
        .to_owned());
    }
    Err(crate::i18n::message(
        "backend-the-changes-don-t-apply-to-the-file-as-it-is-now-43cf8c",
        &[("detail", (err.trim()).to_string().into())],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_patches_are_recognised() {
        let binary = "diff --git a/i.png b/i.png\nindex 1..2 100644\nBinary files a/i.png and b/i.png differ\n";
        assert!(is_binary_patch(binary));
        let git_binary = "diff --git a/i.png b/i.png\nindex 1..2\nGIT binary patch\nliteral 3\n";
        assert!(is_binary_patch(git_binary));
        // A text file whose lines say so is not one.
        let text = "diff --git a/t b/t\n--- a/t\n+++ b/t\n@@ -1 +1 @@\n-Binary files x and y differ\n+GIT binary patch\n";
        assert!(!is_binary_patch(text));
        assert!(check_revertable(binary, "i.png").is_err_and(|e| e.contains("binary")));
        assert!(check_revertable(text, "t").is_ok());
        assert!(check_revertable("", "t").is_err_and(|e| e.contains("nothing to revert")));
    }

    #[test]
    fn submodules_are_refused() {
        let sub = "diff --git a/lib b/lib\nindex 1..2 160000\n--- a/lib\n+++ b/lib\n@@ -1 +1 @@\n-Subproject commit aaa\n+Subproject commit bbb\n";
        assert!(check_revertable(sub, "lib").is_err_and(|e| e.contains("submodule")));
    }

    #[test]
    fn what_a_three_way_merge_said() {
        assert_eq!(
            three_way_outcome("Applied patch to 'a b.txt' with conflicts.\nU a b.txt"),
            Ok(Reverted::Conflicts(vec!["a b.txt".to_owned()]))
        );
        assert_eq!(
            three_way_outcome("Applied patch to 'x' with conflicts.\nU \"\\303\\274.txt\""),
            Ok(Reverted::Conflicts(vec!["ü.txt".to_owned()]))
        );
        let dirty = three_way_outcome("error: a.txt: does not match index");
        assert!(dirty.is_err_and(|e| e.starts_with("a.txt has changed") && e.contains("stash")));
        let other = three_way_outcome("error: something else");
        assert!(other.is_err_and(|e| e.contains("something else")));
    }
}
