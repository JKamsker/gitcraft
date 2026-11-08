//! Local-only evidence behind an immutable commit. No transcript is sent to a model,
//! forge, MCP or process. Explicit reads are bounded and never fetch missing objects.
mod entire;
mod local;
mod parse;
#[cfg(test)]
mod tests;
use crate::git;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
const FILE_LIMIT: usize = 4 * 1024 * 1024;
const TOTAL_LIMIT: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, Default)]
pub(crate) struct Evidence {
    pub commit: String,
    pub sessions: Vec<Session>,
    pub notices: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Session {
    pub source: String,
    pub relation: String,
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug)]
pub(crate) struct Step {
    pub kind: &'static str,
    pub text: String,
    pub at: Option<i64>,
    pub path: Option<String>,
    pub before: String,
    pub after: String,
    pub exact: bool,
    pub ranges: Vec<Span>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    pub start: u32,
    pub end: u32,
    pub old: bool,
}
impl Step {
    fn new(kind: &'static str, text: String, at: Option<i64>) -> Self {
        Self {
            kind,
            text: redact(&text),
            at,
            path: None,
            before: String::new(),
            after: String::new(),
            exact: false,
            ranges: vec![],
        }
    }
}
#[derive(Clone)]
pub(crate) struct Options {
    pub include_local: bool,
    pub roots: Vec<PathBuf>,
    pub hours: u32,
}
impl Default for Options {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let mut roots = Vec::new();
        if let Some(p) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            roots.push(PathBuf::from(p).join("projects"));
        } else if let Some(h) = &home {
            roots.push(h.join(".claude/projects"));
        }
        if let Some(p) = std::env::var_os("CODEX_HOME") {
            roots.push(PathBuf::from(p).join("sessions"));
        } else if let Some(h) = home {
            roots.push(h.join(".codex/sessions"));
        }
        Self {
            include_local: false,
            roots,
            hours: 24,
        }
    }
}
fn read(repo: &Path, args: &[&str], active: &dyn Fn() -> bool) -> Result<Vec<u8>, String> {
    let args = [
        "--no-lazy-fetch",
        "--no-replace-objects",
        "-c",
        "core.fsmonitor=false",
    ]
    .into_iter()
    .chain(args.iter().copied())
    .collect::<Vec<_>>();
    git::forecast_bytes(repo,&args,&[("GIT_NO_LAZY_FETCH","1".into())],&[0],active).map_err(|_|"Local evidence is unavailable; fetch missing checkpoint objects explicitly. No network read was requested".into())
}
fn object(
    repo: &Path,
    rev: &str,
    path: &str,
    active: &dyn Fn() -> bool,
    budget: &mut usize,
) -> Result<Vec<u8>, String> {
    if !git::investigations::oid(rev)
        || path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|p| matches!(p, "." | "..") || p.is_empty())
        || path.chars().any(char::is_control)
    {
        return Err("Invalid evidence object path".into());
    }
    let spec = format!("{rev}:{path}");
    let size = read(repo, &["cat-file", "-s", &spec], active)?;
    let n = std::str::from_utf8(&size)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .ok_or("Invalid evidence size")?;
    if n > FILE_LIMIT || n > *budget {
        return Err(
            "Evidence exceeds the 4 MiB file or 32 MiB total bound; omitted, not fetched".into(),
        );
    }
    let bytes = read(repo, &["cat-file", "blob", &spec], active)?;
    if bytes.len() != n {
        return Err("Evidence object size changed unexpectedly".into());
    }
    *budget -= n;
    Ok(bytes)
}
pub(crate) fn redact(text: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    #[expect(clippy::expect_used, reason = "This fixed credential regex is validated by the pure redaction tests; preserve fail-closed initialization semantics")]
    let re=RE.get_or_init(||regex::Regex::new(r#"(?i)(?:password|passwd|secret|api[_-]?key|access[_-]?token|auth[_-]?token|authorization)\s*[:= ]\s*[\"']?\S{8,}|(?:ghp_|github_pat_|glpat-|sk-|sk_live_|xox[baprs]-|AKIA|AIza)[a-zA-Z0-9_\-]{12,}|[a-zA-Z0-9+/_=-]{48,}|[a-z][a-z0-9+.-]*://[^ /\s]+:[^ /\s]+@"#).expect("fixed credential pattern"));
    let mut private = false;
    let mut result = String::new();
    let mut kept = 0usize;
    for line in text.lines() {
        if line.contains("BEGIN ") && line.contains("PRIVATE KEY") {
            private = true;
        }
        let end = line.contains("END ") && line.contains("PRIVATE KEY");
        let value = if private || re.is_match(line) {
            "[credential-like content withheld]"
        } else {
            line
        };
        if kept + value.len() > 256 * 1024 {
            result.push_str("[display stopped at 256 KiB]\n");
            break;
        }
        result.push_str(value);
        result.push('\n');
        kept += value.len();
        if end {
            private = false;
        }
    }
    if !text.ends_with('\n') {
        result.pop();
    }
    result
}
pub(crate) fn checkpoint_ref(name: &str) -> bool {
    name == "entire/checkpoints/v1"
        || name == "refs/heads/entire/checkpoints/v1"
        || name.starts_with("refs/entire/checkpoints/")
        || name.starts_with("refs/gitcraft/agent/")
}
pub(crate) fn checkpoint_publication(
    source: &str,
    destination: &str,
    push_oid: &str,
    head: &str,
    refs: &str,
) -> bool {
    checkpoint_ref(source)
        || checkpoint_ref(destination)
        || refs
            .lines()
            .filter_map(|line| line.split_once('\0'))
            .any(|(_, oid)| oid == push_oid || oid == head)
}
pub(crate) fn entire_automatic_notice(integration: bool, skip_hooks: bool) -> Option<&'static str> {
    (integration && !skip_hooks).then_some("Entire integration is present. If automatic checkpoint synchronization is enabled, its original pre-push hook may publish prompts, local paths, summaries and tool activity alongside this ordinary code push, possibly to a separate checkpoint remote. Check Entire settings and recipients before approving. Preview does not execute hooks or determine the exact pending checkpoint set.")
}
pub(crate) fn entire_automatic_publication(
    repo: &Path,
    checkpoint_refs: &str,
    skip_hooks: bool,
    active: &dyn Fn() -> bool,
) -> Result<Option<&'static str>, String> {
    if skip_hooks {
        return Ok(None);
    }
    let mut integration = !checkpoint_refs.trim().is_empty();
    if !crate::remote::is_remote(repo) {
        // Bare repositories have no worktree settings. Their hook can still be
        // inspected below; absence of a worktree is not a push-preview failure.
        if let Ok(root) = read(repo, &["rev-parse", "--show-toplevel"], active)
            && let Ok(root) = std::str::from_utf8(&root)
        {
            for settings in [".entire/settings.json", ".entire/settings.local.json"] {
                integration |= std::fs::symlink_metadata(
                    Path::new(root.trim_end_matches(['\r', '\n'])).join(settings),
                )
                .is_ok();
            }
        }
        let hook = read(
            repo,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "hooks/pre-push",
            ],
            active,
        )?;
        let hook = std::str::from_utf8(&hook)
            .map_err(|_| "Invalid pre-push hook path")?
            .trim_end_matches(['\r', '\n']);
        if let Some(parent) = Path::new(hook).parent()
            && let Some(name) = Path::new(hook).file_name().and_then(|n| n.to_str())
            && let Some(bytes) = crate::safe_read::read_inside(parent, name, 64 * 1024)
        {
            integration |= String::from_utf8_lossy(&bytes)
                .to_ascii_lowercase()
                .contains("entire");
        }
    }
    Ok(entire_automatic_notice(integration, skip_hooks))
}
fn occurrences(content: &str, needle: &str, old: bool) -> Vec<Span> {
    if needle.is_empty() {
        return vec![];
    }
    let starts = content
        .match_indices(needle)
        .map(|(p, _)| {
            let start = content[..p].bytes().filter(|b| *b == b'\n').count() as u32 + 1;
            Span {
                start,
                end: start + needle.lines().count().max(1) as u32 - 1,
                old,
            }
        })
        .take(2)
        .collect::<Vec<_>>();
    // Repeated snippets have no unique line attribution. Keep the step, with no line jump.
    if starts.len() == 1 { starts } else { vec![] }
}
fn changed_matches(content: &str, needle: &str, other: &str, old: bool) -> Vec<Span> {
    let this = needle.lines().collect::<Vec<_>>();
    let that = other.lines().collect::<Vec<_>>();
    // Bounded LCS identifies separate changed runs, including unchanged interior
    // lines. Oversized/ambiguous snippets remain visible without line attribution.
    let Some(cells) = (this.len() + 1)
        .checked_mul(that.len() + 1)
        .filter(|n| *n <= 1_000_000)
    else {
        return vec![];
    };
    let width = that.len() + 1;
    let mut lcs = vec![0u32; cells];
    for i in (0..this.len()).rev() {
        for j in (0..that.len()).rev() {
            lcs[i * width + j] = if this[i] == that[j] {
                1 + lcs[(i + 1) * width + j + 1]
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let mut changed = vec![true; this.len()];
    let (mut i, mut j) = (0, 0);
    while i < this.len() && j < that.len() {
        if this[i] == that[j] {
            changed[i] = false;
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * width + j] >= lcs[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    let Some(origin) = occurrences(content, needle, old).into_iter().next() else {
        return vec![];
    };
    let mut spans: Vec<Span> = Vec::new();
    for (offset, is_changed) in changed.into_iter().enumerate() {
        if !is_changed {
            continue;
        }
        let line = origin.start + offset as u32;
        if let Some(last) = spans.last_mut()
            && last.end + 1 == line
        {
            last.end = line;
        } else {
            spans.push(Span {
                start: line,
                end: line,
                old,
            });
        }
    }
    spans
}
pub(crate) fn hunk_steps(
    e: &Evidence,
    path: &str,
    range: (u32, u32),
    old: bool,
) -> Vec<(usize, usize)> {
    e.sessions
        .iter()
        .enumerate()
        .flat_map(|(i, s)| {
            s.steps.iter().enumerate().filter_map(move |(j, step)| {
                (step.path.as_deref() == Some(path)
                    && step
                        .ranges
                        .iter()
                        .any(|s| s.old == old && s.start <= range.1 && range.0 <= s.end))
                .then_some((i, j))
            })
        })
        .collect()
}
pub(crate) fn load(
    repo: &Path,
    commit: &str,
    options: &Options,
    active: &dyn Fn() -> bool,
) -> Result<Evidence, String> {
    if crate::remote::is_remote(repo) {
        return Err("Prompt/session evidence is local only; open its native worktree to read local transcripts".into());
    }
    if !git::investigations::oid(commit) {
        return Err("Select an immutable commit before reading its evidence".into());
    }
    let size = read(repo, &["cat-file", "-s", commit], active)?;
    let size = std::str::from_utf8(&size)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .ok_or("Invalid commit size")?;
    if size > FILE_LIMIT {
        return Err("Commit evidence metadata exceeds 4 MiB; object was not loaded".into());
    }
    let metadata = read(
        repo,
        &[
            "show",
            "--no-show-signature",
            "-s",
            "--format=%H%x00%ct%x00%P%x00%B",
            commit,
        ],
        active,
    )?;
    if metadata.len() > FILE_LIMIT {
        return Err("Commit evidence metadata exceeds 4 MiB".into());
    }
    let metadata = std::str::from_utf8(&metadata).map_err(|_| "Commit metadata is not UTF-8")?;
    let fields = metadata.splitn(4, '\0').collect::<Vec<_>>();
    let [oid, time, parents, message] = fields.as_slice() else {
        return Err("Invalid commit metadata".into());
    };
    if *oid != commit {
        return Err("Commit identity changed".into());
    }
    let time = time.parse::<i64>().map_err(|_| "Invalid commit time")?;
    let root = read(repo, &["rev-parse", "--show-toplevel"], active)?;
    let root = std::str::from_utf8(&root)
        .map_err(|_| "Worktree path must be UTF-8")?
        .trim_end_matches(['\r', '\n']);
    let root = std::fs::canonicalize(root).map_err(|_| "Native worktree is unavailable")?;
    let mut result = Evidence {
        commit: commit.into(),
        ..Default::default()
    };
    let mut budget = TOTAL_LIMIT;
    entire::load(
        repo,
        message,
        commit,
        &root,
        active,
        &mut budget,
        &mut result,
    );
    // GitButler simple mode writes exactly prompt + blank line + change summary,
    // with no provenance marker. Preserve the complete recorded message and label
    // that unavoidable ambiguity, rather than asserting every ordinary subject is AI.
    let (prompt, summary) = message
        .trim_end()
        .split_once("\n\n")
        .unwrap_or((message.trim_end(), ""));
    let mut embedded = vec![Step::new(
        "Possible embedded prompt",
        prompt.into(),
        Some(time),
    )];
    if !summary.is_empty() {
        embedded.push(Step::new(
            "Recorded message / possible change summary",
            summary.into(),
            Some(time),
        ));
    }
    result.sessions.push(Session{source:"Commit message (GitButler-compatible layout)".into(),relation:"Unverified: GitButler simple mode has no marker; a normal subject/body has the same layout. The recorded message is preserved, not inferred agent provenance".into(),steps:embedded});
    if options.include_local {
        local::load(&root, time, options, active, &mut budget, &mut result);
    } else {
        result.notices.push("Local Claude/Codex discovery is off. Opt in and choose roots to match exact worktree and commit-time window".into());
    }
    let parent = parents.split_whitespace().next().unwrap_or("");
    let mut files = std::collections::BTreeMap::new();
    for session in &mut result.sessions {
        for step in &mut session.steps {
            if !step.exact {
                continue;
            }
            let Some(path) = &step.path else {
                continue;
            };
            if files.len() >= 100 && !files.contains_key(path) {
                result
                    .notices
                    .push("Hunk mapping stopped at 100 touched files".into());
                continue;
            }
            let contents = files.entry(path.clone()).or_insert_with(|| {
                let new = object(repo, commit, path, active, &mut budget)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok());
                let old = if parent.is_empty() {
                    None
                } else {
                    object(repo, parent, path, active, &mut budget)
                        .ok()
                        .and_then(|b| String::from_utf8(b).ok())
                };
                (old, new)
            });
            if let Some(new) = &contents.1 {
                step.ranges
                    .extend(changed_matches(new, &step.after, &step.before, false));
            }
            if let Some(old) = &contents.0 {
                step.ranges
                    .extend(changed_matches(old, &step.before, &step.after, true));
            }
        }
    }
    result.notices.push("Only prompts, assistant text and allowlisted edit fields are displayed. Tool output, reasoning and command arguments are omitted; credential-like text is withheld. Exact content matches are evidence, not proof that a session authored a commit. Ambiguous or unavailable matches have no line jump".into());
    Ok(result)
}
