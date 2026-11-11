//! Pull.

use super::*;

/// After the upstream was rewritten (a commit the branch started on dropped and replaced),
/// a pull replays only the branch's own commits, from the upstream's fork point; a plain
/// rebase would replay the dropped commit too, and conflict on it.
#[test]
pub(super) fn a_pull_rebase_starts_from_the_fork_point() -> Result<(), String> {
    let repo = forked("fork-point")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.git(&["config", "core.logAllRefUpdates", "true"])?;
    repo.write("f.txt", b"a\nold\nc\n")?;
    repo.commit("upstream, before the rewrite")?;
    repo.git(&["checkout", "-q", "-b", "topic"])?;
    repo.write("own.txt", b"mine\n")?;
    let head = repo.commit("own work")?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.git(&["reset", "-q", "--hard", "HEAD~1"])?;
    repo.write("f.txt", b"a\nnew\nc\n")?;
    let upstream = repo.commit("upstream, rewritten")?;
    let plain = rebase(repo.path(), features, &head, &upstream)?;
    assert_eq!(conflicts(&plain).0, ["f.txt"]);
    let pulled = pull_rebase(repo.path(), features, &head, "main", &upstream)?;
    assert_eq!(pulled.commits, 1);
    assert_eq!(pulled.outcome, Outcome::Clean { changes: true });
    Ok(())
}

#[test]
pub(super) fn a_pull_rebase_skips_work_upstream_already_took() -> Result<(), String> {
    let repo = forked("fork-point-picked")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.git(&["checkout", "-q", "-b", "topic"])?;
    repo.write("f.txt", b"a\nX\nc\n")?;
    let taken = repo.commit("own work, taken upstream")?;
    repo.write("own.txt", b"mine\n")?;
    let head = repo.commit("own work")?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.write("other.txt", b"theirs\n")?;
    repo.commit("upstream work")?;
    repo.git(&["cherry-pick", &taken])?;
    repo.write("f.txt", b"a\nY\nc\n")?;
    let upstream = repo.commit("upstream builds on it")?;
    let pulled = pull_rebase(repo.path(), features, &head, "main", &upstream)?;
    assert_eq!(pulled.commits, 1);
    assert_eq!(pulled.outcome, Outcome::Clean { changes: true });
    Ok(())
}
