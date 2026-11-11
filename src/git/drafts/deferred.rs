//! Replays in persistent detached worktrees. Until explicit guarded acceptance, no
//! original branch, index or checkout is rewritten. Cancellation leaves inspectable work.
use super::*;
use crate::git::repository_tools::guard::Runner;
use std::path::PathBuf;
const REF: &str = "refs/gitcraft/deferred-rewrites/state";
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Jobs {
    pub jobs: Vec<Job>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub steps: Vec<Step>,
    pub cursor: usize,
    pub busy: bool,
    pub cancelled: bool,
    pub notice: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Step {
    pub branch: String,
    pub old: String,
    pub old_base: String,
    pub parent: String,
    pub onto: String,
    pub worktree: PathBuf,
    pub made: bool,
    #[serde(default)]
    pub root_identity: Option<String>,
    pub result: Option<String>,
    pub accepted: bool,
    pub conflicts: Vec<String>,
}
#[derive(Clone)]
pub struct View {
    pub jobs: Jobs,
    pub version: String,
    pub branch: String,
    pub branches: Vec<(String, String)>,
    common: PathBuf,
}
#[derive(Clone)]
pub struct Spec {
    pub expected: String,
    pub branch: String,
    pub old_base: String,
    pub onto: String,
    pub parent: String,
}

// Separate argv carries one validated metadata observation. The child captures
// directory descriptors and verifies them against retained native handles before
// dispatch. Neither .git nor commondir is reparsed to choose dispatch operands.
// Darwin Git cannot traverse directory-fd paths: use captured physical paths after
// comparing them to the held identities immediately before Git. The locked,
// retain-only lifecycle supports ordinary worktree concurrency, not forced same-user
// administration replacement after the final check. Post-check drift is uncertain;
// retained handles do not make Darwin path dispatch a zero-foreign-write capability.
// BSD stat with no operand uses fstat(STDIN_FILENO); stat("/dev/fd/N")
// observes the Darwin descriptor-filesystem entry instead of the held inode.
// Keep platform selection compile-time and pass the already-open handle on stdin.
macro_rules! owned_alias {
    ($fd_stat:literal, $dispatch:literal) => {
        concat!(r#"alias.gitcraft-owned-replay=!f() { pin=$1; path=$2; metadata=$3; mpin=$4; common=$5; cpin=$6; lpin=$7; link=$8; shift 8; token() { LC_ALL=C stat -L -c '%d:%i' "$1" 2>/dev/null || LC_ALL=C stat -L -f '%d:%i' "$1"; }; token_fd() { "#, $fd_stat, r#"; }; cd -P -- "$path" || exit 1; [ "$(token .)" = "$pin" ] || { printf "%s\n" "Owned replay root identity/content mismatch" >&2; exit 1; }; exec 6<.git || exit 1; [ "$(token_fd <&6)" = "$lpin" ] || { printf "%s\n" "Owned replay git-link identity/content mismatch" >&2; exit 1; }; [ "$(cat <&6)" = "$link" ] || { printf "%s\n" "Owned replay git-link-content identity/content mismatch" >&2; exit 1; }; exec 7<"$metadata" || exit 1; [ "$(token_fd <&7)" = "$mpin" ] || { printf "%s\n" "Owned replay metadata identity/content mismatch" >&2; exit 1; }; exec 8<"$common" || exit 1; [ "$(token_fd <&8)" = "$cpin" ] || { printf "%s\n" "Owned replay common identity/content mismatch" >&2; exit 1; }; [ "$(token "$metadata")" = "$(token_fd <&7)" ] || { printf "%s\n" "Owned replay metadata path changed before Git" >&2; exit 1; }; [ "$(token "$common")" = "$(token_fd <&8)" ] || { printf "%s\n" "Owned replay common path changed before Git" >&2; exit 1; }; "#, $dispatch, r#"; }; f"#)
    };
}
#[cfg(target_os = "macos")]
const OWNED_ALIAS: &str = owned_alias!(
    "LC_ALL=C stat -L -f '%d:%i'",
    r#"GIT_COMMON_DIR="$common" git --no-lazy-fetch --git-dir="$metadata" --work-tree=. "$@""#
);
#[cfg(not(target_os = "macos"))]
const OWNED_ALIAS: &str = owned_alias!(
    "LC_ALL=C stat -L -c '%d:%i' /dev/stdin",
    r#"GIT_COMMON_DIR=/dev/fd/8 git --no-lazy-fetch --git-dir=/dev/fd/7 --work-tree=. "$@""#
);

pub fn start(
    r: &Runner<'_>,
    expected: &View,
    title: String,
    specs: Vec<Spec>,
) -> Result<String, String> {
    let active = || !r.control.is_cancelled();
    check(r.repo, expected, &active)?;
    if specs.is_empty() || specs.len() > 32 {
        return Err(crate::i18n::text("backend-select-1-32-replay-layers-993fec").into());
    }
    let ctx = Context::new(r.repo, &active)?;
    static IDS: AtomicU64 = AtomicU64::new(0);
    let id = format!(
        "{}-{}-{}",
        git::now(),
        std::process::id(),
        IDS.fetch_add(1, Ordering::Relaxed)
    );
    let mut branches = BTreeSet::new();
    let mut steps = Vec::new();
    for spec in specs {
        let reference = format!("refs/heads/{}", spec.branch);
        ctx.source(&["check-ref-format", &reference], vec![], &[0], &active)?;
        if !branches.insert(spec.branch.clone()) {
            return Err(
                crate::i18n::text("backend-a-branch-appears-twice-in-the-replay-30fbdd").into(),
            );
        }
        let old = resolve(&ctx, &reference, &active)?;
        if old != spec.expected
            || !ctx
                .source(
                    &["symbolic-ref", "-q", &reference],
                    vec![],
                    &[0, 1],
                    &active,
                )?
                .is_empty()
        {
            return Err(crate::i18n::text(
                "backend-source-branch-changed-or-became-indirect-since-preview-refresh-be-95f83e",
            )
            .into());
        }

        let old_base = resolve(&ctx, &spec.old_base, &active)?;
        let onto = resolve(&ctx, &spec.onto, &active)?;
        ctx.source(
            &["merge-base", "--is-ancestor", &old_base, &old],
            vec![],
            &[0],
            &active,
        )
        .map_err(|_| {
            crate::i18n::text(
                "backend-the-old-base-must-be-an-ancestor-of-the-original-branch-e1c11e",
            )
        })?;
        let worktree = expected
            .common
            .join("gitcraft/deferred-rewrites")
            .join(&id)
            .join(format!("step-{}", steps.len()));
        steps.push(Step {
            branch: spec.branch,
            old,
            old_base,
            parent: spec.parent,
            onto,
            worktree,
            made: false,
            root_identity: None,
            result: None,
            accepted: false,
            conflicts: Vec::new(),
        });
    }
    let mut view = expected.clone();
    view.jobs.jobs.push(Job {
        id: id.clone(),
        title,
        steps,
        cursor: 0,
        busy: false,
        cancelled: false,
        notice: crate::i18n::text(
            "backend-prepared-immutable-replay-inputs-original-branches-retained-3bc283",
        )
        .into(),
    });
    save(r.repo, &mut view, &active)?;
    resume(r, &view, &id)
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
#[path = "deferred/runtime_tests.rs"]
mod runtime_tests;

#[cfg(test)]
mod backend_fluent_notice_tests;

mod settlement;
#[cfg(test)]
use settlement::Replay;
pub use settlement::accept;
#[cfg(test)]
use settlement::acceptance_allowed;
#[cfg(test)]
use settlement::accepted_tip;
pub use settlement::close;
#[cfg(test)]
use settlement::guarded_in_flight;
pub use settlement::reconcile;
#[cfg(test)]
use settlement::replay;
pub use settlement::resume;

mod ownership;
#[cfg(test)]
use ownership::DispatchBinding;
use ownership::owned;
#[cfg(test)]
use ownership::owned_command;
#[cfg(test)]
use ownership::owned_command_at;
use ownership::owned_run;
#[cfg(any(unix, test))]
#[cfg(test)]
use ownership::ownership_paths;
pub(in crate::git) use ownership::registered_identity;
pub(in crate::git) use ownership::registered_run;
use ownership::root_identity;
use ownership::run;

mod journal;
use journal::check;
pub use journal::read;
use journal::resolve;
use journal::safe_dir;
use journal::save;
#[cfg(test)]
use journal::validate;
