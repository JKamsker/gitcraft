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
            return Err(crate::i18n::text(
                "backend-choose-paths-to-remove-or-literal-text-replacements-7b28de",
            )
            .into());
        }
        if self.paths.len() > RULES || self.replacements.len() > RULES {
            return Err(crate::i18n::text(
                "backend-use-at-most-32-paths-and-32-replacements-7f0e68",
            )
            .into());
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
                return Err(crate::i18n::text("backend-replacements-need-nonempty-literal-text-on-one-line-at-most-4096-69e0a3").into());
            }
            bytes += rule.find.len() + rule.replace.len();
        }
        if bytes > 64 << 10 {
            return Err(
                crate::i18n::text("backend-cleanup-configuration-exceeds-64-kib-3c65fe").into(),
            );
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
            let file = file.and_then(Path::to_str).ok_or(crate::i18n::text(
                "backend-replacement-file-path-must-be-utf-8-450541",
            ))?;
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
                out.push_str(crate::i18n::text("backend-replacement-text-hidden-e095a3"));
                at += pattern.len();
            } else if let Some(c) = text[at..].chars().next() {
                out.push(c);
                at += c.len_utf8();
            }
        }
        if at < text.len() {
            out.push_str(crate::i18n::text("backend-output-truncated-b46ce7"));
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
    String::from_utf8(bytes)
        .map_err(|_| crate::i18n::text("backend-cleanup-output-was-not-utf-8-643278").into())
}
pub(crate) fn capability(repo: &Path) -> Result<String, String> {
    if crate::remote::is_remote(repo) {
        return Err(crate::i18n::text(
            "backend-this-cleanup-wizard-supports-local-repositories-only-ca84fe",
        )
        .into());
    }
    let version = git(repo, &["filter-repo", "--version"], 10)?;
    if version.trim().is_empty() {
        return Err(crate::i18n::text(
            "backend-filter-repo-did-not-report-its-installed-version-b1517a",
        )
        .into());
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
        let (name, oid) = line
            .split_once('\0')
            .ok_or(crate::i18n::text("backend-invalid-ref-report-6022e2"))?;
        if !name.starts_with("refs/")
            || !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|c| c.is_ascii_hexdigit())
            || refs.insert(name.into(), oid.into()).is_some()
        {
            return Err(crate::i18n::text("backend-invalid-or-repeated-ref-report-851574").into());
        }
        if refs.len() > 20_000 {
            return Err(
                crate::i18n::text("backend-cleanup-supports-at-most-20000-refs-3057ca").into(),
            );
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
    let parent = parent.canonicalize().map_err(|e| {
        crate::i18n::message(
            "backend-choose-an-existing-local-output-folder-e-1d8e89",
            &[("detail", (e).to_string().into())],
        )
    })?;
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
            Err(e) => {
                return Err(crate::i18n::message(
                    "backend-could-not-create-an-owned-cleanup-folder-e-dc38a2",
                    &[("detail", (e).to_string().into())],
                ));
            }
        }
    }
    Err(crate::i18n::text("backend-could-not-allocate-a-fresh-cleanup-folder-f43b39").into())
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
        crate::i18n::message(
            "backend-cleanup-stopped-owned-artifacts-remain-at-849273",
            &[
                ("path", (root.display()).to_string().into()),
                ("config", (config.redact(&e)).to_string().into()),
            ],
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
            .ok_or(crate::i18n::text("backend-cleanup-paths-must-be-utf-8-92e003").to_owned())
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
        return Err(crate::i18n::text("backend-the-source-has-no-refs-to-rewrite-bdb255").into());
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
        label: crate::i18n::text("backend-object-integrity-49cf95").into(),
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
        checks.push(check_result(
            crate::i18n::message(
                "backend-removed-path-path-abb2ac",
                &[("path", (path).to_string().into())],
            ),
            answer,
        ));
    }
    checks.extend(residual_checks(config, |args, input, limit| {
        bounded::local(&repo, args, input, limit, Duration::from_secs(120))
    }));
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
            detail: crate::i18n::text("backend-no-matching-reachable-history-found-544805").into(),
        },
        Ok(_) => Check {
            label,
            passed: false,
            detail: crate::i18n::text(
                "backend-matching-reachable-history-remains-inspect-before-publishing-74cfd1",
            )
            .into(),
        },
        Err(e) => Check {
            label,
            passed: false,
            detail: e,
        },
    }
}

// Fixed harmless argv; literal rules never leave memory during residual inspection.
// Enumerate metadata first so a large blob cannot force an unbounded batch allocation.
const SCAN_OBJECTS: usize = 20_000;
const SCAN_BLOB: usize = 4 << 20;
const SCAN_TOTAL: usize = 16 << 20;
fn residual_checks(
    config: &Config,
    mut run: impl FnMut(&[&str], Vec<u8>, usize) -> Result<Vec<u8>, String>,
) -> Vec<Check> {
    if config.replacements.is_empty() {
        return Vec::new();
    }
    let result = scan_residuals(config, &mut run);
    config.replacements.iter().enumerate().map(|(i, _)| {
        let (passed, detail) = match &result {
            Ok((hits, _)) if hits[i] => (false, crate::i18n::text("backend-literal-find-text-remains-in-reachable-text-blobs-inspect-before-dabdf8").into()),
            Ok((_, true)) => (false, crate::i18n::text("backend-unverified-binary-blobs-were-skipped-nul-in-first-8192-bytes-6ac1e7").into()),
            Ok(_) => (true, crate::i18n::text("backend-no-literal-find-text-in-reachable-text-blobs-at-most-20000-object-6fd071").into()),
            Err(error) => (false, format!("Unverified: {}", config.redact(error))),
        };
        Check { label: crate::i18n::message("backend-replacement-residual-search-99ed56", &[("index", (i + 1).to_string().into())]), passed, detail }
    }).collect()
}
fn scan_residuals(
    config: &Config,
    run: &mut impl FnMut(&[&str], Vec<u8>, usize) -> Result<Vec<u8>, String>,
) -> Result<(Vec<bool>, bool), String> {
    let objects = run(
        &["rev-list", "--objects", "--all", "--no-object-names"],
        Vec::new(),
        LIMIT,
    )?;
    let text = std::str::from_utf8(&objects)
        .map_err(|_| crate::i18n::text("backend-invalid-reachable-object-list-1be452"))?;
    let mut ids = std::collections::BTreeSet::new();
    for id in text.lines() {
        if !valid_object_id(id) {
            return Err(crate::i18n::text("backend-invalid-reachable-object-id-413f0f").into());
        }
        ids.insert(id);
        if ids.len() > SCAN_OBJECTS {
            return Err(crate::i18n::text(
                "backend-reachable-object-scan-exceeds-20000-objects-d55e4e",
            )
            .into());
        }
    }
    if ids.is_empty() {
        return Ok((vec![false; config.replacements.len()], false));
    }
    let input: Vec<u8> = ids
        .iter()
        .flat_map(|id| id.bytes().chain([b'\n']))
        .collect();
    let metadata = run(&["cat-file", "--batch-check"], input, LIMIT)?;
    let metadata = std::str::from_utf8(&metadata)
        .map_err(|_| crate::i18n::text("backend-invalid-object-metadata-cafe82"))?;
    let mut lines = metadata.lines();
    let mut blobs = Vec::new();
    let mut total = 0usize;
    for id in ids {
        let line = lines.next().ok_or(crate::i18n::text(
            "backend-incomplete-object-metadata-de847e",
        ))?;
        let fields: Vec<_> = line.split(' ').collect();
        if fields.len() != 3 || fields[0] != id {
            return Err(
                crate::i18n::text("backend-missing-or-mismatched-reachable-object-6c3d8d").into(),
            );
        }
        let size = fields[2]
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-object-size-8e8433"))?;
        match fields[1] {
            "blob" => {
                if size > SCAN_BLOB {
                    return Err(
                        crate::i18n::text("backend-text-scan-exceeds-4-mib-per-blob-inspect-larger-blobs-separately-4b331a").into(),
                    );
                }
                total = total.checked_add(size).ok_or(crate::i18n::text(
                    "backend-object-sizes-overflow-the-scan-budget-f89027",
                ))?;
                if total > SCAN_TOTAL {
                    return Err(
                        crate::i18n::text("backend-text-scan-exceeds-16-mib-total-inspect-the-remaining-history-sepa-a0b054")
                            .into(),
                    );
                }
                blobs.push((id.to_owned(), size));
            }
            "tree" | "commit" | "tag" => (),
            _ => {
                return Err(
                    crate::i18n::text("backend-unknown-reachable-object-type-1850cd").into(),
                );
            }
        }
    }
    if lines.next().is_some() {
        return Err(crate::i18n::text("backend-unexpected-object-metadata-d1d00f").into());
    }
    let input = blobs
        .iter()
        .flat_map(|(id, _)| id.bytes().chain([b'\n']))
        .collect();
    if blobs.is_empty() {
        return Ok((vec![false; config.replacements.len()], false));
    }
    let bytes = run(&["cat-file", "--batch"], input, total + blobs.len() * 96)?;
    let matchers: Vec<_> = config
        .replacements
        .iter()
        .map(|rule| Literal::new(rule.find.as_bytes()))
        .collect();
    let mut hits = vec![false; matchers.len()];
    let mut skipped_binary = false;
    let mut rest = bytes.as_slice();
    for (id, size) in blobs {
        let end = rest
            .iter()
            .position(|b| *b == b'\n')
            .ok_or(crate::i18n::text("backend-missing-blob-header-3b114f"))?;
        let header = format!("{id} blob {size}");
        if rest[..end] != *header.as_bytes() {
            return Err(crate::i18n::text("backend-mismatched-blob-header-41e487").into());
        }
        rest = &rest[end + 1..];
        if rest.len() <= size || rest[size] != b'\n' {
            return Err(crate::i18n::text("backend-truncated-blob-stream-24db27").into());
        }
        let blob = &rest[..size];
        if blob[..blob.len().min(8192)].contains(&0) {
            skipped_binary = true;
        } else {
            for (found, matcher) in hits.iter_mut().zip(&matchers) {
                *found |= matcher.contains(blob);
            }
        }
        rest = &rest[size + 1..];
    }
    if !rest.is_empty() {
        return Err(crate::i18n::text("backend-unexpected-blob-stream-suffix-16709a").into());
    }
    Ok((hits, skipped_binary))
}
fn valid_object_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|b| b.is_ascii_hexdigit())
}
// Linear literal matching over bytes, including non-UTF-8 text; no regex interpretation.
struct Literal<'a> {
    bytes: &'a [u8],
    prefix: Vec<usize>,
}
impl<'a> Literal<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let mut prefix = vec![0; bytes.len()];
        let mut matched = 0;
        for i in 1..bytes.len() {
            while matched > 0 && bytes[i] != bytes[matched] {
                matched = prefix[matched - 1];
            }
            if bytes[i] == bytes[matched] {
                matched += 1;
            }
            prefix[i] = matched;
        }
        Self { bytes, prefix }
    }
    fn contains(&self, haystack: &[u8]) -> bool {
        if self.bytes.is_empty() {
            return false;
        } // Config rejects empty find text.
        let mut matched = 0;
        for byte in haystack {
            while matched > 0 && *byte != self.bytes[matched] {
                matched = self.prefix[matched - 1];
            }
            if *byte == self.bytes[matched] {
                matched += 1;
            }
            if matched == self.bytes.len() {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recorded_scan(
        config: &Config,
        blob: &[u8],
        size_override: Option<usize>,
        truncate: bool,
    ) -> Vec<Check> {
        let id = "a".repeat(40);
        let size = size_override.unwrap_or(blob.len());
        let mut stage = 0;
        let result = residual_checks(config, |args, input, limit| {
            // Both fields are synthetic canaries. Neither may occur in any argv.
            for rule in &config.replacements {
                assert!(!args.join(" ").contains(&rule.find));
                if !rule.replace.is_empty() {
                    assert!(!args.join(" ").contains(&rule.replace));
                }
            }
            stage += 1;
            let output = match stage {
                1 => {
                    assert_eq!(
                        args,
                        ["rev-list", "--objects", "--all", "--no-object-names"]
                    );
                    assert!(input.is_empty());
                    format!("{id}\n").into_bytes()
                }
                2 => {
                    assert_eq!(args, ["cat-file", "--batch-check"]);
                    assert_eq!(input, format!("{id}\n").as_bytes());
                    format!("{id} blob {size}\n").into_bytes()
                }
                3 => {
                    assert_eq!(args, ["cat-file", "--batch"]);
                    assert_eq!(input, format!("{id}\n").as_bytes());
                    let mut out = format!("{id} blob {size}\n").into_bytes();
                    out.extend_from_slice(blob);
                    if !truncate {
                        out.push(b'\n');
                    }
                    out
                }
                _ => return Err("Unexpected command".into()),
            };
            assert!(output.len() <= limit);
            Ok(output)
        });
        assert_eq!(stage, if size > SCAN_BLOB { 2 } else { 3 });
        result
    }
    #[test]
    fn residual_recording_keeps_both_sensitive_fields_out_of_argv() {
        let config = Config {
            paths: Vec::new(),
            replacements: vec![Replacement {
                find: "GC_REVIEW_CANARY_DO_NOT_USE".into(),
                replace: "GC_REPLACEMENT_CANARY".into(),
            }],
        };
        let hit = recorded_scan(
            &config,
            b"prefix GC_REVIEW_CANARY_DO_NOT_USE suffix",
            None,
            false,
        );
        assert!(!hit[0].passed);
        assert!(hit[0].detail.contains("remains"));
        let clean = recorded_scan(&config, b"prefix GC_REPLACEMENT_CANARY suffix", None, false);
        assert!(clean[0].passed);
        let binary = recorded_scan(&config, b"\0binary", None, false);
        assert!(!binary[0].passed);
        assert!(binary[0].detail.contains("Unverified"));
        let big = recorded_scan(&config, b"", Some(SCAN_BLOB + 1), false);
        assert!(!big[0].passed);
        assert!(big[0].detail.contains("4 MiB"));
        let truncated = recorded_scan(&config, b"text", None, true);
        assert!(!truncated[0].passed);
        assert!(truncated[0].detail.contains("Unverified"));
    }
    #[test]
    fn residual_budgets_refuse_before_blob_download_and_errors_are_redacted() {
        let config = Config {
            paths: Vec::new(),
            replacements: vec![Replacement {
                find: "SENSITIVE_FIND_CANARY".into(),
                replace: "SENSITIVE_REPLACE_CANARY".into(),
            }],
        };
        for count in [5, SCAN_OBJECTS + 1] {
            let ids: Vec<_> = (0..count).map(|i| format!("{i:040x}")).collect();
            let mut calls = 0;
            let result = residual_checks(&config, |args, _, limit| {
                calls += 1;
                let data = match args {
                    ["rev-list", ..] => format!("{}\n", ids.join("\n")),
                    ["cat-file", "--batch-check"] => ids
                        .iter()
                        .map(|id| format!("{id} blob {SCAN_BLOB}\n"))
                        .collect(),
                    _ => {
                        return Err("Unexpected blob download after budget refusal".into());
                    }
                };
                assert!(data.len() <= limit);
                Ok(data.into_bytes())
            });
            assert!(!result[0].passed);
            assert!(
                result[0]
                    .detail
                    .contains(if count == 5 { "16 MiB" } else { "20000" })
            );
            assert_eq!(calls, if count == 5 { 2 } else { 1 });
        }
        let checks = residual_checks(&config, |_, _, _| {
            Err("SENSITIVE_FIND_CANARY SENSITIVE_REPLACE_CANARY".into())
        });
        assert!(!checks[0].passed);
        assert!(!checks[0].detail.contains("SENSITIVE_FIND_CANARY"));
        assert!(!checks[0].detail.contains("SENSITIVE_REPLACE_CANARY"));
    }
    #[test]
    fn literal_scan_handles_overlaps_and_non_utf8_without_regex() {
        for (pattern, text, found) in [
            (&b"ababac"[..], &b"ababababac"[..], true),
            (b"a.*", b"abc", false),
            (b"token", b"\xfftoken\xfe", true),
            (b"token", b"toke", false),
        ] {
            assert_eq!(Literal::new(pattern).contains(text), found);
        }
    }

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
