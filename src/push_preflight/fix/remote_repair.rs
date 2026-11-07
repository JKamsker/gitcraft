//! Completion-owned remote repair. The host owns its guard/evidence until native Git exits.
use super::transaction::{HookScript, path_line, script, supported_version};
use crate::{git, remote};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
#[derive(Clone)]
struct Pending {
    owner: remote::OwnedDispatch,
    directory: String,
    nonce: String,
    kind: &'static str,
}
fn pending() -> &'static Mutex<HashMap<std::path::PathBuf, Pending>> {
    static VALUE: OnceLock<Mutex<HashMap<std::path::PathBuf, Pending>>> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}
fn retire(repo: &Path, nonce: &str) {
    let mut entries = pending()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if entries.get(repo).is_some_and(|entry| entry.nonce == nonce) {
        entries.remove(repo);
    }
}
fn read(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let owner = remote::OwnedDispatch::default();
    let words = args
        .iter()
        .map(|word| remote::sh_quote(word))
        .collect::<Vec<_>>()
        .join(" ");
    let command = if args == ["--version"] {
        format!("exec git {words}")
    } else {
        format!("exec git --no-lazy-fetch {words}")
    };
    let output=git::owned_repair_host(repo,&command,Vec::new(),&[],&owner).map_err(|_|"Could not establish the remote repair metadata result; no repair transaction was dispatched".to_owned())?;
    if output.code != 0 || output.stdout.len() > 16 * 1024 * 1024 {
        return Err("Remote repair metadata is unavailable or oversized".into());
    }
    Ok(output.stdout)
}
fn terminal_receipt(bytes: &[u8], nonce: &str, kind: &str) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    if !text.ends_with('\n') {
        return false;
    }
    let fields = text.split_terminator('\n').collect::<Vec<_>>();
    fields.len() == 5
        && fields[0] == "gitcraft-owned-repair-v1"
        && fields[1] == nonce
        && !fields[2].is_empty()
        && fields[2].bytes().all(|byte| byte.is_ascii_digit())
        && fields[2].parse::<u8>().is_ok()
        && fields[3] == kind
        && (fields[4] == "unknown" || super::super::valid_object_id(fields[4]))
}

fn absolute(repo: &Path, args: &[&str]) -> Result<String, String> {
    let path = path_line(read(repo, args)?)?;
    if !path.starts_with('/') {
        return Err("Remote repair requires absolute host metadata paths".into());
    }
    Ok(path)
}
fn prior(repo: &Path) -> Result<(), String> {
    let held = pending()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(repo)
        .cloned();
    let Some(held) = held else {
        return Ok(());
    };
    let result = held.owner.run(|| {
        let reader = remote::OwnedDispatch::default();
        git::owned_repair_host(
            repo,
            &format!(
                "test -f {0}/outcome && cat {0}/outcome",
                remote::sh_quote(&held.directory)
            ),
            Vec::new(),
            &[],
            &reader,
        )
    });
    let finished = result.is_ok_and(|output| {
        output.code == 0 && terminal_receipt(&output.stdout, &held.nonce, held.kind)
    });
    if finished && !held.owner.pending() {
        let mut entries = pending()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if entries
            .get(repo)
            .is_some_and(|value| value.nonce == held.nonce)
        {
            entries.remove(repo);
        }
        return Ok(());
    }
    Err(format!(
        "A previous remote repair has no confirmed terminal outcome. Recheck after the host finishes, or inspect recovery in {}. No replacement repair was dispatched.",
        held.directory
    ))
}
pub(super) fn available(repo: &Path) -> Result<(), String> {
    prior(repo)?;
    if !supported_version(&read(repo, &["--version"])?) {
        return Err(
            "Guarded remote repair requires Git 2.50 or later on the repository host".into(),
        );
    }
    Ok(())
}
pub(super) fn execute(
    repo: &Path,
    branch: &str,
    old: &str,
    new: &str,
    merge_base: Option<&str>,
) -> Result<(), String> {
    available(repo)?;
    if ![old, new].into_iter().all(super::super::valid_object_id)
        || merge_base.is_some_and(|base| !super::super::valid_object_id(base))
    {
        return Err("Repair object identity is invalid".into());
    }
    let index = absolute(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let original = absolute(
        repo,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "hooks/reference-transaction",
        ],
    )?;
    let shared = read(
        repo,
        &["rev-parse", "--path-format=absolute", "--shared-index-path"],
    )?;
    let shared = if shared.is_empty() || shared == b"\n" {
        None
    } else {
        Some(path_line(shared)?)
    };
    if shared.as_ref().is_some_and(|path| !path.starts_with('/')) {
        return Err("Shared-index path is not absolute".into());
    }
    let refs = path_line(read(repo, &["rev-parse", "--show-ref-format"])?)?;
    let objects = path_line(read(repo, &["rev-parse", "--show-object-format"])?)?;
    if !matches!(refs.as_str(), "files" | "reftable")
        || !matches!(objects.as_str(), "sha1" | "sha256")
    {
        return Err("Unsupported remote repair backend".into());
    }
    let version = path_line(read(repo, &["--version"])?)?;
    let affected = read(
        repo,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            "--no-renames",
            old,
            new,
        ],
    )?;
    let affected = affected
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            String::from_utf8(path.to_vec())
                .map_err(|_| "Remote repair requires UTF-8 changed paths".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let before_tree = read(repo, &["rev-parse", "--verify", &format!("{old}^{{tree}}")])?;
    let after_tree = read(repo, &["rev-parse", "--verify", &format!("{new}^{{tree}}")])?;
    let proof = format!("{branch}\n{old}\n{new}\n");
    let wrapper = script(HookScript {
        original: &original,
        branch,
        old,
        new,
        index: &index,
        shared: shared.as_deref(),
        affected: &affected,
        proof: &proof,
        needs_checkout: before_tree != after_tree,
    });
    if wrapper.len() > 4 * 1024 * 1024 {
        return Err("Remote repair wrapper exceeds its size limit".into());
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "No clock")?
        .as_nanos();
    let nonce = format!(
        "{:x}{stamp:x}{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let parent = index.rsplit_once('/').ok_or("No index parent")?.0;
    let directory = format!("{parent}/gitcraft-repair-{nonce}");
    let guard = format!("{parent}/gitcraft-active-repair");
    let owner = remote::OwnedDispatch::default();
    {
        let mut entries = pending()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if entries.contains_key(repo) || entries.len() >= 64 {
            return Err("Resolve pending remote repairs before starting another".into());
        }
        entries.insert(
            repo.into(),
            Pending {
                owner: owner.clone(),
                directory: directory.clone(),
                nonce: nonce.clone(),
                kind: if merge_base.is_some() {
                    "merge"
                } else {
                    "rewrite"
                },
            },
        );
    }
    let env = vec![
        ("GITCRAFT_REPAIR_DIR", directory.clone()),
        ("GITCRAFT_REPAIR_GUARD", guard),
        ("GITCRAFT_REPAIR_NONCE", nonce.clone()),
        ("GITCRAFT_REPAIR_BRANCH", branch.into()),
        ("GITCRAFT_REPAIR_OLD", old.into()),
        ("GITCRAFT_REPAIR_NEW", new.into()),
        ("GITCRAFT_REPAIR_INDEX", index),
        ("GITCRAFT_REPAIR_SHARED", shared.unwrap_or_default()),
        ("GITCRAFT_REPAIR_REFS", refs),
        ("GITCRAFT_REPAIR_OBJECTS", objects),
        ("GITCRAFT_REPAIR_VERSION", version),
        (
            "GITCRAFT_REPAIR_KIND",
            if merge_base.is_some() {
                "merge"
            } else {
                "rewrite"
            }
            .into(),
        ),
        ("GITCRAFT_REPAIR_BASE", merge_base.unwrap_or("").into()),
    ];
    let result = git::owned_repair_host(
        repo,
        include_str!("remote_repair.sh"),
        wrapper.into_bytes(),
        &env,
        &owner,
    );
    match result {
        Err(remote::OwnedFailure::Unknown) => Err(format!(
            "The remote repair outcome is unknown. It may still be running; its owned guard and recovery remain in {directory}. Recheck after the host finishes; no automatic replacement or cleanup was dispatched."
        )),
        Err(remote::OwnedFailure::NotDispatched(error)) => {
            retire(repo, &nonce);
            Err(format!("The repair was not dispatched: {error}"))
        }
        Ok(output) => {
            retire(repo, &nonce);
            if output.code != 0 || output.stdout != format!("complete:{nonce}\n").as_bytes() {
                return Err(format!(
                    "Remote repair did not establish complete success. Inspect actual HEAD and concurrent edits, plus recovery in {directory}; no rollback overwrote later work."
                ));
            }
            // Only this unique, acknowledged completed directory is eligible for cleanup.
            // No shared guard or later operation is touched by a delayed cleanup request.
            let cleanup = format!(
                "d={}; [ \"$(cat \"$d/owner\")\" = {} ] && [ -f \"$d/outcome\" ] && rm -rf -- \"$d\"",
                remote::sh_quote(&directory),
                remote::sh_quote(&nonce)
            );
            let cleanup_owner = remote::OwnedDispatch::default();
            let _ = git::owned_repair_host(repo, &cleanup, Vec::new(), &[], &cleanup_owner);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_receipts_require_complete_schema_nonce_code_kind_and_oid() {
        let good = format!(
            "gitcraft-owned-repair-v1\nabc123\n0\nrewrite\n{}\n",
            "1".repeat(40)
        );
        assert!(terminal_receipt(good.as_bytes(), "abc123", "rewrite"));
        for cut in 0..good.len() {
            assert!(!terminal_receipt(
                &good.as_bytes()[..cut],
                "abc123",
                "rewrite"
            ));
        }
        for bad in [
            good.replace("v1", "v2"),
            good.replace("abc123", "other"),
            good.replace("\n0\n", "\n256\n"),
            good.replace("rewrite", "merge"),
            good.replace(&"1".repeat(40), "not-an-oid"),
            format!("{good}extra\n"),
        ] {
            assert!(!terminal_receipt(bad.as_bytes(), "abc123", "rewrite"));
        }
        println!(
            "complete receipt accepted; all truncations/wrong schema/nonce/code/kind/OID/extra rejected"
        );
    }
    #[test]
    fn recording_registry_retirement_cannot_erase_a_replacement_owner() {
        let key = Path::new("/recording-only/remote-repair-retirement");
        let value = Pending {
            owner: remote::OwnedDispatch::default(),
            directory: "/recording-only/private".into(),
            nonce: "newer".into(),
            kind: "rewrite",
        };
        pending()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.into(), value);
        retire(key, "older");
        assert!(
            pending()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(key)
        );
        retire(key, "newer");
        assert!(
            !pending()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(key)
        );
    }
}
