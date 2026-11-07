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
            Kind::Npm => replace_json(text, &[&["version"]], new),
        }
    }

    /// The lock file `text` with the version of the project's own packages moved from `old` to
    /// `new`.
    /// `names`: the project's packages (the manifest's `[package]` name; none when it takes
    /// the workspace's version, which moves every package of the workspace at `old`).
    pub(crate) fn bump_lock(
        self,
        text: &str,
        names: &[String],
        old: &str,
        new: &str,
    ) -> Option<String> {
        match self {
            Kind::Cargo => Some(bump_cargo_lock(text, names, old, new)),
            // The top and `packages[""]` (lock files of version 2 and 3) say it.
            Kind::Npm => replace_json(text, &[&["version"], &["packages", "", "version"]], new),
        }
    }

    /// The name of the package the manifest `text` is, when it has its own version (not the
    /// workspace's).
    pub(crate) fn own_name(self, text: &str) -> Option<String> {
        match self {
            Kind::Cargo => cargo_name(text),
            Kind::Npm => serde_json::from_str::<serde_json::Value>(text)
                .ok()?
                .get("name")?
                .as_str()
                .map(str::to_owned),
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
            // `[package] # a comment`.
            let header = t.split('#').next().unwrap_or(t).trim();
            section = header.trim_matches(['[', ']']).trim().to_owned();
            continue;
        }
        let Some((key, value)) = t.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.split('#').next().unwrap_or(value).trim();
        let quoted = unquote(value);
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

/// A TOML string value, `"x"` or `'x'`.
fn unquote(value: &str) -> Option<String> {
    ['"', '\'']
        .iter()
        .find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
        .map(str::to_owned)
}

/// The `[package]` name of a `Cargo.toml` that has a version of its own.
fn cargo_name(text: &str) -> Option<String> {
    let mut section = String::new();
    let mut name = None;
    let mut own_version = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            let header = t.split('#').next().unwrap_or(t).trim();
            section = header.trim_matches(['[', ']']).trim().to_owned();
            continue;
        }
        if section != "package" {
            continue;
        }
        let Some((key, value)) = t.split_once('=') else {
            continue;
        };
        let value = value.split('#').next().unwrap_or(value).trim();
        match key.trim() {
            "name" => name = unquote(value),
            "version" => own_version = unquote(value).is_some(),
            _ => {}
        }
    }
    name.filter(|_| own_version)
}

/// `Cargo.lock` with the project's packages at `old` moved to `new`: those named in `names`,
/// or, without names (a workspace's version), every package of the project (one without a
/// `source`).
fn bump_cargo_lock(text: &str, names: &[String], old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let blocks: Vec<&str> = text.split_inclusive("\n[[package]]").collect();
    for block in blocks {
        let local = !block
            .lines()
            .any(|l| l.trim_start().starts_with("source ="));
        let named = block.lines().any(|l| {
            l.trim()
                .strip_prefix("name = \"")
                .and_then(|n| n.strip_suffix('"'))
                .is_some_and(|n| names.is_empty() || names.iter().any(|x| x == n))
        });
        if local && named {
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

/// `text` (JSON) with the string values at `paths` (keys from the top, `""` for an empty key)
/// made `new`, found by where they are in the text, whatever comes before them. `None` when one
/// isn't there.
fn replace_json(text: &str, paths: &[&[&str]], new: &str) -> Option<String> {
    let mut spans = Vec::new();
    for path in paths {
        spans.push(json_value_span(text, path)?);
    }
    spans.sort();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end) in spans {
        out.push_str(&text[at..start]);
        out.push('"');
        out.push_str(new);
        out.push('"');
        at = end;
    }
    out.push_str(&text[at..]);
    Some(out)
}

/// Where the string value at `path` is in `text`: from its opening quote to after its closing
/// one.
fn json_value_span(text: &str, path: &[&str]) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    // The key each open object or array was reached by (`None` for the top one).
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut key: Option<String> = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'[' => {
                stack.push(key.take());
                i += 1;
            }
            b'}' | b']' => {
                stack.pop();
                key = None;
                i += 1;
            }
            b'"' => {
                let start = i;
                let end = string_end(bytes, i)?;
                let s = serde_json::from_str::<String>(&text[start..end]).ok()?;
                let mut j = end;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                // A key is followed by `:`.
                if bytes.get(j) == Some(&b':') {
                    key = Some(s);
                    i = j + 1;
                    continue;
                }
                let mut here: Vec<&str> = stack
                    .iter()
                    .skip(1)
                    .map(|k| k.as_deref().unwrap_or("\u{0}"))
                    .collect();
                if let Some(k) = &key {
                    here.push(k);
                }
                if here == path {
                    return Some((start, end));
                }
                key = None;
                i = end;
            }
            b',' => {
                key = None;
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// Where the JSON string starting at `bytes[start]` (a quote) ends: after its closing quote.
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
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
            .bump_lock(lock, &["app".to_owned()], "0.3.4", "0.4.0")
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
            .bump_lock(lock, &[], "1.2.3", "1.3.0")
            .unwrap_or_default();
        assert_eq!(bumped.matches("\"1.3.0\"").count(), 2);
        assert!(bumped.contains("\"node_modules/x\": {\n      \"version\": \"1.2.3\""));
        assert_eq!(Kind::Npm.bump("not json", "1"), None);
    }

    /// A nested `"version"` before the top one isn't the package's.
    #[test]
    fn only_the_top_version_of_package_json_moves() {
        let pkg = r#"{"name":"x","config":{"version":"1.2.3"},"version":"1.2.3"}"#;
        assert_eq!(
            Kind::Npm.bump(pkg, "2.0.0").as_deref(),
            Some(r#"{"name":"x","config":{"version":"1.2.3"},"version":"2.0.0"}"#)
        );
        let escaped = r#"{"description":"say \"version\": \"1\"","version":"1"}"#;
        assert_eq!(
            Kind::Npm.bump(escaped, "2").as_deref(),
            Some(r#"{"description":"say \"version\": \"1\"","version":"2"}"#)
        );
        assert_eq!(Kind::Npm.bump(r#"{"name":"x"}"#, "2"), None);
    }

    #[test]
    fn a_member_with_its_own_version_isnt_moved_in_the_lock() {
        let lock = "[[package]]\nname = \"app\"\nversion = \"1.0.0\"\n\n[[package]]\nname = \"tool\"\nversion = \"1.0.0\"\n";
        let bumped = Kind::Cargo
            .bump_lock(lock, &["app".to_owned()], "1.0.0", "1.1.0")
            .unwrap_or_default();
        assert!(bumped.contains("name = \"app\"\nversion = \"1.1.0\""));
        assert!(bumped.contains("name = \"tool\"\nversion = \"1.0.0\""));
        let quoted = "[package] # x\nname = 'app'\nversion = '1.0.0'\n";
        assert_eq!(Kind::Cargo.own_name(quoted).as_deref(), Some("app"));
        assert_eq!(Kind::Cargo.version(quoted).as_deref(), Some("1.0.0"));
        assert_eq!(
            Kind::Cargo.own_name("[package]\nname = \"a\"\nversion.workspace = true\n"),
            None
        );
    }
}
