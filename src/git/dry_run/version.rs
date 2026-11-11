//! Which git the repository's machine has, and so what a simulation can run on there.

use std::path::Path;

use super::Failure;

/// A git version: major, minor, patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitVersion(pub u32, pub u32, pub u32);

impl GitVersion {
    /// From what `git version` prints: `git version 2.43.0`, `git version 2.45.1.windows.1`,
    /// `git version 2.39.3 (Apple Git-146)`.
    pub fn parse(printed: &str) -> Option<Self> {
        let word = printed
            .split_whitespace()
            .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))?;
        let mut parts = word.split('.').map(|p| {
            p.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<u32>()
        });
        let major = parts.next()?.ok()?;
        let minor = parts.next().and_then(Result::ok).unwrap_or(0);
        let patch = parts.next().and_then(Result::ok).unwrap_or(0);
        Some(Self(major, minor, patch))
    }

    /// Compatibility String interface; cached previews use [`Self::of_with_witness`].
    #[cfg(test)]
    pub fn of(repo: &Path) -> Result<Self, String> {
        Self::of_with_witness(repo).map_err(|failure| failure.display())
    }

    /// The git that runs for `repo` (on its machine, for one in WSL or over SSH).
    pub(crate) fn of_with_witness(repo: &Path) -> Result<Self, Failure> {
        let printed = super::super::run(Some(repo), &["version"])?;
        Self::parse(&printed).ok_or_else(|| Failure::UnknownVersion {
            printed: printed.trim().to_owned(),
        })
    }

    /// `git replay --onto` (2.44).
    pub fn has_replay(self) -> bool {
        self >= Self(2, 44, 0)
    }

    /// `git merge-tree --write-tree` (2.38).
    pub fn has_merge_tree(self) -> bool {
        self >= Self(2, 38, 0)
    }

    /// `git merge-tree --merge-base` (2.40), which a cherry-pick in memory needs.
    pub fn has_merge_base_option(self) -> bool {
        self >= Self(2, 40, 0)
    }
}

impl std::fmt::Display for GitVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[cfg(test)]
mod tests {
    use super::GitVersion;

    #[test]
    fn reads_the_versions_git_prints() {
        for (printed, version) in [
            ("git version 2.43.0\n", GitVersion(2, 43, 0)),
            ("git version 2.45.1.windows.1", GitVersion(2, 45, 1)),
            ("git version 2.39.3 (Apple Git-146)", GitVersion(2, 39, 3)),
            ("git version 2.50.0-rc1", GitVersion(2, 50, 0)),
            ("git version 3.0", GitVersion(3, 0, 0)),
        ] {
            assert_eq!(GitVersion::parse(printed), Some(version), "{printed}");
        }
        assert_eq!(GitVersion::parse("not git"), None);
    }

    #[test]
    fn features_come_with_their_versions() {
        let v = |a, b| GitVersion(2, a, b);
        assert!(!v(43, 9).has_replay() && v(44, 0).has_replay());
        assert!(!v(37, 4).has_merge_tree() && v(38, 0).has_merge_tree());
        assert!(!v(39, 5).has_merge_base_option() && v(40, 0).has_merge_base_option());
        assert!(GitVersion(3, 0, 0).has_replay());
    }
}
