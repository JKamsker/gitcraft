//! Deliberate filter-repo cleanup of a newly created, exclusively owned local mirror.
//! No --force, shell interpolation, publishing or edits to the source repository.
use super::command::bounded;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
const LIMIT: usize = 4 << 20;
const RULES: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Replacement {
    pub find: String,
    pub replace: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Config {
    pub paths: Vec<String>,
    pub replacements: Vec<Replacement>,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.paths.is_empty() && self.replacements.is_empty() {
            return Err("Choose paths to remove or literal text replacements".into());
        }
        if self.paths.len() > RULES || self.replacements.len() > RULES {
            return Err("Use at most 32 paths and 32 replacements".into());
        }
        let mut bytes = 0;
        for path in &self.paths {
            let trimmed = path.strip_suffix('/').unwrap_or(path);
            super::validate_folders(&[trimmed.to_owned()])?;
            bytes += path.len();
        }
        for rule in &self.replacements {
            if rule.find.is_empty()
                || rule.find.contains("==>")
                || rule.replace.contains("==>")
                || [&rule.find, &rule.replace]
                    .iter()
                    .any(|s| s.len() > 4096 || s.contains(['\0', '\r', '\n']))
            {
                return Err("Replacements need nonempty literal text on one line, at most 4096 bytes per field; neither field can contain ==>".into());
            }
            bytes += rule.find.len() + rule.replace.len();
        }
        if bytes > 64 << 10 {
            return Err("Cleanup configuration exceeds 64 KiB".into());
        }
        Ok(())
    }
    // Upstream get_replace_text splits on the LAST ==>; reject that delimiter in both fields.
    // https://github.com/newren/git-filter-repo/blob/main/git-filter-repo
    fn replacement_text(&self) -> String {
        self.replacements
            .iter()
            .map(|r| format!("literal:{}==>{}\n", r.find, r.replace))
            .collect()
    }
    fn args(&self, file: Option<&Path>) -> Result<Vec<String>, String> {
        self.validate()?;
        let mut args = vec!["filter-repo".into()];
        if !self.paths.is_empty() {
            args.push("--invert-paths".into());
            args.extend(self.paths.iter().map(|p| format!("--path={p}")));
        }
        if !self.replacements.is_empty() {
            let file = file
                .and_then(Path::to_str)
                .ok_or("Replacement file path must be UTF-8")?;
            args.extend(["--replace-text".into(), file.into()]);
        }
        Ok(args)
    }
    fn redact(&self, text: &str) -> String {
        // One pass over the original text: never re-redact inserted markers, and never
        // replace the empty string (deletion rules commonly have an empty replacement).
        let mut patterns: Vec<_> = self
            .replacements
            .iter()
            .flat_map(|r| [&r.find, &r.replace])
            .filter(|s| !s.is_empty())
            .collect();
        patterns.sort_by_key(|s| std::cmp::Reverse(s.len()));
        let mut out = String::new();
        let mut at = 0;
        while at < text.len() && out.len() < LIMIT {
            if let Some(pattern) = patterns.iter().find(|p| text[at..].starts_with(p.as_str())) {
                out.push_str("[replacement text hidden]");
                at += pattern.len();
            } else if let Some(c) = text[at..].chars().next() {
                out.push(c);
                at += c.len_utf8();
            }
        }
        if at < text.len() {
            out.push_str("\n[output truncated]");
        }
        out
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefChange {
    pub name: String,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Check {
    pub label: String,
    pub passed: bool,
    pub detail: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Report {
    pub repo: PathBuf,
    pub changes: Vec<RefChange>,
    pub checks: Vec<Check>,
    pub output: String,
}
fn git(repo: &Path, args: &[&str], timeout: u64) -> Result<String, String> {
    let bytes = bounded::local(repo, args, Vec::new(), LIMIT, Duration::from_secs(timeout))?;
    String::from_utf8(bytes).map_err(|_| "Cleanup output was not UTF-8".into())
}
pub(crate) fn capability(repo: &Path) -> Result<String, String> {
    if crate::remote::is_remote(repo) {
        return Err("This cleanup wizard supports local repositories only".into());
    }
    let version = git(repo, &["filter-repo", "--version"], 10)?;
    if version.trim().is_empty() {
        return Err("filter-repo did not report its installed version".into());
    }
    Ok(version.trim().chars().take(128).collect())
}
fn refs(repo: &Path) -> Result<BTreeMap<String, String>, String> {
    parse_refs(&git(
        repo,
        &["for-each-ref", "--format=%(refname)%00%(objectname)"],
        30,
    )?)
}
fn parse_refs(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut refs = BTreeMap::new();
    for line in text.lines() {
        let (name, oid) = line.split_once('\0').ok_or("Invalid ref report")?;
        if !name.starts_with("refs/")
            || !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|c| c.is_ascii_hexdigit())
            || refs.insert(name.into(), oid.into()).is_some()
        {
            return Err("Invalid or repeated ref report".into());
        }
        if refs.len() > 20_000 {
            return Err("Cleanup supports at most 20000 refs".into());
        }
    }
    Ok(refs)
}
fn changes(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Vec<RefChange> {
    let names: std::collections::BTreeSet<_> = before.keys().chain(after.keys()).collect();
    names
        .into_iter()
        .filter(|name| before.get(*name) != after.get(*name))
        .map(|name| RefChange {
            name: name.clone(),
            before: before.get(name).cloned(),
            after: after.get(name).cloned(),
        })
        .collect()
}
fn workspace(parent: &Path) -> Result<PathBuf, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = parent
        .canonicalize()
        .map_err(|e| format!("Choose an existing local output folder: {e}"))?;
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    for _ in 0..100 {
        let path = parent.join(format!(
            "gitcraft-cleanup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(format!("Could not create an owned cleanup folder: {e}")),
        }
    }
    Err("Could not allocate a fresh cleanup folder".into())
}
struct ReplacementFile(PathBuf);
impl Drop for ReplacementFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn replacement_file(root: &Path, config: &Config) -> Result<ReplacementFile, String> {
    use std::io::Write;
    let path = root.join("replacement-rules");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|e| e.to_string())?;
    let owned = ReplacementFile(path);
    file.write_all(config.replacement_text().as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(owned)
}
/// All process work uses the existing bounded managed runner and the ambient task token.
/// Retain the owned clone on success/failure/cancel; never recursively remove user data.
pub(crate) fn execute(source: &Path, parent: &Path, config: &Config) -> Result<Report, String> {
    config.validate()?;
    capability(source)?;
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    let root = workspace(parent)?;
    execute_owned(&source, &root, config).map_err(|e| {
        format!(
            "Cleanup stopped. Owned artifacts remain at {}. {}",
            root.display(),
            config.redact(&e)
        )
    })
}
fn execute_owned(source: &Path, root: &Path, config: &Config) -> Result<Report, String> {
    let repo = root.join("rewritten.git");
    let templates = root.join("empty-templates");
    let hooks = root.join("empty-hooks");
    std::fs::create_dir(&templates).map_err(|e| e.to_string())?;
    std::fs::create_dir(&hooks).map_err(|e| e.to_string())?;
    let text = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or("Cleanup paths must be UTF-8".to_owned())
    };
    let source = text(source)?;
    let destination = text(&repo)?;
    let templates_arg = format!("--template={}", text(&templates)?);
    let hooks_arg = format!("core.hooksPath={}", text(&hooks)?);
    // No hardlinks or alternates: the source's objects are never rewritten by this tool.
    git(
        root,
        &[
            "-c",
            &hooks_arg,
            "clone",
            "--mirror",
            "--no-local",
            "--no-hardlinks",
            &templates_arg,
            "--",
            &source,
            &destination,
        ],
        600,
    )?;
    let before = refs(&repo)?;
    if before.is_empty() {
        return Err("The source has no refs to rewrite".into());
    }
    let rules = if config.replacements.is_empty() {
        None
    } else {
        Some(replacement_file(root, config)?)
    };
    let args = config.args(rules.as_ref().map(|f| f.0.as_path()))?;
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    let result = git(&repo, &args, 600);
    drop(rules); // Do not leave a plaintext copy of sensitive replacement rules on disk.
    let output = config.redact(&result?);
    let mut checks = Vec::new();
    let fsck = git(&repo, &["fsck", "--full", "--no-reflogs"], 600);
    checks.push(Check {
        label: "Object integrity".into(),
        passed: fsck.is_ok(),
        detail: config.redact(&fsck.unwrap_or_else(|e| e)),
    });
    for path in &config.paths {
        let answer = git(
            &repo,
            &[
                "--literal-pathspecs",
                "log",
                "--all",
                "--format=%H",
                "--",
                path,
            ],
            120,
        );
        checks.push(check_result(format!("Removed path: {path}"), answer));
    }
    for (index, rule) in config.replacements.iter().enumerate() {
        let pickaxe = format!("-S{}", rule.find);
        let answer = git(
            &repo,
            &[
                "log",
                "--all",
                "--format=%H",
                "--no-ext-diff",
                "--no-textconv",
                &pickaxe,
                "--",
            ],
            120,
        );
        checks.push(check_result(
            format!("Replacement {} residual search", index + 1),
            answer.map_err(|e| config.redact(&e)),
        ));
    }
    // filter-repo normally removes origin; enforce the separate publication boundary.
    let remotes = git(&repo, &["remote"], 30)?;
    if remotes.lines().any(|r| r == "origin") {
        git(&repo, &["remote", "remove", "origin"], 30)?;
    }
    let after = refs(&repo)?;
    Ok(Report {
        repo,
        changes: changes(&before, &after),
        checks,
        output,
    })
}
fn check_result(label: String, result: Result<String, String>) -> Check {
    match result {
        Ok(s) if s.trim().is_empty() => Check {
            label,
            passed: true,
            detail: "No matching reachable history found".into(),
        },
        Ok(_) => Check {
            label,
            passed: false,
            detail: "Matching reachable history remains; inspect before publishing".into(),
        },
        Err(e) => Check {
            label,
            passed: false,
            detail: e,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_is_bounded_and_literal_without_force_or_publication() -> Result<(), String> {
        let config = Config {
            paths: vec!["-old/secret".into(), "legacy/".into()],
            replacements: vec![Replacement {
                find: "regex:token.*".into(),
                replace: "gone$1".into(),
            }],
        };
        let args = config.args(Some(Path::new("/recorded/rules")))?;
        assert_eq!(
            args,
            [
                "filter-repo",
                "--invert-paths",
                "--path=-old/secret",
                "--path=legacy/",
                "--replace-text",
                "/recorded/rules"
            ]
        );
        assert_eq!(
            config.replacement_text(),
            "literal:regex:token.*==>gone$1\n"
        );
        for path in ["../secret", "/secret", ".git/config"] {
            assert!(
                Config {
                    paths: vec![path.into()],
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(Config::default().validate().is_err());
        assert!(
            Config {
                paths: vec!["a".into(); 33],
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn ref_report_includes_deleted_added_and_rewritten_refs() -> Result<(), String> {
        let a = "a".repeat(40);
        let b = "b".repeat(64);
        let before = parse_refs(&format!("refs/heads/main\0{a}\nrefs/tags/old\0{a}\n"))?;
        let after = parse_refs(&format!("refs/heads/main\0{b}\nrefs/tags/new\0{b}\n"))?;
        let changed = changes(&before, &after);
        assert_eq!(changed.len(), 3);
        assert_eq!(changed[0].before.as_deref(), Some(a.as_str()));
        assert_eq!(changed[0].after.as_deref(), Some(b.as_str()));
        assert!(changed[1].before.is_none());
        assert!(changed[2].after.is_none());
        assert!(parse_refs("refs/heads/main\0not-a-hash\n").is_err());
        Ok(())
    }
    #[test]
    fn result_checks_do_not_claim_unverified_cleanliness_and_hide_rule_text() {
        assert!(!check_result("search".into(), Err("tool failed".into())).passed);
        assert!(!check_result("search".into(), Ok("some history".into())).passed);
        assert!(check_result("search".into(), Ok("".into())).passed);
        let config = Config {
            replacements: vec![Replacement {
                find: "secret-value".into(),
                replace: "removed".into(),
            }],
            ..Default::default()
        };
        assert!(
            !config
                .redact("secret-value removed")
                .contains("secret-value")
        );
    }
}

/// Parent-owned real-tool validation; compile only under the global runtime hold.
#[cfg(test)]
mod runtime_held_tests {
    use super::*;
    #[test]
    fn actual_filter_repo_rewrites_only_the_owned_fresh_clone_and_reports_refs()
    -> Result<(), String> {
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "gitcraft-cleanup-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let owned = Owned(root);
        let source = owned.0.join("source");
        std::fs::create_dir(&source).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            super::super::run(Some(&source), &args)?;
        }
        std::fs::write(source.join("secrets.txt"), "fixture-key").map_err(|e| e.to_string())?;
        std::fs::write(source.join("config.txt"), "fixture-token").map_err(|e| e.to_string())?;
        super::super::run(Some(&source), &["add", "."])?;
        super::super::run(Some(&source), &["commit", "-qm", "fixture"])?;
        let before = refs(&source)?;
        let report = execute(
            &source,
            &owned.0,
            &Config {
                paths: vec!["secrets.txt".into()],
                replacements: vec![Replacement {
                    find: "fixture-token".into(),
                    replace: "redacted".into(),
                }],
            },
        )?;
        assert_eq!(refs(&source)?, before);
        assert!(!report.changes.is_empty());
        assert!(
            report.checks.iter().all(|c| c.passed),
            "{:?}",
            report.checks
        );
        assert_eq!(
            git(&report.repo, &["show", "HEAD:config.txt"], 30)?.trim(),
            "redacted"
        );
        assert!(git(&report.repo, &["show", "HEAD:secrets.txt"], 30).is_err());
        assert!(git(&report.repo, &["remote"], 30)?.trim().is_empty());
        assert!(
            !report
                .repo
                .parent()
                .ok_or("No owned root")?
                .join("replacement-rules")
                .exists()
        );
        Ok(())
    }
}
