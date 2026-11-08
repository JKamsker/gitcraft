//! Allowlisted diagnostics for copies the user explicitly links. No environment, remote
//! URLs, credential configuration or arbitrary configuration dump is collected.
use super::command::bounded;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
const LIMIT: usize = 4 << 20;
const LINKS: &str = "gitcraft.linkedRepository";
const SETTINGS: [&str; 6] = [
    "core.autocrlf",
    "core.eol",
    "core.safecrlf",
    "core.filemode",
    "core.ignorecase",
    "core.symlinks",
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackedFact {
    pub path: String,
    pub oid: String,
    pub mode: String,
    pub endings: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MachineFacts {
    pub repository: PathBuf,
    pub machine: String,
    pub head: String,
    pub tree: String,
    pub content_digest: String,
    pub clean: bool,
    pub files: Vec<TrackedFact>,
    pub case_collisions: Vec<Vec<String>>,
    pub mode_changes: String,
    pub settings: BTreeMap<String, String>,
    pub versions: BTreeMap<String, String>,
    pub test_command_digest: String,
    pub stored_test: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentComparison {
    Match,
    Different,
    Changed,
    Incompatible,
}
pub fn compare_machine_content(a: &MachineFacts, b: &MachineFacts) -> ContentComparison {
    if !a.clean || !b.clean {
        ContentComparison::Changed
    } else if a.head.len() != b.head.len() {
        ContentComparison::Incompatible
    } else if a.content_digest == b.content_digest {
        ContentComparison::Match
    } else {
        ContentComparison::Different
    }
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes)
        .map_err(|_| crate::i18n::text("backend-diagnostic-is-not-utf-8-2fcd9d").into())
}
fn read(repo: &Path, no_lazy: &[&str], args: &[&str]) -> Result<Vec<u8>, String> {
    let args: Vec<_> = no_lazy
        .iter()
        .copied()
        .chain(args.iter().copied())
        .collect();
    bounded::run(repo, &args, vec![], LIMIT)
}
pub fn linked_repositories(repo: &Path) -> Result<Vec<PathBuf>, String> {
    let out = super::run_ok_codes(
        Some(repo),
        &["config", "--local", "--get-all", LINKS],
        &[0, 1],
    )?;
    let mut links = vec![repo.to_owned()];
    for path in out.lines() {
        let path = link_path(path)?;
        if !links.contains(&path) {
            links.push(path);
        }
    }
    if links.len() > 8 {
        return Err(crate::i18n::text("backend-link-at-most-eight-project-copies-ad9c7c").into());
    }
    Ok(links)
}
fn link_path(path: &str) -> Result<PathBuf, String> {
    let path = path.trim();
    if path.is_empty() || path.len() > 4096 || path.contains(['\n', '\r', '\0']) {
        return Err(crate::i18n::text(
            "backend-give-a-full-local-wsl-or-ssh-host-path-repository-path-9bdfe6",
        )
        .into());
    }
    let p = PathBuf::from(path);
    if crate::remote::parse(&p).is_none() && !p.is_absolute() {
        return Err(
            crate::i18n::text("backend-the-linked-copy-needs-an-absolute-path-8680e7").into(),
        );
    }
    Ok(p)
}
pub fn link_repository(repo: &Path, path: &str) -> Result<(), String> {
    let path = link_path(path)?;
    let mut links = linked_repositories(repo)?;
    if links.contains(&path) {
        return Ok(());
    }
    if links.len() >= 8 {
        return Err(crate::i18n::text("backend-link-at-most-eight-project-copies-ad9c7c").into());
    }
    // No remote connection is opened by saving a link. Reading/running is a separate click.
    links.push(path.clone());
    super::run(
        Some(repo),
        &["config", "--local", "--add", LINKS, &path.to_string_lossy()],
    )
    .map(|_| ())
}
pub fn unlink_repository(repo: &Path, path: &Path) -> Result<(), String> {
    if repo == path {
        return Err(crate::i18n::text(
            "backend-the-open-repository-stays-in-this-comparison-5e02ae",
        )
        .into());
    }
    super::run_ok_codes(
        Some(repo),
        &[
            "config",
            "--local",
            "--fixed-value",
            "--unset-all",
            LINKS,
            &path.to_string_lossy(),
        ],
        &[0, 5],
    )
    .map(|_| ())
}
pub fn machine_facts(repo: &Path) -> Result<MachineFacts, String> {
    let no_lazy = super::bundles::no_lazy_fetch(repo)?;
    collect_facts(repo, |args| read(repo, &no_lazy, args))
}
fn collect_facts(
    repo: &Path,
    mut query: impl FnMut(&[&str]) -> Result<Vec<u8>, String>,
) -> Result<MachineFacts, String> {
    let head = text(query(&["rev-parse", "--verify", "HEAD"])?)?
        .trim()
        .to_owned();
    let tree = text(query(&["rev-parse", "--verify", "HEAD^{tree}"])?)?
        .trim()
        .to_owned();
    let status = query(&[
        "status",
        "--porcelain=v1",
        "--untracked-files=no",
        "--ignore-submodules=none",
    ])?;
    let index = query(&["ls-files", "--stage", "-z"])?;
    let flags = query(&["ls-files", "-v", "-z"])?;
    let fully_checked = flags
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .all(|s| s.starts_with(b"H "));
    let endings = query(&["ls-files", "--eol", "-z"])?;
    let mut files = parse_index(&index)?;
    let mut eols = BTreeMap::new();
    for item in endings.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let s = std::str::from_utf8(item)
            .map_err(|_| crate::i18n::text("backend-tracked-paths-are-not-utf-8-3ba029"))?;
        if let Some((fact, path)) = s.split_once('\t') {
            eols.insert(path.to_owned(), fact.trim().to_owned());
        }
    }
    for file in &mut files {
        file.endings = eols
            .remove(&file.path)
            .unwrap_or_else(|| "unavailable".into());
    }
    let mode_changes = text(query(&[
        "-c",
        "core.filemode=true",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--summary",
        "--",
    ])?)?;
    let mut settings = BTreeMap::new();
    for key in SETTINGS {
        let value = text(query(&["config", "--default", "(unset)", "--get", key])?)?;
        settings.insert(key.into(), setting_value(key, value.trim()).into());
    }
    let tools = text(query(&["-c", TOOL_ALIAS, "gitcraft-machine-tools"])?)?;
    let versions = parse_versions(&tools);
    // Only the digest of this fixed, allowlisted setting leaves the collector.
    let command = query(&["config", "--default", "", "--get", "test.default.command"])?;
    let test_command_digest = digest(command.strip_suffix(b"\n").unwrap_or(&command));
    let notes = query(&[
        "for-each-ref",
        "--format=%(objectname)",
        "refs/notes/tests/default",
    ])?;
    let stored_test = if notes.is_empty() {
        None
    } else {
        let tip = text(notes)?;
        let tip = tip.trim();
        if !matches!(tip.len(), 40 | 64) || !tip.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(crate::i18n::text("backend-invalid-test-notes-ref-ca6706").into());
        }
        let listing = text(query(&["ls-tree", "-r", "-z", tip])?)?;
        if let Some(blob) = super::testlane::parse_notes_tree(&listing).get(&tree) {
            let note = text(query(&["cat-file", "blob", blob])?)?;
            super::Verdict::parse(&note).map(|v| v.word().to_owned())
        } else {
            None
        }
    };
    // A concurrent edit/ref/index change makes this a changed sample, never a match claim.
    let clean = fully_checked
        && status.is_empty()
        && query(&[
            "status",
            "--porcelain=v1",
            "--untracked-files=no",
            "--ignore-submodules=none",
        ])? == status
        && query(&["ls-files", "--stage", "-z"])? == index
        && text(query(&["rev-parse", "--verify", "HEAD"])?)?.trim() == head;
    let mut canonical = Vec::new();
    for file in &files {
        canonical.extend(file.path.as_bytes());
        canonical.push(0);
        canonical.extend(file.oid.as_bytes());
        canonical.push(0);
    }
    let mut cases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in &files {
        cases
            .entry(file.path.to_lowercase())
            .or_default()
            .push(file.path.clone());
    }
    Ok(MachineFacts {
        repository: repo.into(),
        machine: crate::teleport::Machine::of(repo).label(),
        head,
        tree,
        content_digest: digest(&canonical),
        clean,
        files,
        case_collisions: cases
            .into_values()
            .filter(|paths| paths.len() > 1)
            .collect(),
        mode_changes,
        settings,
        versions,
        test_command_digest,
        stored_test,
    })
}
fn parse_index(bytes: &[u8]) -> Result<Vec<TrackedFact>, String> {
    let mut files = BTreeMap::new();
    for item in bytes.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let s = std::str::from_utf8(item)
            .map_err(|_| crate::i18n::text("backend-tracked-paths-are-not-utf-8-3ba029"))?;
        let (metadata, path) = s.split_once('\t').ok_or(crate::i18n::text(
            "backend-malformed-tracked-file-metadata-27f11d",
        ))?;
        let words: Vec<_> = metadata.split_ascii_whitespace().collect();
        let [mode, oid, stage] = words.as_slice() else {
            return Err(crate::i18n::text("backend-malformed-index-entry-a3e1c4").into());
        };
        if *stage != "0" {
            return Err(crate::i18n::text(
                "backend-resolve-index-conflicts-before-comparing-tracked-content-3231c1",
            )
            .into());
        }
        if !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|b| b.is_ascii_hexdigit())
            || !matches!(*mode, "100644" | "100755" | "120000" | "160000")
            || path.is_empty()
        {
            return Err(crate::i18n::text("backend-invalid-tracked-file-identity-8be39a").into());
        }
        if files
            .insert(
                path.to_owned(),
                TrackedFact {
                    path: path.into(),
                    oid: (*oid).into(),
                    mode: (*mode).into(),
                    endings: String::new(),
                },
            )
            .is_some()
        {
            return Err(crate::i18n::text("backend-duplicate-tracked-path-2ffeff").into());
        }
        if files.len() > 20000 {
            return Err(crate::i18n::text(
                "backend-comparison-is-limited-to-20000-tracked-files-438bab",
            )
            .into());
        }
    }
    Ok(files.into_values().collect())
}
fn setting_value<'a>(key: &str, value: &'a str) -> &'a str {
    if !SETTINGS.contains(&key) {
        return crate::i18n::text("backend-nonstandard-value-not-collected-bd25b0");
    }
    let allowed = match key {
        "core.autocrlf" => matches!(value, "true" | "false" | "input" | "(unset)"),
        "core.eol" => matches!(value, "lf" | "crlf" | "native" | "(unset)"),
        "core.safecrlf" => matches!(value, "true" | "false" | "warn" | "(unset)"),
        _ => matches!(value, "true" | "false" | "(unset)"),
    };
    if allowed {
        value
    } else {
        crate::i18n::text("backend-nonstandard-value-not-collected-bd25b0")
    }
}
fn parse_versions(input: &str) -> BTreeMap<String, String> {
    let allowed = BTreeSet::from(["git", "python3", "rustc", "cargo", "node"]);
    let mut result = BTreeMap::new();
    for line in input.lines() {
        let Some((tool, value)) = line.split_once('\t') else {
            continue;
        };
        if allowed.contains(tool)
            && value.len() <= 200
            && value.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
        {
            let prefix = match tool {
                "git" => "git version ",
                "python3" => "Python ",
                "rustc" => "rustc ",
                "cargo" => "cargo ",
                "node" => "v",
                _ => continue,
            };
            if let Some(version) = value
                .strip_prefix(prefix)
                .and_then(|s| s.split_ascii_whitespace().next())
                && version.starts_with(|c: char| c.is_ascii_digit())
                && version
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
            {
                result.insert(tool.into(), format!("{prefix}{version}"));
            }
        }
    }
    result
}
const TOOL_ALIAS: &str = "alias.gitcraft-machine-tools=!f() { for tool in git python3 rustc cargo node; do if command -v \"$tool\" >/dev/null 2>&1; then printf '%s\\t' \"$tool\"; \"$tool\" --version 2>/dev/null | head -c 200; printf '\\n'; fi; done; }; f";
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_collector_uses_fixed_queries_and_excludes_raw_command_and_versions_suffix()
    -> Result<(), String> {
        fn collect(flag: &str) -> Result<MachineFacts, String> {
            let mut unexpected = Vec::new();
            let result = collect_facts(Path::new("/synthetic/repo"), |args| {
                Ok(match args {
                    ["rev-parse", "--verify", "HEAD"] => format!("{}\n", "a".repeat(40)),
                    ["rev-parse", "--verify", "HEAD^{tree}"] => format!("{}\n", "b".repeat(40)),
                    ["ls-files", "--stage", "-z"] => format!("100644 {} 0\tfile\0", "c".repeat(40)),
                    ["ls-files", "-v", "-z"] => format!("{flag} file\0"),
                    ["ls-files", "--eol", "-z"] => "i/lf w/crlf attr/\tfile\0".into(),
                    [
                        "status",
                        "--porcelain=v1",
                        "--untracked-files=no",
                        "--ignore-submodules=none",
                    ] => String::new(),
                    [
                        "-c",
                        "core.filemode=true",
                        "diff",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--summary",
                        "--",
                    ] => String::new(),
                    ["config", "--default", "(unset)", "--get", key] if SETTINGS.contains(key) => {
                        "secret-config-value\n".into()
                    }
                    ["config", "--default", "", "--get", "test.default.command"] => {
                        "secret-test-command\n".into()
                    }
                    ["-c", TOOL_ALIAS, "gitcraft-machine-tools"] => {
                        "git\tgit version 2.50.1 secret-tool-suffix\nenv\tsecret-environment\n"
                            .into()
                    }
                    [
                        "for-each-ref",
                        "--format=%(objectname)",
                        "refs/notes/tests/default",
                    ] => String::new(),
                    _ => {
                        unexpected.push(format!("{args:?}"));
                        return Err(format!("Unexpected collector command: {args:?}"));
                    }
                }
                .into_bytes())
            });
            assert!(
                unexpected.is_empty(),
                "Unexpected collector commands: {unexpected:?}"
            );
            result
        }
        let facts = collect("H")?;
        assert!(facts.clean);
        assert_eq!(facts.files[0].endings, "i/lf w/crlf attr/");
        assert_eq!(facts.versions["git"], "git version 2.50.1");
        assert!(facts.stored_test.is_none());
        let json = serde_json::to_string(&facts).map_err(|error| error.to_string())?;
        assert!(!json.contains("secret-"));
        assert!(!collect("h")?.clean); // assume-unchanged is not a verified checkout.
        assert!(!collect("S")?.clean); // sparse/skip-worktree is not a verified checkout.
        Ok(())
    }
    fn sample() -> MachineFacts {
        MachineFacts {
            repository: PathBuf::from("/synthetic/repo"),
            machine: "synthetic".into(),
            head: "a".repeat(40),
            tree: "b".repeat(40),
            content_digest: "content".into(),
            clean: true,
            files: Vec::new(),
            case_collisions: Vec::new(),
            mode_changes: String::new(),
            settings: BTreeMap::new(),
            versions: BTreeMap::new(),
            test_command_digest: "command".into(),
            stored_test: None,
        }
    }
    #[test]
    fn recording_content_comparison_never_guesses_dirty_or_incompatible_content() {
        let a = sample();
        let mut b = a.clone();
        b.head = "c".repeat(40); // Different commit identity can preserve tracked content.
        b.settings.insert("core.autocrlf".into(), "true".into());
        assert_eq!(compare_machine_content(&a, &b), ContentComparison::Match);
        b.content_digest = "changed".into();
        assert_eq!(
            compare_machine_content(&a, &b),
            ContentComparison::Different
        );
        b.head = "c".repeat(64);
        assert_eq!(
            compare_machine_content(&a, &b),
            ContentComparison::Incompatible
        );
        b.clean = false;
        assert_eq!(compare_machine_content(&a, &b), ContentComparison::Changed);
    }
    #[test]
    fn recording_index_comparison_keeps_modes_and_case_distinctions() -> Result<(), String> {
        let files = parse_index(
            format!(
                "100755 {} 0\tFile\0100644 {} 0\tfile\0",
                "a".repeat(40),
                "a".repeat(40)
            )
            .as_bytes(),
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].mode, "100755");
        assert_eq!(files[1].path, "file");
        assert!(parse_index(b"malformed\0").is_err());
        assert!(link_path("//wsl.localhost/Ubuntu/home/me/repo").is_ok());
        assert!(link_path("ssh://host/path\ncredential").is_err());
        Ok(())
    }
    #[test]
    fn recording_machine_facts_never_admit_credentials_or_unknown_settings() {
        assert_eq!(
            setting_value("core.eol", "secret"),
            "nonstandard value (not collected)"
        );
        assert_eq!(
            setting_value("credential.helper", "true"),
            "nonstandard value (not collected)"
        );
        let versions = parse_versions(
            "env\tSECRET=value\ngit\tgit version 2.50.1 secret=value\nnode\tv22.1.0\npython3\tcredential-secret\ncargo\tcargo 1.95.0\n",
        );
        assert_eq!(versions.len(), 3);
        assert_eq!(versions["git"], "git version 2.50.1");
        assert!(!versions.contains_key("env"));
        assert!(!versions.contains_key("python3"));
        assert!(parse_index(format!("100644 {} 1\tfile\0", "a".repeat(40)).as_bytes()).is_err());
        assert!(link_path("../outside").is_err());
        assert!(link_path("ssh://build/home/me/repo").is_ok());
    }
}
