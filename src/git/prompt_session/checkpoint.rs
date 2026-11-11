//! Checkpoint.

use super::*;

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
    (integration && !skip_hooks).then_some(crate::i18n::text(
        "backend-entire-integration-is-present-if-automatic-checkpoint-synchroniza-57f84c",
    ))
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
            .map_err(|_| crate::i18n::text("backend-invalid-pre-push-hook-path-2d01b5"))?
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

pub(super) fn occurrences(content: &str, needle: &str, old: bool) -> Vec<Span> {
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

pub(super) fn changed_matches(content: &str, needle: &str, other: &str, old: bool) -> Vec<Span> {
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
