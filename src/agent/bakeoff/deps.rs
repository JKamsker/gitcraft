//! New dependencies an attempt brings in, read from the lines its diff adds to the manifests of
//! the common package managers: names that are added and weren't there before (a version bump
//! removes the old line and adds a new one, and is no new dependency).

/// Whether `path` is a manifest whose dependencies are read.
pub(crate) fn is_manifest(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "Cargo.toml" | "package.json" | "go.mod" | "pyproject.toml" | "Gemfile" | "composer.json"
    ) || (name.starts_with("requirements") && name.ends_with(".txt"))
}

/// The dependencies `patch` (a diff of the manifest `path`) adds, sorted.
pub(crate) fn added(path: &str, patch: &str) -> Vec<String> {
    let name_of: fn(&str) -> Option<String> = {
        let file = path.rsplit('/').next().unwrap_or(path);
        match file {
            "Cargo.toml" | "pyproject.toml" => toml_name,
            "package.json" | "composer.json" => json_name,
            "go.mod" => go_name,
            "Gemfile" => gem_name,
            _ => requirement_name,
        }
    };
    let lines = |mark: char| {
        patch
            .lines()
            .filter(move |l| l.starts_with(mark) && !l.starts_with("+++") && !l.starts_with("---"))
            .filter_map(move |l| name_of(l[1..].trim()))
    };
    let removed: Vec<String> = lines('-').collect();
    let mut out: Vec<String> = lines('+').filter(|n| !removed.contains(n)).collect();
    out.sort();
    out.dedup();
    out
}

/// `serde = "1"`, `serde = { version = … }`, and pyproject's `"requests>=2"` in a list.
fn toml_name(line: &str) -> Option<String> {
    if line.starts_with('[') || line.starts_with('#') {
        return None;
    }
    if let Some(quoted) = line.strip_prefix('"') {
        return requirement_name(quoted.split('"').next()?);
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim().trim_matches('"');
    let value = value.trim();
    // Keys that are fields of the package, not dependencies.
    let fields = [
        "name",
        "version",
        "edition",
        "authors",
        "description",
        "license",
        "readme",
        "repository",
        "homepage",
        "rust-version",
        "requires-python",
        "publish",
        "build",
        "dependencies",
        "keywords",
        "categories",
        "workspace",
        "path",
        "features",
        "default",
    ];
    let looks_like_dep = value.starts_with('"') || value.starts_with('{');
    (looks_like_dep && !fields.contains(&key) && valid(key)).then(|| key.to_owned())
}

/// `"left-pad": "^1.3.0",` (any `"name": "version"` line).
fn json_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix('"')?;
    let (key, value) = rest.split_once("\":")?;
    let value = value.trim().trim_end_matches(',');
    let fields = [
        "name",
        "version",
        "description",
        "main",
        "license",
        "type",
        "private",
    ];
    (value.starts_with('"') && !fields.contains(&key) && valid(key)).then(|| key.to_owned())
}

/// `require github.com/x/y v1.2.3` or `\tgithub.com/x/y v1.2.3` inside a `require (…)`.
fn go_name(line: &str) -> Option<String> {
    let line = line.strip_prefix("require ").unwrap_or(line);
    let mut words = line.split_whitespace();
    let module = words.next()?;
    let version = words.next()?;
    (version.starts_with('v') && module.contains('.') && valid(module)).then(|| module.to_owned())
}

/// `gem 'rails', '~> 7'`.
fn gem_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("gem ")?.trim_start();
    let name = rest.trim_start_matches(['\'', '"']);
    let name = name.split(['\'', '"']).next()?;
    valid(name).then(|| name.to_owned())
}

/// `requests>=2.0  # comment`, `numpy[extra]==1`.
fn requirement_name(line: &str) -> Option<String> {
    let line = line.split('#').next()?.trim();
    if line.is_empty() || line.starts_with('-') {
        return None;
    }
    let end = line
        .find(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(line.len());
    let name = &line[..end];
    valid(name).then(|| name.to_owned())
}

fn valid(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '@'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_are_known_by_name() {
        assert!(is_manifest("crates/x/Cargo.toml"));
        assert!(is_manifest("requirements-dev.txt"));
        assert!(!is_manifest("Cargo.lock"));
        assert!(!is_manifest("src/package.json.rs"));
    }

    #[test]
    fn cargo_dependencies_added_not_bumped() {
        let patch = "+++ b/Cargo.toml\n-serde = \"1.0\"\n+serde = \"1.1\"\n+anyhow = \"1\"\n+tokio = { version = \"1\", features = [\"rt\"] }\n+version = \"0.2\"\n+[dependencies]\n";
        assert_eq!(added("Cargo.toml", patch), ["anyhow", "tokio"]);
    }

    #[test]
    fn npm_go_python_and_ruby_dependencies() {
        assert_eq!(
            added(
                "package.json",
                "+    \"left-pad\": \"^1.3.0\",\n+  \"version\": \"2.0.0\",\n+    \"@scope/pkg\": \"1\"\n"
            ),
            ["@scope/pkg", "left-pad"]
        );
        assert_eq!(
            added(
                "go.mod",
                "+require github.com/pkg/errors v0.9.1\n+\tgolang.org/x/text v0.3.0\n+go 1.22\n"
            ),
            ["github.com/pkg/errors", "golang.org/x/text"]
        );
        assert_eq!(
            added(
                "requirements.txt",
                "+requests>=2.0  # http\n+# a comment\n+-r base.txt\n+numpy[extra]==1\n"
            ),
            ["numpy", "requests"]
        );
        assert_eq!(
            added(
                "pyproject.toml",
                "+    \"httpx>=0.27\",\n+requires-python = \">=3.10\"\n"
            ),
            ["httpx"]
        );
        assert_eq!(added("Gemfile", "+gem 'rails', '~> 7'\n"), ["rails"]);
    }

    #[test]
    fn nothing_from_an_empty_or_odd_patch() {
        assert!(added("Cargo.toml", "").is_empty());
        assert!(added("package.json", "+{\n+}\n+ünïcode garbage\n").is_empty());
    }
}
