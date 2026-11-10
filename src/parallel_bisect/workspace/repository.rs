//! Private scratch repository discovery and generation-safe cleanup scripts.
use super::*;

/// The folder for bisecting in `repo` (on `machine`): as the app names it, and as a path there.
#[cfg(test)]
pub fn base_for(repo: &Path, machine: &Machine) -> Result<(PathBuf, String), String> {
    let there = format!("{}/gitcraft-bisect-work", canonical_common(repo)?);
    Ok((machine.path(&there), there))
}

pub(super) const CLEAR: &str = r#"LC_ALL=C; export LC_ALL
b=$1
ledger=$2
own=$3
generation=$4
shift 4
fail() { printf 'gitcraft: %s; external recovery ledger retained\n' "$1" >&2; exit 1; }
[ "$b" = "${ledger%/*}/gitcraft-bisect-work" ] || fail 'cleanup base is outside claimed repository'
[ ! -L "$b" ] && [ ! -L "$ledger" ] || fail 'workspace or ledger became a symbolic link'
for f in "$b"/*.pid "$b"/*.owner "$b"/owner; do
  [ ! -e "$f" ] && [ ! -L "$f" ] || fail 'unresolved legacy evidence in workspace base'
done
IFS=' ' read -r kind count || fail 'missing generation proof'
[ "$kind" = G ] || fail 'invalid generation proof'
case $count in ''|*[!0-9]*) fail 'invalid generation count';; esac
[ "$count" -gt 0 ] && [ "$count" -le 4097 ] || fail 'generation proof bound'
generations=''
i=0
while [ "$i" -lt "$count" ]; do
  IFS= read -r g || fail 'truncated generation proof'
  [ "${#g}" -eq 64 ] || fail 'invalid generation length'
  case $g in *[!0-9a-f]*) fail 'invalid generation';; esac
  generations="$generations $g"
  i=$((i+1))
done
case " $generations " in *" $generation "*) ;; *) fail 'current generation missing';; esac
IFS=' ' read -r kind count || fail 'missing record proof'
[ "$kind" = R ] || fail 'invalid record proof'
case $count in ''|*[!0-9]*) fail 'invalid record count';; esac
[ "$count" -le 4096 ] || fail 'record proof bound'
i=0
for f in "$ledger"/*.owner; do
  [ -e "$f" ] || [ -L "$f" ] || continue
  [ "${f##*/}" != "$own" ] || continue
  IFS= read -r expected && IFS= read -r proof || fail 'owner records changed'
  [ "${f##*/}" = "$expected" ] && [ ! -L "$f" ] || fail 'owner record changed'
  # The actual fenced owner validated private regular files before this bounded reread.
  actual=$(od -An -v -tx1 -N 2049 "$f") || fail 'cannot inspect owner record'
  actual=$(printf '%s' "$actual" | tr -d ' \n') || fail 'cannot encode owner record'
  [ "${#actual}" -le 4096 ] && [ "$actual" = "$proof" ] || fail 'owner record replaced'
  i=$((i+1))
done
[ "$i" -eq "$count" ] || fail 'owner record set changed'
if IFS= read -r extra; then fail 'unexpected trailing recovery proof'; fi
refs=$(git for-each-ref --format='%(refname) %(objectname)' refs/gitcraft-bisect/) || fail 'cannot list bisect refs'
while IFS=' ' read -r r expected; do
  [ -n "$r" ] || continue
  suffix=${r#refs/gitcraft-bisect/}
  g=${suffix%%/*}
  case " $generations " in *" $g "*) ;; *) fail "ref has no recovered generation: $r";; esac
done <<GITCRAFT_REFS
$refs
GITCRAFT_REFS
worktrees=$(git -c core.quotePath=false worktree list --porcelain) || fail 'cannot list worktrees'
while IFS= read -r line; do
  case $line in 'worktree "'*) fail 'quoted worktree path requires manual recovery';; esac
done <<GITCRAFT_WORKTREES
$worktrees
GITCRAFT_WORKTREES
while IFS= read -r line; do
  case $line in 'worktree '*) w=${line#worktree }; case $w in "$b"/*)
    git worktree remove --force --force "$w" >/dev/null || fail "cannot remove worktree $w";; esac;; esac
done <<GITCRAFT_WORKTREES
$worktrees
GITCRAFT_WORKTREES
remaining=$(git -c core.quotePath=false worktree list --porcelain) || fail 'cannot verify remaining worktrees'
while IFS= read -r line; do
  case $line in "worktree $b/"*) fail 'a bisect worktree remains registered';; esac
done <<GITCRAFT_WORKTREES
$remaining
GITCRAFT_WORKTREES
c=$(git rev-parse --git-common-dir) || fail 'cannot resolve worktree metadata'
for g in "$c"/worktrees/*/gitdir; do
  [ -f "$g" ] || continue
  target=$(cat "$g") || fail 'cannot read worktree metadata'
  case $target in "$b"/*)
    rm -rf -- "${g%/gitdir}" || fail 'cannot remove bisect worktree metadata'
    [ ! -e "${g%/gitdir}" ] && [ ! -L "${g%/gitdir}" ] || fail 'worktree metadata remains';;
  esac
done
while IFS=' ' read -r r expected; do
  [ -n "$r" ] || continue
  git update-ref -d "$r" "$expected" || fail "cannot remove unchanged ref $r"
done <<GITCRAFT_REFS
$refs
GITCRAFT_REFS
remaining=$(git for-each-ref --format='%(refname)' refs/gitcraft-bisect/) || fail 'cannot verify remaining refs'
[ -z "$remaining" ] || fail 'bisect refs remain'
[ ! -L "$b" ] || fail 'workspace became a symbolic link'
rm -rf -- "$b" || fail "cannot remove workspace $b"
[ ! -e "$b" ] && [ ! -L "$b" ] || fail 'workspace remains after removal'
"#;

/// The repository the bisect works in on `machine` (see [`Workspace::open`]).
pub(super) fn repository_for(
    source: &Path,
    machine: &Machine,
    repo_there: Option<&str>,
) -> Result<PathBuf, String> {
    let named = repo_there.map(str::trim).filter(|p| !p.is_empty());
    if let Some(named) = named {
        let there = match named.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => {
                format!("{}{rest}", machine.home()?)
            }
            _ => named.to_owned(),
        };
        let repo = machine.path(&there);
        git::run(Some(&repo), &["rev-parse", "--git-dir"])
            .map_err(|_| format!("{there} on {} is not a git repository", machine.label()))?;
        return Ok(repo);
    }
    if *machine == Machine::of(source) {
        return Ok(source.to_path_buf());
    }
    scratch_repository(source, machine)
}

/// A repository of its own on `machine` for bisecting `source`'s commits there, made the first
/// time: `~/.cache/gitcraft/bisect/<name>-<hash of where source is>`. It keeps the commits
/// sent to it, so that the next bisect sends less.
pub(super) fn scratch_repository(source: &Path, machine: &Machine) -> Result<PathBuf, String> {
    let name: String = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let digest = format!("{:x}", md5::compute(source.to_string_lossy().as_bytes()));
    let home = machine.home()?;
    let there = format!(
        "{}/.cache/gitcraft/bisect/{}-{}",
        home.trim_end_matches(['/', '\\']),
        name.trim_start_matches('.'),
        &digest[..8]
    );
    let from = match machine {
        Machine::Here => None,
        Machine::There(_) => Some(machine.path(&home)),
    };
    git::run(from.as_deref(), &["init", "-q", &there]).map_err(|e| {
        format!(
            "could not make a repository for the bisect on {}: {e}",
            machine.label()
        )
    })?;
    Ok(machine.path(&there))
}

/// `repo`'s git folder shared by its worktrees, as an absolute path on its machine.
pub(super) fn git_common_dir(repo: &Path) -> Result<String, String> {
    let printed = git::run(
        Some(repo),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .or_else(|_| git::run(Some(repo), &["rev-parse", "--git-common-dir"]))?;
    let printed = printed.trim();
    let absolute = printed.starts_with('/') || printed.get(1..2) == Some(":");
    Ok(if absolute {
        printed.to_owned()
    } else {
        format!(
            "{}/{printed}",
            remote::git_path(repo).trim_end_matches(['/', '\\'])
        )
    })
}

pub(in super::super) fn canonical_common(repo: &Path) -> Result<String, String> {
    let common = git_common_dir(repo)?;
    let canonical = if remote::parse(repo).is_some() {
        runner::shell(repo, "cd \"$1\" && pwd -P", &[&common])?
            .trim()
            .to_owned()
    } else {
        crate::app::strip_verbatim(std::fs::canonicalize(&common).map_err(|e| e.to_string())?)
            .to_string_lossy()
            .replace('\\', "/")
    };
    if canonical.contains(['\n', '\r', '\0'])
        || !(canonical.starts_with('/') || canonical.get(1..2) == Some(":"))
    {
        return Err("unsupported canonical repository path".into());
    }
    Ok(canonical)
}

pub(super) fn has_commit(repo: &Path, hash: &str) -> bool {
    git::run(
        Some(repo),
        &["cat-file", "-e", &format!("{hash}^{{commit}}")],
    )
    .is_ok()
}
