//! Review state decoding, hunk identity, and aggregation.

use super::*;

#[derive(Clone)]
pub struct Hunk {
    pub patch: String,
    pub digest: String,
    pub reviewed: bool,
}

#[derive(Clone)]
pub struct Review {
    pub parent: String,
    pub turn: String,
    pub hunks: Vec<Hunk>,
    pub aggregate: Option<Aggregate>,
    pub(super) session: String,
    pub(super) boundary: Option<String>,
    pub(super) reference: String,
    pub(super) old: String,
    pub(super) known: BTreeSet<String>,
}

#[derive(Clone)]
pub struct Aggregate {
    pub session: String,
    pub last_looked: Option<String>,
    pub saved_turns: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct ReviewState {
    pub(super) schema: u32,
    pub(super) known: BTreeSet<String>,
    pub(super) boundary: Option<String>,
}

pub(super) fn decode_review(bytes: &[u8]) -> Result<ReviewState, String> {
    if let Ok(state) = serde_json::from_slice::<ReviewState>(bytes) {
        if state.schema != 1
            || state.boundary.as_ref().is_some_and(|oid| !oid_valid(oid))
            || state.known.len() > 10000
        {
            return Err(
                crate::i18n::text("backend-unsupported-review-boundary-state-b545e8").into(),
            );
        }
        return Ok(state);
    }
    let known: BTreeSet<String> = serde_json::from_slice(bytes)
        .map_err(|_| crate::i18n::text("backend-review-state-is-invalid-a15bd5"))?;
    if known.len() > 10000 {
        return Err(crate::i18n::text("backend-review-history-exceeds-10000-hunks-56e7b7").into());
    }
    Ok(ReviewState {
        schema: 1,
        known,
        boundary: None,
    })
}

pub(super) fn review_ref(worktree: &str, session: &str) -> String {
    format!(
        "refs/gitcraft/reviewed-turns/{worktree}/{}",
        digest(session.as_bytes())
    )
}

pub(super) fn load_review(
    ctx: &Context,
    session: &str,
    active: &dyn Fn() -> bool,
) -> Result<(String, String, ReviewState), String> {
    let reference = review_ref(&ctx.id, session);
    if !ctx
        .source(&["symbolic-ref", "-q", &reference], vec![], &[0, 1], active)?
        .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-review-state-must-be-a-direct-worktree-session-ref-ee7eea",
        )
        .into());
    }
    let old = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", &reference],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| crate::i18n::text("backend-invalid-review-ref-5ca2f2"))?
    .trim()
    .to_owned();
    let state = if old.is_empty() {
        ReviewState {
            schema: 1,
            known: BTreeSet::new(),
            boundary: None,
        }
    } else {
        let size = ctx
            .text(&["cat-file", "-s", &old], vec![], active)?
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-review-data-a42396"))?;
        if size > 1024 * 1024 {
            return Err(crate::i18n::text("backend-review-state-exceeds-1-mib-226894").into());
        }
        decode_review(&ctx.run(&["cat-file", "blob", &old], vec![], &[0], active)?)?
    };
    Ok((reference, old, state))
}

pub(super) fn session_turns(
    repo: &Path,
    active: &dyn Fn() -> bool,
) -> Result<Vec<crate::agent::checkpoint::Turn>, String> {
    let ctx = Context::new(repo, active)?;
    let top =
        String::from_utf8(ctx.source(&["rev-parse", "--show-toplevel"], vec![], &[0], active)?)
            .map_err(|_| crate::i18n::text("backend-worktree-must-be-utf-8-de53d2"))?;
    if !active() {
        return Err("Review cancelled".into());
    }
    let turns = crate::agent::checkpoint::turns(repo, top.trim_end_matches(['\r', '\n']))?;
    if turns.len() > 10000 {
        return Err(crate::i18n::text(
            "backend-review-turn-inventory-exceeds-10000-saved-turns-4de5d1",
        )
        .into());
    }
    Ok(turns)
}

// Endpoint is the immutable last saved turn selected when opening the dialog.
// Later arrivals are intentionally excluded rather than silently advancing it.
pub(super) fn aggregate_span(
    turns: &[crate::agent::checkpoint::Turn],
    session: &str,
    endpoint: &str,
    boundary: Option<&str>,
) -> Result<(String, usize), String> {
    let mut session_turns = turns
        .iter()
        .filter(|turn| turn.session == session)
        .collect::<Vec<_>>();
    session_turns.sort_by_key(|turn| turn.number);
    let end = session_turns
        .iter()
        .position(|turn| turn.commit == endpoint)
        .ok_or(crate::i18n::text(
            "backend-selected-saved-endpoint-is-no-longer-in-this-worktree-session-ref-4207e0",
        ))?;
    let selected = &session_turns[..=end];
    if selected
        .windows(2)
        .any(|pair| pair[1].number <= pair[0].number || pair[1].parent != pair[0].commit)
        || !oid_valid(endpoint)
        || !oid_valid(&selected[0].parent)
    {
        return Err(crate::i18n::text(
            "backend-saved-session-chain-changed-or-has-no-immutable-initial-parent-in-b15edc",
        )
        .into());
    }
    if let Some(boundary) = boundary {
        let begin = selected
            .iter()
            .position(|turn| turn.commit == boundary)
            .ok_or(crate::i18n::text(
                "backend-last-looked-boundary-is-outside-the-selected-saved-session-endpoi-e0e42e",
            ))?;
        Ok((boundary.into(), end - begin))
    } else {
        Ok((selected[0].parent.clone(), selected.len()))
    }
}

pub fn aggregate_review(
    repo: &Path,
    session: &str,
    endpoint: &str,
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    let ctx = Context::new(repo, active)?;
    let (_, _, state) = load_review(&ctx, session, active)?;
    let (parent, count) = aggregate_span(
        &session_turns(repo, active)?,
        session,
        endpoint,
        state.boundary.as_deref(),
    )?;
    let mut review = review(repo, &parent, endpoint, session, active)?;
    if review.boundary != state.boundary {
        return Err(crate::i18n::text(
            "backend-last-looked-boundary-changed-during-aggregate-preparation-reload-68990d",
        )
        .into());
    }
    review.aggregate = Some(Aggregate {
        session: session.into(),
        last_looked: state.boundary,
        saved_turns: count,
    });
    Ok(review)
}

impl Review {
    pub fn same_snapshot(&self, other: &Self) -> bool {
        self.reference == other.reference
            && self.old == other.old
            && self.parent == other.parent
            && self.turn == other.turn
    }
}

pub(super) fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(super) fn split(raw: &str, known: &BTreeSet<String>) -> Vec<Hunk> {
    let mut parts = Vec::<(String, String)>::new();
    let mut header = String::new();
    let mut body = String::new();
    let flush = |parts: &mut Vec<(String, String)>, header: &String, body: &mut String| {
        if !body.is_empty() {
            parts.push((header.clone(), std::mem::take(body)));
        }
    };
    for line in raw.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            flush(&mut parts, &header, &mut body);
            if !header.is_empty() && parts.last().is_none_or(|(h, _)| h != &header) {
                parts.push((header.clone(), String::new()));
            }
            header = line.into();
        } else if line.starts_with("@@ ") || line.starts_with("GIT binary patch") {
            flush(&mut parts, &header, &mut body);
            body.push_str(line);
        } else if body.is_empty() {
            header.push_str(line);
        } else {
            body.push_str(line);
        }
    }
    flush(&mut parts, &header, &mut body);
    if !header.is_empty() && parts.last().is_none_or(|(h, _)| h != &header) {
        parts.push((header, String::new()));
    }
    // Blob IDs vary when another hunk changes; paths, mode and other file metadata
    // still belong to the review identity. Unique text survives line-number shifts.
    let stable = parts
        .iter()
        .map(|(header, body)| {
            let metadata = header
                .lines()
                .filter_map(|line| {
                    if line.starts_with("index ") {
                        line.split_whitespace()
                            .nth(2)
                            .map(|mode| format!("index-mode {mode}"))
                    } else {
                        Some(line.to_owned())
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let text = body
                .lines()
                .filter(|line| !line.starts_with("@@ "))
                .collect::<Vec<_>>()
                .join("\n");
            format!("review-v2\n{metadata}\n{text}")
        })
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::new();
    for key in &stable {
        *counts.entry(key).or_insert(0usize) += 1;
    }
    parts
        .into_iter()
        .enumerate()
        .map(|(i, (header, body))| {
            let identity = if counts[&stable[i]] == 1 {
                stable[i].clone()
            } else {
                // Ambiguous identical occurrences bind location and ordinal. A moved,
                // inserted or removed duplicate is shown again rather than guessed.
                format!(
                    "{}\noccurrence {i}\n{}",
                    stable[i],
                    body.lines().next().unwrap_or_default()
                )
            };
            let key = digest(identity.as_bytes());
            Hunk {
                patch: format!("{header}{body}"),
                reviewed: known.contains(&key),
                digest: key,
            }
        })
        .collect()
}
