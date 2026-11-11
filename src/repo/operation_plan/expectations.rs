use super::*;

/// Preparation may take minutes and observes its own coherent snapshot. It never
/// authorizes a changed operation context: retain the original verifier and run
/// it again after preparation, immediately before command dispatch.
pub(in crate::repo) fn with_rechecked_preparation(
    mut verify: impl FnMut() -> Result<(), String>,
    prepare: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    verify()?;
    prepare()?;
    verify()
}

/// Marks a step that runs no git (no argument can hold a NUL).
pub(in crate::repo) const EXPECT: &str = "\u{0}expect";

/// A step of an undo (or redo) that runs nothing, but refuses the whole of it unless the ref
/// `name` is still where the operation it undoes left it (it learns where when that runs): so
/// that undoing a stash pops that stash and not one pushed since, and undoing a pull does not
/// put back a branch that got commits since.
pub(in crate::repo) fn expect_ref(name: &str) -> Vec<String> {
    vec![EXPECT.to_owned(), name.to_owned(), String::new()]
}

/// A step that runs nothing, but refuses the whole operation unless `name` is at `commit`.
pub(crate) fn expect_ref_at(name: &str, commit: &str) -> Vec<String> {
    vec![EXPECT.to_owned(), name.to_owned(), commit.to_owned()]
}

/// Marks a step that runs no git and expects a symbolic ref.
pub(in crate::repo) const EXPECT_SYMREF: &str = "\u{0}expect-symref";

/// A step that runs nothing, but refuses the whole operation unless the symbolic ref `name`
/// (`HEAD`) points to the ref `target` (`refs/heads/main`), or is detached when `target` is
/// empty: two branches at the same commit can't be told apart by what they resolve to.
pub(in crate::repo) fn expect_symref(name: &str, target: &str) -> Vec<String> {
    vec![EXPECT_SYMREF.to_owned(), name.to_owned(), target.to_owned()]
}

/// Marks a step that runs no git and expects a revision to have a tree.
pub(in crate::repo) const EXPECT_TREE: &str = "\u{0}expect-tree";

/// A step that runs nothing, but refuses the whole operation unless the revision `rev`
/// (`stash@{0}^2`) has the tree `tree`: unlike [`expect_ref`], what it expects stays as it was
/// made, however often the operation is redone (a patch made from that tree is applied next).
pub(in crate::repo) fn expect_tree(rev: &str, tree: &str) -> Vec<String> {
    vec![EXPECT_TREE.to_owned(), rev.to_owned(), tree.to_owned()]
}

/// The revision and the tree of an `expect_tree` step.
pub(in crate::repo) fn expected_tree(op: &[String]) -> Option<(&str, &str)> {
    match op {
        [mark, rev, tree] if mark == EXPECT_TREE => Some((rev, tree)),
        _ => None,
    }
}

/// [`expect_symref`] in another worktree of the repository: `dir` is its folder as git on the
/// repository's machine knows it.
pub(in crate::repo) fn expect_symref_in(dir: &str, name: &str, target: &str) -> Vec<String> {
    let mut step = expect_symref(name, target);
    step.push(dir.to_owned());
    step
}

/// The ref and the commit an `expect_ref` step expects.
pub(in crate::repo) fn expected_ref(op: &[String]) -> Option<(&str, &str)> {
    match op {
        [mark, name, value] if mark == EXPECT => Some((name, value)),
        _ => None,
    }
}

/// The symbolic ref, the ref it should point to and the worktree (`None`: this one) of an
/// `expect_symref` step.
pub(in crate::repo) fn expected_symref(op: &[String]) -> Option<(&str, &str, Option<&str>)> {
    match op {
        [mark, name, target] if mark == EXPECT_SYMREF => Some((name, target, None)),
        [mark, name, target, dir] if mark == EXPECT_SYMREF => Some((name, target, Some(dir))),
        _ => None,
    }
}

/// Marks a step whose failure doesn't stop the operation.
pub(in crate::repo) const TRY: &str = "\u{0}try";
/// Starts what an operation says when it went through but a step tried failed.
pub(in crate::repo) const TRIED: &str = "\u{0}tried\u{0}";

/// A step that runs `args` unattended (nobody is asked to sign in) and doesn't stop the
/// operation when it fails: the status says `says` then. A fetch before starting from what it
/// fetches, when what was fetched before will do (offline).
pub(in crate::repo) fn try_step(args: &[&str], says: &str) -> Vec<String> {
    [TRY, says]
        .into_iter()
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect()
}

/// What the steps tried and failed say, from what an operation that went through gave.
pub(in crate::repo) fn tried_note(out: &str) -> Option<&str> {
    out.strip_prefix(TRIED)
}

/// Whether `op` is a step that runs nothing, only checks.
pub(in crate::repo) fn is_expectation(op: &[String]) -> bool {
    crate::repo::forks::guard::is_guard(op)
        || expected_ref(op).is_some()
        || expected_symref(op).is_some()
        || expected_tree(op).is_some()
        || matches!(op, [mark, _, _] if mark == crate::gerrit::CHECK)
        || matches!(op, [mark, _] if mark == EXPECT_HTTPS_ROUTE)
        || matches!(op, [mark, _] if mark == EXPECT_FOLD_SAFE)
        || matches!(op, [mark, _] if mark == EXPECT_FOLD_IGNORED)
}

pub(in crate::repo) const EXPECT_FOLD_SAFE: &str = "\u{0}expect-fold-safe";
pub(in crate::repo) const EXPECT_FOLD_IGNORED: &str = "\u{0}expect-fold-ignored";
