//! What looks wrong in a package's files: those of the project left out (a README, a source
//! file, what the manifest names), those that look shipped by accident (secrets, logs, build
//! output, files git doesn't track), and how they differ from the version published.

use super::Ecosystem;

/// One file the panel points out, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Note {
    pub path: String,
    pub why: String,
}

/// Files a tool adds to every package, made as it packs: never "not tracked".
fn generated(eco: Ecosystem, path: &str) -> bool {
    match eco {
        Ecosystem::Cargo => matches!(
            path,
            "Cargo.toml.orig" | ".cargo_vcs_info.json" | "Cargo.lock" | "Cargo.toml"
        ),
        Ecosystem::Npm => path == "package.json",
    }
}

/// Why `path` looks shipped by accident, if it does.
pub(crate) fn suspicious(path: &str) -> Option<&'static str> {
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let parts: Vec<&str> = lower.split('/').collect();
    let dir = |d: &str| parts[..parts.len().saturating_sub(1)].contains(&d);
    if name == ".env" || name.starts_with(".env.") {
        return Some("an environment file: often secrets");
    }
    if name == ".npmrc" || name == ".pypirc" || name == "credentials" || name == ".netrc" {
        return Some("a credentials file: may hold a token");
    }
    if name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || [".pem", ".key", ".p12", ".pfx"]
            .iter()
            .any(|e| name.ends_with(e))
    {
        return Some("a key or certificate");
    }
    if dir("node_modules") || dir("target") || dir(".git") {
        return Some("build output or dependencies");
    }
    if dir("coverage")
        || dir(".idea")
        || dir(".vscode")
        || name == ".ds_store"
        || name == "thumbs.db"
    {
        return Some("an editor's or tool's leftover");
    }
    if name.ends_with(".log") {
        return Some("a log");
    }
    if [".tgz", ".crate", ".zip", ".tar.gz"]
        .iter()
        .any(|e| name.ends_with(e))
    {
        return Some("an archive, perhaps an earlier package");
    }
    if [".sqlite", ".sqlite3", ".db"]
        .iter()
        .any(|e| name.ends_with(e))
    {
        return Some("a database");
    }
    None
}

/// How big a file must be to be pointed out.
const BIG: u64 = 1024 * 1024;

/// The files shipped that look like an accident. `tracked`: the files git tracks in the
/// package's folder, relative to it.
pub(crate) fn accidental(
    eco: Ecosystem,
    files: &[(String, Option<u64>)],
    tracked: &[String],
) -> Vec<Note> {
    let mut out = Vec::new();
    for (path, size) in files {
        let why = if let Some(why) = suspicious(path) {
            why.to_owned()
        } else if !generated(eco, path) && !tracked.iter().any(|t| t == path) {
            "git doesn't track it: a leftover of this folder".to_owned()
        } else if let Some(size) = size.filter(|s| *s >= BIG) {
            format!("{} — big for a package", crate::git::human_size(size))
        } else {
            continue;
        };
        out.push(Note {
            path: path.clone(),
            why,
        });
    }
    out
}

/// The files of the project left out of the package. `manifest`: the text of `Cargo.toml` or
/// `package.json`.
pub(crate) fn missing(
    eco: Ecosystem,
    files: &[(String, Option<u64>)],
    tracked: &[String],
    manifest: &str,
) -> Vec<Note> {
    let shipped = |p: &str| files.iter().any(|(f, _)| f == p);
    let mut out = Vec::new();
    let mut note = |path: &str, why: &str| {
        if !shipped(path) && !out.iter().any(|n: &Note| n.path == path) {
            out.push(Note {
                path: path.to_owned(),
                why: why.to_owned(),
            });
        }
    };
    for t in tracked.iter().filter(|t| !t.contains('/')) {
        let upper = t.to_uppercase();
        if upper.starts_with("README") {
            note(t, "the README: the registry's page shows it");
        } else if upper.starts_with("LICENSE")
            || upper.starts_with("LICENCE")
            || upper.starts_with("COPYING")
        {
            note(
                t,
                "the licence: most licences ask to be shipped with the code",
            );
        }
    }
    match eco {
        Ecosystem::Cargo => {
            for t in tracked {
                if (t.starts_with("src/") && t.ends_with(".rs")) || t == "build.rs" {
                    note(t, "a source file: the package may not build without it");
                }
            }
            for key in ["readme", "license-file"] {
                if let Some(path) = toml_string(manifest, key) {
                    note(&path, &format!("Cargo.toml names it ({key})"));
                }
            }
        }
        Ecosystem::Npm => {
            let v: serde_json::Value = serde_json::from_str(manifest).unwrap_or_default();
            for key in ["main", "module", "types", "typings"] {
                if let Some(path) = v.get(key).and_then(|x| x.as_str()) {
                    note(
                        path.trim_start_matches("./"),
                        &format!("package.json names it ({key})"),
                    );
                }
            }
            match v.get("bin") {
                Some(serde_json::Value::String(path)) => {
                    note(path.trim_start_matches("./"), "package.json names it (bin)");
                }
                Some(serde_json::Value::Object(map)) => {
                    for path in map.values().filter_map(|x| x.as_str()) {
                        note(path.trim_start_matches("./"), "package.json names it (bin)");
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// A string value of `[package]` in `Cargo.toml`.
fn toml_string(text: &str, key: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t.split('#').next().unwrap_or(t).trim() == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((k, v)) = t.split_once('=') else {
            continue;
        };
        if k.trim() == key {
            let v = v.split('#').next().unwrap_or(v).trim();
            return ['"', '\'']
                .iter()
                .find_map(|q| v.strip_prefix(*q)?.strip_suffix(*q))
                .map(str::to_owned);
        }
    }
    None
}

/// What changed since the version published: (files added, files gone).
pub(crate) fn compare(
    eco: Ecosystem,
    files: &[(String, Option<u64>)],
    published: &[String],
) -> (Vec<String>, Vec<String>) {
    let now: Vec<&str> = files
        .iter()
        .map(|(f, _)| f.as_str())
        .filter(|f| !generated(eco, f))
        .collect();
    let before: Vec<&str> = published
        .iter()
        .map(String::as_str)
        .filter(|f| !generated(eco, f))
        .collect();
    let added = now
        .iter()
        .filter(|f| !before.contains(f))
        .map(|f| (*f).to_owned())
        .collect();
    let gone = before
        .iter()
        .filter(|f| !now.contains(f))
        .map(|f| (*f).to_owned())
        .collect();
    (added, gone)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(paths: &[&str]) -> Vec<(String, Option<u64>)> {
        paths.iter().map(|p| ((*p).to_owned(), None)).collect()
    }

    fn s(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| (*p).to_owned()).collect()
    }

    #[test]
    fn files_shipped_by_accident() {
        let files = vec![
            (".env".to_owned(), None),
            ("src/lib.rs".to_owned(), None),
            ("notes ü.txt".to_owned(), None),
            ("Cargo.toml.orig".to_owned(), None),
            ("assets/big.bin".to_owned(), Some(5 * 1024 * 1024)),
            ("target/debug/x".to_owned(), None),
        ];
        let tracked = s(&[".env", "src/lib.rs", "assets/big.bin", "target/debug/x"]);
        let notes = accidental(Ecosystem::Cargo, &files, &tracked);
        let paths: Vec<&str> = notes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(
            paths,
            [".env", "notes ü.txt", "assets/big.bin", "target/debug/x"]
        );
        assert!(notes[1].why.contains("doesn't track"));
        assert!(
            notes[2].why.contains("5.0 MB") || notes[2].why.contains("MB"),
            "{:?}",
            notes[2]
        );
    }

    #[test]
    fn files_left_out() {
        let manifest =
            "[package]\nname = \"app\"\nreadme = \"docs/README.md\"\nlicense-file = 'LICENSE'\n";
        let files = f(&["Cargo.toml", "src/lib.rs"]);
        let tracked = s(&[
            "README.md",
            "LICENSE",
            "src/lib.rs",
            "src/extra.rs",
            "build.rs",
            "tests/a.rs",
        ]);
        let notes = missing(Ecosystem::Cargo, &files, &tracked, manifest);
        let paths: Vec<&str> = notes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "README.md",
                "LICENSE",
                "src/extra.rs",
                "build.rs",
                "docs/README.md"
            ]
        );
        let pkg = r#"{"main":"./dist/index.js","types":"dist/index.d.ts","bin":{"x":"bin/x.js"}}"#;
        let notes = missing(
            Ecosystem::Npm,
            &f(&["dist/index.js", "package.json"]),
            &s(&[]),
            pkg,
        );
        let paths: Vec<&str> = notes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(paths, ["dist/index.d.ts", "bin/x.js"]);
    }

    #[test]
    fn against_the_version_published() {
        let (added, gone) = compare(
            Ecosystem::Cargo,
            &f(&["Cargo.toml", "src/lib.rs", "src/new.rs"]),
            &s(&["Cargo.toml", "Cargo.toml.orig", "src/lib.rs", "src/old.rs"]),
        );
        assert_eq!(added, ["src/new.rs"]);
        assert_eq!(gone, ["src/old.rs"]);
        assert_eq!(compare(Ecosystem::Npm, &[], &[]), (vec![], vec![]));
    }
}
