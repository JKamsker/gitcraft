//! Binary files through the diff the view loads, on real repositories.

use std::path::{Path, PathBuf};

use super::extract::tests::{make_docx, make_pdf};
use super::member_changes;
#[cfg(unix)]
use super::textconv_for;
use super::view::size_change;
use super::zip::{self, tests::make_zip};
use crate::git::{self, Area, DiffOptions, StatusFile};
use crate::repo::DiffKey;
use crate::repo::diff::line_diff::Edit;
use crate::repo::diff::load;

fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-binary-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    // No diff driver for the kinds the tests diff, whatever the system's attributes say (Git
    // for Windows maps `*.docx` and `*.pdf` to its `astextplain`).
    let attributes = "*.docx !diff\n*.pdf !diff\n*.zip !diff\n*.bin !diff\n*.png !diff\n";
    std::fs::write(dir.join(".git/info/attributes"), attributes).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn commit(dir: &Path, name: &str, bytes: &[u8], message: &str) -> Result<String, String> {
    std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())?;
    git::run(Some(dir), &["add", "-A"])?;
    git::run(Some(dir), &["commit", "-qm", message])?;
    Ok(git::head(dir))
}

fn commit_key(hash: &str, path: &str) -> DiffKey {
    DiffKey::Commit {
        hash: hash.to_owned(),
        path: path.to_owned(),
    }
}

fn texts(lines: &[(Edit, String)]) -> Vec<String> {
    lines
        .iter()
        .map(|(e, l)| {
            let mark = match e {
                Edit::Same => ' ',
                Edit::Del => '-',
                Edit::Add => '+',
            };
            format!("{mark}{l}")
        })
        .collect()
}

// Formats; included to preserve qualified test names.
include!("tests/formats.rs");

// Comparison; included to preserve qualified test names.
include!("tests/comparison.rs");

// Bounded reads; included to preserve qualified test names.
include!("tests/bounded_reads.rs");
