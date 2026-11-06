//! The version written in a project's manifests — `Cargo.toml` (and its `Cargo.lock`),
//! `package.json` (and its `package-lock.json`) — and those files with a new one. The text is
//! changed where the version is and nowhere else, so that their formatting stays.

/// A manifest whose version a release moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Cargo,
    Npm,
}

impl Kind {
    pub(crate) const ALL: [Kind; 2] = [Kind::Cargo, Kind::Npm];

    /// The manifest, then its lock file.
    pub(crate) fn files(self) -> (&'static str, &'static str) {
        match self {
            Kind::Cargo => ("Cargo.toml", "Cargo.lock"),
            Kind::Npm => ("package.json", "package-lock.json"),
        }
    }

    /// The version the manifest `text` says.
    pub(crate) fn version(self, text: &str) -> Option<String> {
        match self {
            Kind::Cargo => cargo_version(text).map(|(_, v)| v),
            Kind::Npm => serde_json::from_str::<serde_json::Value>(text)
                .ok()?
                .get("version")?
                .as_str()
                .map(str::to_owned),
        }
    }

    /// The manifest `text` with `new` in place of its version.
    pub(crate) fn bump(self, text: &str, new: &str) -> Option<String> {
        match self {
            Kind::Cargo => {
                let (line, old) = cargo_version(text)?;
                let lines: Vec<&str> = text.split_inclusive('\n').collect();
                let changed =
                    lines
                        .get(line)?
                        .replacen(&format!("\"{old}\""), &format!("\"{new}\""), 1);
                Some(lines[..line].concat() + &changed + &lines[line + 1..].concat())
            }
            Kind::Npm => {
                let old = self.version(text)?;
                replace_json_versions(text, &old, new, 1)
            }
        }
    }

    /// The lock file `text` with the version of the project's own packages moved from `old` to
    /// `new`.
    pub(crate) fn bump_lock(self, text: &str, old: &str, new: &str) -> Option<String> {
        match self {
            Kind::Cargo => Some(bump_cargo_lock(text, old, new)),
            Kind::Npm => {
                let v: serde_json::Value = serde_json::from_str(text).ok()?;
                // The top and `packages[""]` (lock files of version 2 and 3) say it.
                let mut n = 0;
                if v.get("version").and_then(|v| v.as_str()) == Some(old) {
                    n += 1;
                }
                if v.pointer("/packages/")
                    .and_then(|p| p.get("version"))
                    .and_then(|v| v.as_str())
                    == Some(old)
                {
                    n += 1;
                }
                replace_json_versions(text, old, new, n)
            }
        }
    }
}

/// The line and the value of the version of `[package]`, or of `[workspace.package]` when the
/// package takes the workspace's (`version.workspace = true`).
fn cargo_version(text: &str) -> Option<(usize, String)> {
    let mut section = String::new();
    let mut found: Option<(usize, String)> = None;
    let mut inherits = false;
    let mut workspace: Option<(usize, String)> = None;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.trim_matches(['[', ']']).trim().to_owned();
            continue;
        }
        let Some((key, value)) = t.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.split('#').next().unwrap_or(value).trim();
        let quoted = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .map(str::to_owned);
        match (section.as_str(), key) {
            ("package", "version") => match quoted {
                Some(v) => found = Some((i, v)),
                None => inherits = true,
            },
            ("package", "version.workspace") => inherits = true,
            ("workspace.package", "version") => workspace = quoted.map(|v| (i, v)),
            _ => {}
        }
    }
    if inherits || found.is_none() {
        workspace
    } else {
        found
    }
}

/// `Cargo.lock` with every package of the project (one without a `source`) at `old` moved to
/// `new`.
fn bump_cargo_lock(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let blocks: Vec<&str> = text.split_inclusive("\n[[package]]").collect();
    for block in blocks {
        let local = !block
            .lines()
            .any(|l| l.trim_start().starts_with("source ="));
        let is_package = block.contains("name = \"");
        if local && is_package {
            out.push_str(&block.replacen(
                &format!("\nversion = \"{old}\""),
                &format!("\nversion = \"{new}\""),
                1,
            ));
        } else {
            out.push_str(block);
        }
    }
    out
}

/// `text` (JSON) with the first `n` `"version": "old"` made `new`, whatever the spaces around
/// the colon.
fn replace_json_versions(text: &str, old: &str, new: &str, n: usize) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut done = 0;
    while done < n {
        let at = rest.find("\"version\"")?;
        let after_key = &rest[at + 9..];
        let colon = after_key.find(':')?;
        if !after_key[..colon].trim().is_empty() {
            out.push_str(&rest[..at + 9]);
            rest = after_key;
            continue;
        }
        let value = after_key[colon + 1..].trim_start();
        let skipped = after_key.len() - colon - 1 - value.len();
        let quoted = format!("\"{old}\"");
        if value.starts_with(&quoted) {
            out.push_str(&rest[..at + 9 + colon + 1 + skipped]);
            out.push_str(&format!("\"{new}\""));
            rest = &value[quoted.len()..];
            done += 1;
        } else {
            out.push_str(&rest[..at + 9]);
            rest = after_key;
        }
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARGO: &str = "[package]\nname = \"app\"\nversion = \"0.3.4\" # the version\nedition = \"2024\"\n\n[dependencies]\nserde = { version = \"1\" }\n";

    #[test]
    fn cargo_toml_keeps_its_formatting() {
        assert_eq!(Kind::Cargo.version(CARGO).as_deref(), Some("0.3.4"));
        let bumped = Kind::Cargo.bump(CARGO, "0.4.0").unwrap_or_default();
        assert_eq!(bumped, CARGO.replace("\"0.3.4\"", "\"0.4.0\""));
    }

    #[test]
    fn a_workspace_version_is_the_one_moved() {
        let text = "[workspace.package]\nversion = \"2.0.0\"\n\n[package]\nname = \"a\"\nversion.workspace = true\n";
        assert_eq!(Kind::Cargo.version(text).as_deref(), Some("2.0.0"));
        let text2 = "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"2.0.0\"\n";
        assert_eq!(
            Kind::Cargo.bump(text2, "2.1.0").unwrap_or_default(),
            text2.replace("2.0.0", "2.1.0")
        );
        assert_eq!(Kind::Cargo.version("[dependencies]\nx = \"1\"\n"), None);
    }

    #[test]
    fn the_lock_moves_only_the_projects_own_packages() {
        let lock = "version = 4\n\n[[package]]\nname = \"app\"\nversion = \"0.3.4\"\ndependencies = [\n \"dep\",\n]\n\n[[package]]\nname = \"dep\"\nversion = \"0.3.4\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
        let bumped = Kind::Cargo
            .bump_lock(lock, "0.3.4", "0.4.0")
            .unwrap_or_default();
        assert!(bumped.contains("name = \"app\"\nversion = \"0.4.0\""));
        assert!(bumped.contains("name = \"dep\"\nversion = \"0.3.4\""));
        assert!(bumped.starts_with("version = 4\n"));
    }

    #[test]
    fn package_json_and_its_lock() {
        let pkg = "{\n  \"name\": \"app\",\n  \"version\" : \"1.2.3\",\n  \"devDependencies\": { \"x\": \"1.2.3\" }\n}\n";
        assert_eq!(Kind::Npm.version(pkg).as_deref(), Some("1.2.3"));
        assert_eq!(
            Kind::Npm.bump(pkg, "1.3.0").unwrap_or_default(),
            pkg.replace("\"version\" : \"1.2.3\"", "\"version\" : \"1.3.0\"")
        );
        let lock = "{\n  \"name\": \"app\",\n  \"version\": \"1.2.3\",\n  \"lockfileVersion\": 3,\n  \"packages\": {\n    \"\": {\n      \"name\": \"app\",\n      \"version\": \"1.2.3\"\n    },\n    \"node_modules/x\": {\n      \"version\": \"1.2.3\"\n    }\n  }\n}\n";
        let bumped = Kind::Npm
            .bump_lock(lock, "1.2.3", "1.3.0")
            .unwrap_or_default();
        assert_eq!(bumped.matches("\"1.3.0\"").count(), 2);
        assert!(bumped.contains("\"node_modules/x\": {\n      \"version\": \"1.2.3\""));
        assert_eq!(Kind::Npm.bump("not json", "1"), None);
    }
}
