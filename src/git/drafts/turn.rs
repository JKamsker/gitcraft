//! Strict inverse-turn application. Later non-overlapping work is retained; an overlap
//! fails before writes. Raw private Git context preserves the real index and bypasses
//! checkout filters. A protected safety draft is retained for recovery/undo.
use super::*;
use std::sync::Arc;
#[derive(Clone)]
pub struct Inverse {
    pub turn: String,
    pub parent: String,
    pub patch: Arc<Vec<u8>>,
    pub paths: Vec<String>,
    source: Arc<Source>,
    target: Arc<working_transaction::Contents>,
    files: Arc<working_transaction::Contents>,
    policies: Arc<native_files::Policies>,
}
fn read_paths(
    ctx: &Context,
    paths: &[String],
    s: &Source,
    active: &dyn Fn() -> bool,
) -> Result<working_transaction::Contents, String> {
    paths.iter().map(|p|{
        let mode=s.index.get(p).map_or("100644",|e|e.mode.as_str());
        if !matches!(mode,"100644"|"100755"){return Err("Turn reversal supports regular files; links and submodules need manual resolution".into());}
        Ok((p.clone(),read_file(ctx,p,mode,active)?))
    }).collect()
}
pub fn inverse_preview(
    repo: &Path,
    parent: &str,
    turn: &str,
    active: &dyn Fn() -> bool,
) -> Result<Inverse, String> {
    if !oid_valid(parent) || !oid_valid(turn) {
        return Err("Select an immutable saved turn with a parent".into());
    }
    let ctx = Context::new(repo, active)?;
    let paths = ctx.run(
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--name-only",
            "-z",
            turn,
            parent,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    let paths = untracked_paths(paths)?.into_iter().collect::<Vec<_>>();
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err("Turn is empty or exceeds the 2000-file reversal limit".into());
    }
    let now = source(&ctx, active)?;
    let files = read_paths(&ctx, &paths, &now, active)?;
    if files
        .values()
        .flatten()
        .map(|(_, b)| b.len())
        .sum::<usize>()
        > TOTAL_LIMIT
    {
        return Err("Turn reversal exceeds 32 MiB".into());
    }
    // Mode changes, links and gitlinks are checked from both immutable trees, even if the
    // current index no longer contains the path.
    for rev in [parent, turn] {
        let data = ctx.run(&["ls-tree", "-r", "-z", rev], vec![], &[0], active)?;
        for row in data.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let row = std::str::from_utf8(row).map_err(|_| "Turn paths must be UTF-8")?;
            let (info, path) = row.split_once('\t').ok_or("Invalid tree")?;
            if paths.iter().any(|p| p == path)
                && !info.starts_with("100644 blob ")
                && !info.starts_with("100755 blob ")
            {
                return Err("Turn contains links or gitlinks; resolve those manually".into());
            }
        }
    }
    let patch = ctx.run(
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            turn,
            parent,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    let mut entries = now.index.clone();
    for (path, data) in &files {
        match data {
            Some((mode, bytes)) => {
                entries.insert(
                    path.clone(),
                    Entry {
                        mode: mode.clone(),
                        oid: ctx.blob(bytes.clone(), active)?,
                    },
                );
            }
            None => {
                entries.remove(path);
            }
        }
    }
    tree(&ctx, &entries, active)?;
    super::restore::raw_attributes(&ctx, active)?;
    ctx.run(&["-c","core.autocrlf=false","apply","--cached","--binary","--whitespace=nowarn","-"],patch.clone(),&[0],active).map_err(|_|"Later changes overlap this turn. Nothing was changed; inspect the turn and resolve the overlap explicitly".to_owned())?;
    let mut target = BTreeMap::new();
    for path in &paths {
        let rows = ctx.run(
            &[
                "--literal-pathspecs",
                "ls-files",
                "--stage",
                "-z",
                "--",
                path,
            ],
            vec![],
            &[0],
            active,
        )?;
        let row = std::str::from_utf8(&rows).map_err(|_| "Invalid result path")?;
        let data = if row.is_empty() {
            None
        } else {
            let info = row
                .split_once('\t')
                .ok_or("Invalid result index")?
                .0
                .split_whitespace()
                .collect::<Vec<_>>();
            if info.len() != 3 || !matches!(info[0], "100644" | "100755") {
                return Err("Unsupported result mode".into());
            }
            let size = ctx
                .text(&["cat-file", "-s", info[1]], vec![], active)?
                .parse::<usize>()
                .map_err(|_| "Invalid blob size")?;
            if size > FILE_LIMIT {
                return Err("Result exceeds 8 MiB".into());
            }
            Some((
                info[0].into(),
                ctx.run(&["cat-file", "blob", info[1]], vec![], &[0], active)?,
            ))
        };
        target.insert(path.clone(), data);
    }
    if source(&ctx, active)? != now || read_paths(&ctx, &paths, &now, active)? != files {
        return Err("Checkout changed during preview; reopen it".into());
    }
    Ok(Inverse {
        turn: turn.into(),
        parent: parent.into(),
        patch: Arc::new(patch),
        paths,
        source: Arc::new(now),
        policies: Arc::new(native_files::capture_policies(repo, &files, active)?),
        files: Arc::new(files),
        target: Arc::new(target),
    })
}
pub struct Applied {
    pub draft: Id,
    pub evidence: String,
}
pub fn inverse_apply(
    repo: &Path,
    p: &Inverse,
    active: &dyn Fn() -> bool,
) -> Result<Applied, String> {
    let ctx = Context::new(repo, active)?;
    let verify = || -> Result<(), String> {
        if source(&ctx, active)? != *p.source
            || read_paths(&ctx, &p.paths, &p.source, active)? != *p.files
            || native_files::capture_policies(repo, &p.files, active)? != *p.policies
        {
            return Err(
                "HEAD, index or working files changed since the turn preview; reopen it".into(),
            );
        }
        Ok(())
    };
    verify()?;
    let backup = capture_preserving_paths(
        repo,
        "Before reverting one agent turn",
        None,
        &p.paths,
        active,
    )?
    .ok_or("Could not protect current work")?;
    verify()?;
    let check = || -> Result<(), String> {
        if source(&ctx, active)? != *p.source {
            return Err("HEAD or index changed while reverting".into());
        }
        Ok(())
    };
    let evidence =
        working_transaction::apply(repo, &p.files, &p.target, &p.policies, &check, active)?;
    Ok(Applied {
        draft: backup.id,
        evidence,
    })
}

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
    session: String,
    boundary: Option<String>,
    reference: String,
    old: String,
    known: BTreeSet<String>,
}
#[derive(Clone)]
pub struct Aggregate {
    pub session: String,
    pub last_looked: Option<String>,
    pub saved_turns: usize,
}
#[derive(Clone, Serialize, Deserialize)]
struct ReviewState {
    schema: u32,
    known: BTreeSet<String>,
    boundary: Option<String>,
}
fn decode_review(bytes: &[u8]) -> Result<ReviewState, String> {
    if let Ok(state) = serde_json::from_slice::<ReviewState>(bytes) {
        if state.schema != 1
            || state.boundary.as_ref().is_some_and(|oid| !oid_valid(oid))
            || state.known.len() > 10000
        {
            return Err("Unsupported review boundary state".into());
        }
        return Ok(state);
    }
    let known: BTreeSet<String> =
        serde_json::from_slice(bytes).map_err(|_| "Review state is invalid")?;
    if known.len() > 10000 {
        return Err("Review history exceeds 10000 hunks".into());
    }
    Ok(ReviewState {
        schema: 1,
        known,
        boundary: None,
    })
}
fn review_ref(worktree: &str, session: &str) -> String {
    format!(
        "refs/gitcraft/reviewed-turns/{worktree}/{}",
        digest(session.as_bytes())
    )
}
fn load_review(
    ctx: &Context,
    session: &str,
    active: &dyn Fn() -> bool,
) -> Result<(String, String, ReviewState), String> {
    let reference = review_ref(&ctx.id, session);
    if !ctx
        .source(&["symbolic-ref", "-q", &reference], vec![], &[0, 1], active)?
        .is_empty()
    {
        return Err("Review state must be a direct worktree/session ref".into());
    }
    let old = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", &reference],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| "Invalid review ref")?
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
            .map_err(|_| "Invalid review data")?;
        if size > 1024 * 1024 {
            return Err("Review state exceeds 1 MiB".into());
        }
        decode_review(&ctx.run(&["cat-file", "blob", &old], vec![], &[0], active)?)?
    };
    Ok((reference, old, state))
}
fn session_turns(
    repo: &Path,
    active: &dyn Fn() -> bool,
) -> Result<Vec<crate::agent::checkpoint::Turn>, String> {
    let ctx = Context::new(repo, active)?;
    let top =
        String::from_utf8(ctx.source(&["rev-parse", "--show-toplevel"], vec![], &[0], active)?)
            .map_err(|_| "Worktree must be UTF-8")?;
    if !active() {
        return Err("Review cancelled".into());
    }
    let turns = crate::agent::checkpoint::turns(repo, top.trim_end_matches(['\r', '\n']))?;
    if turns.len() > 10000 {
        return Err("Review turn inventory exceeds 10000 saved turns".into());
    }
    Ok(turns)
}
// Endpoint is the immutable last saved turn selected when opening the dialog.
// Later arrivals are intentionally excluded rather than silently advancing it.
fn aggregate_span(
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
        .ok_or("Selected saved endpoint is no longer in this worktree/session; refresh")?;
    let selected = &session_turns[..=end];
    if selected
        .windows(2)
        .any(|pair| pair[1].number <= pair[0].number || pair[1].parent != pair[0].commit)
        || !oid_valid(endpoint)
        || !oid_valid(&selected[0].parent)
    {
        return Err("Saved session chain changed or has no immutable initial parent; inspect individual turns".into());
    }
    if let Some(boundary) = boundary {
        let begin=selected.iter().position(|turn|turn.commit==boundary).ok_or("Last-looked boundary is outside the selected saved session endpoint; refresh the session before advancing")?;
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
        return Err("Last-looked boundary changed during aggregate preparation; reload".into());
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
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn split(raw: &str, known: &BTreeSet<String>) -> Vec<Hunk> {
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

pub fn review(
    repo: &Path,
    parent: &str,
    turn: &str,
    session: &str,
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    if !oid_valid(parent) || !oid_valid(turn) {
        return Err("Turn has no immutable parent".into());
    }
    let ctx = Context::new(repo, active)?;
    let (reference, old, state) = load_review(&ctx, session, active)?;
    let known = state.known;
    let stat = ctx.run(
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--numstat",
            parent,
            turn,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    if stat.len() > 128 * 1024 {
        return Err("Turn has too many files for review".into());
    }
    let raw = ctx.run(
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            parent,
            turn,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    if raw.len() > TOTAL_LIMIT {
        return Err("Turn diff exceeds 32 MiB; inspect files separately".into());
    }
    let raw = String::from_utf8(raw).map_err(|_| "Turn diff cannot be displayed as UTF-8")?;
    let hunks = split(&raw, &known);
    Ok(Review {
        parent: parent.into(),
        turn: turn.into(),
        hunks,
        aggregate: None,
        session: session.into(),
        boundary: state.boundary,
        reference,
        old,
        known,
    })
}
pub fn mark_reviewed(
    repo: &Path,
    review: &Review,
    selected: &[usize],
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    let ctx = Context::new(repo, active)?;
    let mut known = review.known.clone();
    for &i in selected {
        known.insert(
            review
                .hunks
                .get(i)
                .ok_or("Invalid reviewed hunk")?
                .digest
                .clone(),
        );
    }
    if known.len() > 10000 {
        return Err("Review history reached 10000 hunks; choose another session".into());
    }
    save_review(
        &ctx,
        review,
        ReviewState {
            schema: 1,
            known,
            boundary: review.boundary.clone(),
        },
        active,
    )
}
fn save_review(
    ctx: &Context,
    review: &Review,
    state: ReviewState,
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    if review.reference != review_ref(&ctx.id, &review.session) {
        return Err("Review belongs to a different worktree/session".into());
    }
    let bytes = serde_json::to_vec(&state).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Review state exceeds 1 MiB".into());
    }
    let oid = ctx.blob(bytes, active)?;
    let zero = "0".repeat(oid.len());
    ctx.source(
        &[
            "update-ref",
            "--no-deref",
            &review.reference,
            &oid,
            if review.old.is_empty() {
                &zero
            } else {
                &review.old
            },
        ],
        vec![],
        &[0],
        active,
    )
    .map_err(|_| {
        "Review marks or boundary changed concurrently; reload the saved review".to_owned()
    })?;
    let mut updated = review.clone();
    updated.old = oid;
    updated.known = state.known;
    updated.boundary = state.boundary;
    for hunk in &mut updated.hunks {
        hunk.reviewed = updated.known.contains(&hunk.digest);
    }
    if let Some(aggregate) = &mut updated.aggregate {
        aggregate.last_looked = updated.boundary.clone();
    }
    Ok(updated)
}
pub fn finish_aggregate(
    repo: &Path,
    review: &Review,
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    let state = finish_state(review, &session_turns(repo, active)?)?;
    let ctx = Context::new(repo, active)?;
    save_review(&ctx, review, state, active)
}
fn finish_state(
    review: &Review,
    turns: &[crate::agent::checkpoint::Turn],
) -> Result<ReviewState, String> {
    let aggregate = review
        .aggregate
        .as_ref()
        .ok_or("Open an aggregate saved-turn review first")?;
    if aggregate.session != review.session || aggregate.last_looked != review.boundary {
        return Err("Review boundary context changed".into());
    }
    let (base, _) = aggregate_span(
        turns,
        &review.session,
        &review.turn,
        review.boundary.as_deref(),
    )?;
    if base != review.parent {
        return Err("Aggregate baseline changed; reopen before advancing its boundary".into());
    }
    let mut known = review.known.clone();
    known.extend(review.hunks.iter().map(|hunk| hunk.digest.clone()));
    if known.len() > 10000 {
        return Err("Review history reached 10000 hunks".into());
    }
    Ok(ReviewState {
        schema: 1,
        known,
        boundary: Some(review.turn.clone()),
    })
}

#[cfg(test)]
pub(crate) fn recorded_aggregate_fixture() -> Review {
    Review {
        parent: "b".repeat(40),
        turn: "c".repeat(40),
        hunks: vec![Hunk {
            patch: "new aggregate hunk".into(),
            digest: "hunk".into(),
            reviewed: false,
        }],
        aggregate: Some(Aggregate {
            session: "recorded-session".into(),
            last_looked: Some("b".repeat(40)),
            saved_turns: 1,
        }),
        session: "recorded-session".into(),
        boundary: Some("b".repeat(40)),
        reference: review_ref("recorded-worktree", "recorded-session"),
        old: "d".repeat(40),
        known: BTreeSet::new(),
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    fn patch(mode: &str, hunks: &str) -> String {
        format!("diff --git a/f b/f\nindex aaaa..bbbb {mode}\n--- a/f\n+++ b/f\n{hunks}")
    }
    #[test]
    fn pure_turn_review_identity_distinguishes_duplicate_locations_and_file_modes() {
        let text = " same\n-old\n+new\n same\n";
        let raw = patch(
            "100644",
            &format!("@@ -1,3 +1,3 @@\n{text}@@ -21,3 +21,3 @@\n{text}"),
        );
        let hunks = split(&raw, &BTreeSet::new());
        assert_eq!(hunks.len(), 2);
        assert_ne!(hunks[0].digest, hunks[1].digest);
        let known = [hunks[0].digest.clone()].into();
        let marked = split(&raw, &known);
        assert!(marked[0].reviewed);
        assert!(!marked[1].reviewed);
        let unique = patch("100644", &format!("@@ -1,3 +1,3 @@\n{text}"));
        assert!(!split(&unique, &known)[0].reviewed);
        let unique_mark = [split(&unique, &BTreeSet::new())[0].digest.clone()].into();
        let shifted = patch("100644", &format!("@@ -11,3 +11,3 @@\n{text}"));
        assert!(split(&shifted, &unique_mark)[0].reviewed);
        assert!(
            !split(
                &patch("100755", &format!("@@ -1,3 +1,3 @@\n{text}")),
                &unique_mark
            )[0]
            .reviewed
        );
        assert!(split(&raw, &unique_mark).iter().all(|h| !h.reviewed));
        let mode_change = format!(
            "diff --git a/f b/f\nold mode 100644\nnew mode 100755\nindex aaaa..bbbb\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n{text}"
        );
        assert!(!split(&mode_change, &unique_mark)[0].reviewed);
        let old_key = digest(format!("diff --git a/f b/f\n{}", text.trim_end()).as_bytes());
        assert!(!split(&unique, &[old_key].into())[0].reviewed);
    }
}

#[cfg(test)]
mod aggregate_tests {
    use super::*;
    fn turns() -> Vec<crate::agent::checkpoint::Turn> {
        (1..=3)
            .map(|n| crate::agent::checkpoint::Turn {
                session: "recorded-session".into(),
                number: n,
                commit: char::from(b'a' + n as u8).to_string().repeat(40),
                parent: char::from(b'a' + n as u8 - 1).to_string().repeat(40),
                ..Default::default()
            })
            .collect()
    }
    #[test]
    fn pure_aggregate_review_fixed_endpoint_excludes_new_turns_and_refuses_changed_baselines()
    -> Result<(), String> {
        let mut review = recorded_aggregate_fixture();
        let turns = turns();
        assert_eq!(
            aggregate_span(
                &turns,
                "recorded-session",
                &review.turn,
                review.boundary.as_deref()
            )
            .map_err(|error| error.to_string())?,
            ("b".repeat(40), 1)
        );
        let mut advanced = review.clone();
        advanced.old = "e".repeat(40);
        assert!(!review.same_snapshot(&advanced));
        advanced = review.clone();
        advanced.reference = review_ref("other-worktree", "recorded-session");
        assert!(!review.same_snapshot(&advanced));
        assert!(review.same_snapshot(&review.clone()));
        let state = finish_state(&review, &turns).map_err(|error| error.to_string())?;
        assert_eq!(state.boundary.as_deref(), Some(review.turn.as_str()));
        assert_ne!(state.boundary.as_deref(), Some(turns[2].commit.as_str()));
        assert!(state.known.contains("hunk"));
        review.parent = "a".repeat(40);
        assert!(finish_state(&review, &turns).is_err());
        review = recorded_aggregate_fixture();
        review
            .aggregate
            .as_mut()
            .ok_or("expected Some")?
            .last_looked = Some("c".repeat(40));
        assert!(finish_state(&review, &turns).is_err());
        let mut changed = turns.clone();
        changed[1].parent = "f".repeat(40);
        assert!(finish_state(&recorded_aggregate_fixture(), &changed).is_err());
        assert!(aggregate_span(&turns, "other-session", &review.turn, None).is_err());
        assert_ne!(
            review_ref("worktree-a", "session"),
            review_ref("worktree-b", "session")
        );
        assert_ne!(
            review_ref("worktree-a", "session"),
            review_ref("worktree-a", "other-session")
        );
        Ok(())
    }
    #[test]
    fn pure_aggregate_review_migrates_legacy_marks_without_inventing_a_review_boundary()
    -> Result<(), String> {
        let old: BTreeSet<String> = ["old-hunk-key".into()].into();
        let state = decode_review(&serde_json::to_vec(&old).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        assert_eq!(state.known, old);
        assert!(state.boundary.is_none());
        let current = finish_state(&recorded_aggregate_fixture(), &turns())
            .map_err(|error| error.to_string())?;
        let encoded = serde_json::to_vec(&current).map_err(|error| error.to_string())?;
        assert_eq!(
            decode_review(&encoded)
                .map_err(|error| error.to_string())?
                .boundary,
            current.boundary
        );
        assert!(decode_review(br#"{"schema":2,"known":[],"boundary":null}"#).is_err());
        Ok(())
    }
}
