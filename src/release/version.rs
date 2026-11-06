//! Versions as tags name them (`v1.2.3`, `1.2.3-rc.1`), and the next one Conventional Commits
//! call for.

use std::fmt;

/// A version, with the prefix its tag has (`v`, or none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Version {
    pub prefix: String,
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// `rc.1` in `1.2.3-rc.1`; empty for a release.
    pub pre: String,
}

/// How much a set of changes moves the version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Bump {
    Patch,
    Minor,
    Major,
}

impl Version {
    /// Reads `v1.2.3`, `1.2`, `release-1.2.3-rc.1`: the prefix is whatever comes before the first
    /// digit; build metadata (`+…`) is dropped.
    pub(crate) fn parse(tag: &str) -> Option<Version> {
        let tag = tag.trim();
        let start = tag.find(|c: char| c.is_ascii_digit())?;
        let (prefix, rest) = tag.split_at(start);
        let rest = rest.split('+').next().unwrap_or(rest);
        let (core, pre) = rest.split_once('-').unwrap_or((rest, ""));
        let mut parts = core.split('.');
        let mut num = || -> Option<u64> {
            match parts.next() {
                None => Some(0),
                Some(p) => p.parse().ok(),
            }
        };
        let (major, minor, patch) = (num()?, num()?, num()?);
        if parts.next().is_some() {
            return None;
        }
        Some(Version {
            prefix: prefix.to_owned(),
            major,
            minor,
            patch,
            pre: pre.to_owned(),
        })
    }

    /// The version after this one for changes that call for `bump`. Before 1.0, a breaking
    /// change moves the minor version, as semver allows. A pre-release becomes its release.
    pub(crate) fn next(&self, bump: Bump) -> Version {
        let mut v = Version {
            pre: String::new(),
            ..self.clone()
        };
        if !self.pre.is_empty() {
            return v;
        }
        match bump {
            Bump::Major if self.major > 0 => {
                v.major += 1;
                v.minor = 0;
                v.patch = 0;
            }
            Bump::Major | Bump::Minor => {
                v.minor += 1;
                v.patch = 0;
            }
            Bump::Patch => v.patch += 1,
        }
        v
    }

    /// Without the prefix: `1.2.3`, as manifests write it.
    pub(crate) fn number(&self) -> String {
        let mut s = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if !self.pre.is_empty() {
            s.push('-');
            s.push_str(&self.pre);
        }
        s
    }

    pub(crate) fn is_pre(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.prefix, self.number())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap_or(Version {
            prefix: "?".into(),
            major: 0,
            minor: 0,
            patch: 0,
            pre: String::new(),
        })
    }

    #[test]
    fn tags_are_read_with_their_prefix() {
        assert_eq!(v("v1.2.3").to_string(), "v1.2.3");
        assert_eq!(v("1.2").to_string(), "1.2.0");
        assert_eq!(
            v("release-2.0.1-rc.1+build.5").to_string(),
            "release-2.0.1-rc.1"
        );
        assert!(Version::parse("latest").is_none());
        assert!(Version::parse("v1.2.3.4").is_none());
        assert!(Version::parse("v1.x").is_none());
    }

    #[test]
    fn the_next_version() {
        assert_eq!(v("v1.2.3").next(Bump::Patch).to_string(), "v1.2.4");
        assert_eq!(v("v1.2.3").next(Bump::Minor).to_string(), "v1.3.0");
        assert_eq!(v("v1.2.3").next(Bump::Major).to_string(), "v2.0.0");
        // Before 1.0, a breaking change is a minor one.
        assert_eq!(v("0.3.4").next(Bump::Major).to_string(), "0.4.0");
        // A release candidate becomes its release.
        assert_eq!(v("v2.0.0-rc.2").next(Bump::Patch).to_string(), "v2.0.0");
        assert!(v("v2.0.0-rc.2").is_pre());
    }
}
