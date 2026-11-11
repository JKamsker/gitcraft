//! Explicit owned-local gates, excluded from the `subtree_pure_` selection.
use super::*;
use crate::git::drafts::virtuals::tests::Fixture;

fn candidate_view(repo: &Path) -> Result<(View, Candidate), String> {
    let view = read(repo, "")?;
    let candidate = view
        .state
        .candidates
        .last()
        .cloned()
        .ok_or("No candidate")?;
    Ok((read(repo, &candidate.id)?, candidate))
}

fn lifecycle(history: History) -> Result<(), String> {
    let source = Fixture::new()?;
    let original = Fixture::new()?;
    // The repositories are independent histories, even when initialized within one second.
    // Fixture::new otherwise gives both identical root tree/author/message/timestamps.
    source.git(&["commit", "--amend", "-qm", "independent upstream root"])?;
    assert_ne!(
        source.git(&["rev-parse", "HEAD"])?,
        original.git(&["rev-parse", "HEAD"])?
    );
    let repo = original.path();
    original.git(&[
        "remote",
        "add",
        "upstream",
        source.path().to_str().ok_or("Fixture path is not UTF-8")?,
    ])?;
    let control = git::OpControl::new(|| {});
    let runner = Runner {
        repo,
        control: &control,
        credentials: None,
    };
    let prefix = "vendor/a folder";
    let file = format!("{prefix}/file");
    let before = original.git(&["rev-parse", "HEAD"])?;
    let index = std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())?;
    register(
        &runner,
        &read(repo, "")?,
        prefix.into(),
        "upstream".into(),
        "main".into(),
        history,
    )?;
    let key = read(repo, "")?
        .state
        .sources
        .first()
        .ok_or("No registered source")?
        .id
        .clone();
    fetch(&runner, &read(repo, &key)?, &key)?;
    assert_eq!(original.git(&["rev-parse", "HEAD"])?, before);
    assert_eq!(
        std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    prepare(&runner, &read(repo, &key)?, &key, Kind::Update, false)?;
    let (view, added) = candidate_view(repo)?;
    assert_eq!(
        added.stage,
        Stage::Ready,
        "{}; {:?}",
        added.notice,
        control.take_steps()
    );
    assert!(!repo.join(prefix).exists());
    assert_eq!(original.git(&["rev-parse", "HEAD"])?, before);
    assert_eq!(
        std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())?,
        index
    );
    accept(&runner, &view, &added.id)?;
    let imported = added.result.as_deref().ok_or("No import result")?;
    assert_eq!(original.git(&["rev-parse", "HEAD"])?, imported);
    assert_eq!(
        std::fs::read(repo.join(&file)).map_err(|e| e.to_string())?,
        source.base().as_bytes()
    );
    assert_eq!(
        read(repo, &key)?.state.sources[0].imported.as_deref(),
        added.upstream.as_deref()
    );

    let ConflictOutcome {
        result,
        upstream_head,
        conflict,
    } = assert_conflict_resolution(&original, &source, &runner, &key, &file)?;

    // The split is an exact committed folder snapshot; original checkout stays intact.
    prepare(&runner, &read(repo, &key)?, &key, Kind::Split, false)?;
    let (view, split) = candidate_view(repo)?;
    assert_eq!(
        split.stage,
        Stage::SplitReady,
        "{}; {:?}",
        split.notice,
        control.take_steps()
    );
    let split_oid = split.result.as_deref().ok_or("No split result")?;
    assert_eq!(
        original.git(&["rev-parse", &format!("{result}:{prefix}")])?,
        original.git(&["rev-parse", &format!("{split_oid}^{{tree}}")])?
    );
    assert_eq!(original.git(&["rev-parse", "HEAD"])?, result);
    assert!(accept(&runner, &view, &split.id).is_err());
    assert_eq!(source.git(&["rev-parse", "main"])?, upstream_head);
    close(&runner, &view, &split.id)?;
    let closed = read(repo, &split.id)?;
    assert_eq!(
        closed.state.candidates.last().ok_or("Lost split")?.stage,
        Stage::Closed
    );
    assert!(split.worktree.is_dir());
    assert!(conflict.worktree.is_dir());
    assert_eq!(original.git(&["status", "--porcelain"])?, "");
    Ok(())
}

#[test]
fn subtree_owned_squash_import_conflict_accept_and_split() -> Result<(), String> {
    lifecycle(History::Squash)
}

#[test]
fn subtree_owned_full_history_import_conflict_accept_and_split() -> Result<(), String> {
    lifecycle(History::Full)
}

#[test]
fn subtree_signing_default_reads_effective_config_without_the_inspection_override()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    let repo = fixture.path();
    fixture.git(&["config", "commit.gpgSign", "yes"])?;
    assert!(read(repo, "")?.sign_commits);
    fixture.git(&["config", "commit.gpgSign", "false"])?;
    assert!(!read(repo, "")?.sign_commits);
    // Includes participate in the effective value too; no commit or signing key is needed.
    fixture.write(".git/subtree-signing", b"[commit]\n\tgpgSign = true\n")?;
    fixture.git(&["config", "--unset", "commit.gpgSign"])?;
    fixture.git(&["config", "include.path", "subtree-signing"])?;
    assert!(read(repo, "")?.sign_commits);
    Ok(())
}

struct ConflictOutcome {
    result: String,
    upstream_head: String,
    conflict: Candidate,
}

fn assert_conflict_resolution(
    original: &Fixture,
    source: &Fixture,
    runner: &Runner<'_>,
    key: &str,
    file: &str,
) -> Result<ConflictOutcome, String> {
    let repo = original.path();
    // Conflicting edits exercise the real helper, retained candidate and continuation.
    original.write(
        file,
        source.base().replace("line 3\n", "LOCAL\n").as_bytes(),
    )?;
    original.git(&["commit", "-qam", "local vendor edit"])?;
    source.write(
        "file",
        source.base().replace("line 3\n", "UPSTREAM\n").as_bytes(),
    )?;
    source.git(&["commit", "-qam", "upstream edit"])?;
    let upstream_head = source.git(&["rev-parse", "main"])?;
    fetch(runner, &read(repo, key)?, key)?;
    let old = original.git(&["rev-parse", "HEAD"])?;
    let old_index = std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())?;
    let old_file = std::fs::read(repo.join(file)).map_err(|e| e.to_string())?;
    prepare(runner, &read(repo, key)?, key, Kind::Update, false)?;
    let (view, conflict) = candidate_view(repo)?;
    assert_eq!(
        conflict.stage,
        Stage::Conflict,
        "{}; {:?}",
        conflict.notice,
        runner.control.take_steps()
    );
    assert_eq!(original.git(&["rev-parse", "HEAD"])?, old);
    assert_eq!(
        std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())?,
        old_index
    );
    assert_eq!(
        std::fs::read(repo.join(file)).map_err(|e| e.to_string())?,
        old_file
    );
    assert!(resume(runner, &view, &conflict.id).is_err());
    let resolved = source.base().replace("line 3\n", "LOCAL plus UPSTREAM\n");
    std::fs::write(conflict.worktree.join(file), &resolved).map_err(|e| e.to_string())?;
    git::run(Some(&conflict.worktree), &["add", "--", file])?;
    resume(runner, &read(repo, &conflict.id)?, &conflict.id)?;
    let ready = read(repo, &conflict.id)?;
    let resolved_candidate = ready
        .state
        .candidates
        .iter()
        .find(|c| c.id == conflict.id)
        .ok_or("Lost candidate")?;
    assert_eq!(
        resolved_candidate.stage,
        Stage::Ready,
        "{}",
        resolved_candidate.notice
    );
    let result = resolved_candidate
        .result
        .clone()
        .ok_or("No resolved result")?;
    original.write("other", b"unrelated dirty edit\n")?;
    assert!(accept(runner, &ready, &conflict.id).is_err());
    assert_eq!(original.git(&["rev-parse", "HEAD"])?, old);
    assert_eq!(
        std::fs::read(repo.join("other")).map_err(|e| e.to_string())?,
        b"unrelated dirty edit\n"
    );
    original.git(&["restore", "--worktree", "other"])?;
    accept(runner, &read(repo, &conflict.id)?, &conflict.id)?;
    assert_eq!(original.git(&["rev-parse", "HEAD"])?, result);
    assert_eq!(
        std::fs::read(repo.join(file)).map_err(|e| e.to_string())?,
        resolved.as_bytes()
    );

    Ok(ConflictOutcome {
        result,
        upstream_head,
        conflict,
    })
}
