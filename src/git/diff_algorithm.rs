//! Which of git's diff algorithms pairs a file's lines: chosen in the diff header, kept per
//! file type, and passed to each `git diff` / `git show` (never written to the repository's
//! config).

use serde::{Deserialize, Serialize};

/// One of git's `--diff-algorithm` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiffAlgorithm {
    /// git's own default: fast, pairs the longest common runs first.
    Myers,
    /// Myers, taking the time to find the smallest diff.
    Minimal,
    /// Pairs lines that appear once on both sides first: braces and blank lines are not used
    /// to line up unrelated code.
    Patience,
    /// Patience, extended to lines that are rare rather than unique; usually the clearest.
    Histogram,
}

impl DiffAlgorithm {
    pub const ALL: [DiffAlgorithm; 4] = [
        DiffAlgorithm::Myers,
        DiffAlgorithm::Minimal,
        DiffAlgorithm::Patience,
        DiffAlgorithm::Histogram,
    ];

    /// What the diff header calls it.
    pub fn label(self) -> &'static str {
        match self {
            DiffAlgorithm::Myers => "Myers",
            DiffAlgorithm::Minimal => "Minimal",
            DiffAlgorithm::Patience => "Patience",
            DiffAlgorithm::Histogram => "Histogram",
        }
    }

    /// What it does, for its tooltip.
    pub fn about(self) -> &'static str {
        match self {
            DiffAlgorithm::Myers => "The classic algorithm, fast: pairs the longest common runs",
            DiffAlgorithm::Minimal => "Myers, taking the time to find the smallest diff",
            DiffAlgorithm::Patience => {
                "Pairs lines that appear once on each side first, so braces and blank lines \
                 don't line up unrelated code"
            }
            DiffAlgorithm::Histogram => "Patience, extended to lines that are rare; often clearest",
        }
    }

    /// The option git takes.
    pub fn arg(self) -> &'static str {
        match self {
            DiffAlgorithm::Myers => "--diff-algorithm=myers",
            DiffAlgorithm::Minimal => "--diff-algorithm=minimal",
            DiffAlgorithm::Patience => "--diff-algorithm=patience",
            DiffAlgorithm::Histogram => "--diff-algorithm=histogram",
        }
    }
}

/// The file type a choice of algorithm is kept for: `*.` and the lowercased extension
/// (`*.rs` for `src/Main.RS`), or the whole lowercased name when there is none (`makefile`,
/// `.bashrc`), so a file named `json` is not taken for every `.json` file.
pub fn file_type(path: &str) -> String {
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            format!("*.{}", ext.to_lowercase())
        }
        _ => name.to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_types_are_extensions_or_names() {
        assert_eq!(file_type("src/Main.RS"), "*.rs");
        assert_eq!(file_type("archive.tar.gz"), "*.gz");
        // A file named like an extension is not every file with that extension.
        assert_ne!(file_type("json"), file_type("a.json"));
        assert_eq!(file_type("Makefile"), "makefile");
        assert_eq!(file_type("home/.bashrc"), ".bashrc");
        assert_eq!(file_type("dir.d/README"), "readme");
        assert_eq!(file_type("ünï cödé/Straße.TXT"), "*.txt");
        assert_eq!(file_type("trailing."), "trailing.");
        assert_eq!(file_type(""), "");
    }

    #[test]
    fn every_algorithm_has_its_own_option() {
        let args: std::collections::HashSet<_> =
            DiffAlgorithm::ALL.iter().map(|a| a.arg()).collect();
        assert_eq!(args.len(), DiffAlgorithm::ALL.len());
        for a in DiffAlgorithm::ALL {
            assert!(a.arg().ends_with(&a.label().to_lowercase()));
        }
    }
}
