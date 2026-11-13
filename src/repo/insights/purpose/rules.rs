//! The rules that say what a unit is for: the repository's own (`.gitcraft/review-groups` at
//! the base), then built-in ones on paths, file types, markers in the lines and the repeated
//! edits found. Each placement says which rule made it.

use super::units::{Body, Unit, Whole};

/// What a change is for, in the order a reviewer is offered them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(in crate::repo::insights) enum Purpose {
    Api,
    Migrations,
    Logic,
    Repeated,
    Tests,
    Config,
    Docs,
    Formatting,
    Moves,
    Assets,
    Generated,
    Other,
}

impl Purpose {
    pub(in crate::repo::insights) fn title(self) -> &'static str {
        match self {
            Purpose::Api => "API and contracts",
            Purpose::Migrations => "Migrations and schema",
            Purpose::Logic => "Runtime logic",
            Purpose::Repeated => "Repeated edit",
            Purpose::Tests => "Tests",
            Purpose::Config => "Build and configuration",
            Purpose::Docs => "Documentation",
            Purpose::Formatting => "Formatting only",
            Purpose::Moves => "Moves and renames",
            Purpose::Assets => "Images and other binaries",
            Purpose::Generated => "Generated, vendored and lockfiles",
            Purpose::Other => "Other changes",
        }
    }

    /// The word a rules file names it by.
    fn word(self) -> &'static str {
        match self {
            Purpose::Api => "api",
            Purpose::Migrations => "migrations",
            Purpose::Logic => "logic",
            Purpose::Repeated => "repeated",
            Purpose::Tests => "tests",
            Purpose::Config => "config",
            Purpose::Docs => "docs",
            Purpose::Formatting => "formatting",
            Purpose::Moves => "moves",
            Purpose::Assets => "assets",
            Purpose::Generated => "generated",
            Purpose::Other => "other",
        }
    }

    const NAMED: [Purpose; 10] = [
        Purpose::Api,
        Purpose::Migrations,
        Purpose::Logic,
        Purpose::Tests,
        Purpose::Config,
        Purpose::Docs,
        Purpose::Formatting,
        Purpose::Moves,
        Purpose::Assets,
        Purpose::Generated,
    ];
}

/// Where a rule of the repository puts what it matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) enum Target {
    Purpose(Purpose),
    /// A group of its own, by name (a feature across folders, say).
    Named(String),
}

/// A rule of the repository's rules file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) struct Rule {
    pub(in crate::repo::insights) target: Target,
    pub(in crate::repo::insights) patterns: Vec<String>,
    /// Its line, as written, to say why.
    pub(in crate::repo::insights) line: usize,
}

/// Reads a rules file: `purpose: pattern…` or `group Name: pattern…` per line, `#` comments.
/// Lines that aren't understood are said, not skipped silently.
pub(in crate::repo::insights) fn parse(text: &str) -> (Vec<Rule>, Vec<String>) {
    let mut rules = Vec::new();
    let mut errors = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((head, patterns)) = line.split_once(':') else {
            errors.push(format!("line {}: no `:` in “{line}”", i + 1));
            continue;
        };
        let patterns: Vec<String> = patterns.split_whitespace().map(str::to_owned).collect();
        let head = head.trim();
        let target = match head.strip_prefix("group ") {
            Some(name) if !name.trim().is_empty() => Target::Named(name.trim().to_owned()),
            _ => match Purpose::NAMED.iter().find(|p| p.word() == head) {
                Some(&p) => Target::Purpose(p),
                None => {
                    errors.push(format!("line {}: unknown purpose “{head}”", i + 1));
                    continue;
                }
            },
        };
        if patterns.is_empty() {
            errors.push(format!("line {}: no patterns", i + 1));
            continue;
        }
        rules.push(Rule {
            target,
            patterns,
            line: i + 1,
        });
    }
    (rules, errors)
}

/// A pattern without `/` matches a file's name; with one, its path (a leading `/` ignored);
/// ending in `/`, everything in that folder. `*` stays within a folder, `**` doesn't.
pub(in crate::repo::insights) fn matches(pattern: &str, path: &str) -> bool {
    if let Some(folder) = pattern.strip_suffix('/') {
        return matches(&format!("{folder}/**"), path);
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    match pattern.strip_prefix('/') {
        Some(rooted) => glob(rooted.as_bytes(), path.as_bytes()),
        None if pattern.contains('/') => glob(pattern.as_bytes(), path.as_bytes()),
        None => glob(pattern.as_bytes(), name.as_bytes()),
    }
}

fn glob(p: &[u8], t: &[u8]) -> bool {
    match p {
        [] => t.is_empty(),
        [b'*', b'*', b'/', rest @ ..] => {
            glob(rest, t) || (0..t.len()).any(|i| t[i] == b'/' && glob(rest, &t[i + 1..]))
        }
        [b'*', b'*', rest @ ..] => (0..=t.len()).any(|i| glob(rest, &t[i..])),
        [b'*', rest @ ..] => {
            let within = t.iter().position(|&b| b == b'/').unwrap_or(t.len());
            (0..=within).any(|i| glob(rest, &t[i..]))
        }
        [b'?', rest @ ..] => matches!(t.first(), Some(&c) if c != b'/') && glob(rest, &t[1..]),
        [c, rest @ ..] => t.first() == Some(c) && glob(rest, &t[1..]),
    }
}

/// Where the rules put a unit, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) struct Placement {
    pub(in crate::repo::insights) target: Target,
    pub(in crate::repo::insights) why: String,
}

fn put(purpose: Purpose, why: impl Into<String>) -> Placement {
    Placement {
        target: Target::Purpose(purpose),
        why: why.into(),
    }
}

/// Folders and names of tests.
const TEST_DIRS: [&str; 6] = [
    "tests/",
    "test/",
    "__tests__/",
    "spec/",
    "testdata/",
    "e2e/",
];
const TEST_NAMES: [&str; 7] = [
    "_test.", "_tests.", ".test.", ".spec.", "_spec.", "Test.", "Tests.",
];
const DOC_EXT: [&str; 5] = [".md", ".rst", ".adoc", ".txt", ".ftl"];
const DOC_NAMES: [&str; 5] = ["README", "CHANGELOG", "LICENSE", "CONTRIBUTING", "NOTICE"];
const CONFIG_EXT: [&str; 9] = [
    ".toml", ".yml", ".yaml", ".json", ".ini", ".cfg", ".conf", ".gradle", ".cmake",
];
const CONFIG_NAMES: [&str; 8] = [
    "Makefile",
    "Dockerfile",
    "build.rs",
    "CMakeLists.txt",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    "Justfile",
];
const API_PATHS: [&str; 6] = [
    "api/",
    "proto/",
    "openapi",
    "schema/",
    "include/",
    "contracts/",
];
const API_EXT: [&str; 4] = [".proto", ".graphql", ".d.ts", ".thrift"];
const SOURCE_EXT: [&str; 22] = [
    ".rs", ".py", ".js", ".mjs", ".ts", ".tsx", ".jsx", ".go", ".java", ".kt", ".swift", ".c",
    ".cc", ".cpp", ".h", ".hpp", ".cs", ".rb", ".php", ".scala", ".sh", ".lua",
];
const GENERATED_DIRS: [&str; 5] = [
    "vendor/",
    "third_party/",
    "node_modules/",
    "dist/",
    "generated/",
];
const GENERATED_NAMES: [&str; 4] = [".generated.", ".pb.go", "_pb2.py", ".g.dart"];

fn in_dir(path: &str, dirs: &[&str]) -> Option<String> {
    dirs.iter()
        .find(|d| path.starts_with(*d) || path.contains(&format!("/{d}")))
        .map(|d| format!("in a `{d}` folder"))
}

/// Where the rules put `unit`. `generated`: why the file folding rules fold its file, if they
/// do (`linguist-generated`, a lockfile, a minified file, a pattern of the settings).
pub(in crate::repo::insights) fn place(
    unit: &Unit,
    rules: &[Rule],
    generated: Option<&str>,
) -> Placement {
    let path = unit.path.as_str();
    for rule in rules {
        if let Some(p) = rule.patterns.iter().find(|p| matches(p, path)) {
            return Placement {
                target: rule.target.clone(),
                why: format!("`{p}` (line {} of {})", rule.line, super::units::CONFIG),
            };
        }
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if let Some(reason) = generated {
        return put(Purpose::Generated, format!("{reason} (file folding rules)"));
    }
    if let Some(why) = in_dir(path, &GENERATED_DIRS) {
        return put(Purpose::Generated, why);
    }
    if let Some(n) = GENERATED_NAMES.iter().find(|n| name.contains(*n)) {
        return put(Purpose::Generated, format!("named `*{n}*`"));
    }
    if let Body::File(Whole::NoLines) = unit.body {
        return match &unit.old_path {
            Some(old) => put(Purpose::Moves, format!("from {old}, no lines changed")),
            None => put(Purpose::Other, "no lines changed (its mode, say)"),
        };
    }
    if unit.markers.layout {
        return put(
            Purpose::Formatting,
            "only whitespace or line breaks change (Repeated edits)",
        );
    }
    if let Some((_, deviates)) = unit.repeated {
        return put(
            Purpose::Repeated,
            if deviates {
                "resembles the repeated edit but is made otherwise: check it (Repeated edits)"
            } else {
                "the same edit is made in other files (Repeated edits)"
            },
        );
    }
    by_path(unit, path, name)
}

fn by_path(unit: &Unit, path: &str, name: &str) -> Placement {
    if let Some(why) = in_dir(path, &TEST_DIRS) {
        return put(Purpose::Tests, why);
    }
    if let Some(n) = TEST_NAMES.iter().find(|n| name.contains(*n)) {
        return put(Purpose::Tests, format!("named `*{n}*`"));
    }
    if name.starts_with("test_") || name == "tests.rs" {
        return put(Purpose::Tests, "a test file by its name");
    }
    if let Some(why) = in_dir(path, &["migrations/", "migrate/"]) {
        return put(Purpose::Migrations, why);
    }
    if name.ends_with(".sql") {
        return put(Purpose::Migrations, "an SQL file");
    }
    if let Some(e) = API_EXT.iter().find(|e| name.ends_with(*e)) {
        return put(Purpose::Api, format!("a `{e}` file"));
    }
    if let Some(why) = in_dir(path, &API_PATHS) {
        return put(Purpose::Api, why);
    }
    if DOC_NAMES.iter().any(|n| name.starts_with(n)) || in_dir(path, &["docs/", "doc/"]).is_some() {
        return put(Purpose::Docs, "documentation by its name or folder");
    }
    if let Some(e) = DOC_EXT.iter().find(|e| name.ends_with(*e)) {
        return put(Purpose::Docs, format!("a `{e}` file"));
    }
    if CONFIG_NAMES.contains(&name) || path.starts_with(".github/") || path.starts_with("ci/") {
        return put(Purpose::Config, "a build or CI file by its name or folder");
    }
    if let Some(e) = CONFIG_EXT.iter().find(|e| name.ends_with(*e)) {
        return put(Purpose::Config, format!("a `{e}` file"));
    }
    if let Some(ext) = SOURCE_EXT.iter().find(|e| name.ends_with(*e)) {
        if let Some(t) = &unit.markers.test {
            return put(Purpose::Tests, format!("its lines add or remove `{t}`"));
        }
        if let Some(a) = &unit.markers.api {
            return put(Purpose::Api, format!("changes a public declaration: `{a}`"));
        }
        return put(Purpose::Logic, format!("a `{ext}` source file"));
    }
    if let Body::File(Whole::Binary) = unit.body {
        return put(Purpose::Assets, "a binary file");
    }
    put(Purpose::Other, "no rule matched")
}
