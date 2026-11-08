use super::guard::{Runner, Snapshot, text};
use std::path::Path;

#[derive(Clone, Debug)]
pub(crate) struct Replacement {
    pub original: String,
    pub effective: String,
    pub parents: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Histories {
    pub snapshot: Snapshot,
    pub replacements: Vec<Replacement>,
    pub legacy: String,
    legacy_bytes: Option<Vec<u8>>,
    pub original: String,
    pub effective: String,
}
pub(crate) fn read(repo: &Path) -> Result<Histories, String> {
    let cancel = crate::tasks::cancellation();
    let legacy_bytes = graft_bytes(repo, &|| !cancel.is_cancelled())?;
    let legacy = String::from_utf8(legacy_bytes.clone().unwrap_or_default())
        .map_err(|_| "Legacy graft metadata is not UTF-8; inspect info/grafts with Git")?;
    let snapshot = Snapshot::capture(repo)?;
    let rows = text(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/replace/",
        ],
    )?;
    let mut replacements = Vec::new();
    for line in rows.lines() {
        let Some((name, effective)) = line.split_once(' ') else {
            return Err("Invalid replace ref record".into());
        };
        let original = name
            .strip_prefix("refs/replace/")
            .ok_or("Invalid replacement")?
            .to_owned();
        let parents = text(
            repo,
            &[
                "--no-replace-objects",
                "show",
                "-s",
                "--format=%P",
                effective,
            ],
        )
        .unwrap_or_default()
        .trim()
        .to_owned();
        replacements.push(Replacement {
            original,
            effective: effective.into(),
            parents,
        });
    }
    let args = [
        "--no-pager",
        "log",
        "--exclude=refs/replace/*",
        "--all",
        "--graph",
        "--decorate=full",
        "--no-color",
        "--no-show-signature",
        "--format=%h %d %s",
        "-n",
        "180",
    ];
    let effective = text(repo, &args)?;
    // Bypass legacy graft files too, while retaining the selected profile and repository.
    let original = text(
        repo,
        &[
            "-c",
            "alias.gitcraft-original=!f() { GIT_GRAFT_FILE=/dev/null git --no-replace-objects --no-pager log --exclude=refs/replace/* --all --graph --decorate=full --no-color --no-show-signature --format='%h %d %s' -n 180; }; f",
            "gitcraft-original",
        ],
    )?;
    Ok(Histories {
        snapshot,
        replacements,
        legacy,
        legacy_bytes,
        original,
        effective,
    })
}
fn graft_bytes(repo: &Path, active: &dyn Fn() -> bool) -> Result<Option<Vec<u8>>, String> {
    let path = text(repo, &["rev-parse", "--git-path", "info/grafts"])?;
    super::object::files::read(
        &crate::remote::resolve(repo, path.trim()),
        64 * 1024,
        active,
    )
    .map_err(|e| format!("Cannot inspect legacy info/grafts (64 KiB regular-file limit): {e}"))
}
impl Histories {
    fn check(&self, runner: &Runner<'_>) -> Result<(), String> {
        if graft_bytes(runner.repo, &|| !runner.control.is_cancelled())? != self.legacy_bytes {
            return Err("Legacy graft file changed; read a fresh history preview".into());
        }
        self.snapshot.check(runner.repo)?;
        Ok(())
    }
}
pub(crate) fn graft(
    runner: &Runner<'_>,
    history: &Histories,
    original: &str,
    parent: &str,
) -> Result<String, String> {
    history.check(runner)?;
    let resolve = |s: &str| -> Result<String, String> {
        if !super::object::valid_oid(s) {
            return Err("Use full immutable commit IDs from the preview".into());
        }
        Ok(text(
            runner.repo,
            &[
                "--no-replace-objects",
                "rev-parse",
                "--verify",
                &format!("{s}^{{commit}}"),
            ],
        )?
        .trim()
        .into())
    };
    let original = resolve(original)?;
    let parent = resolve(parent)?;
    if original == parent {
        return Err("A commit cannot be its own parent".into());
    }
    // Refuse cycles through the effective graph, including existing replacement joins.
    let ancestry = super::captured(
        runner.repo,
        &["merge-base", "--is-ancestor", &original, &parent],
        None,
    )?;
    match ancestry.code {
        Some(0) => return Err("This join would create a history cycle".into()),
        Some(1) => (),
        _ => return Err(ancestry.stderr),
    }
    history.check(runner)?;
    runner.run(&["replace", "--graft", &original, &parent])?;
    Ok(format!(
        "Temporary graft {original} → parent {parent}. Remove its replace ref to restore original history."
    ))
}
pub(crate) fn remove(
    runner: &Runner<'_>,
    history: &Histories,
    original: &str,
    effective: &str,
) -> Result<String, String> {
    history.check(runner)?;
    if !history
        .replacements
        .iter()
        .any(|r| r.original == original && r.effective == effective)
    {
        return Err("Replacement is absent from the preview".into());
    }
    runner.run(&[
        "update-ref",
        "-d",
        &format!("refs/replace/{original}"),
        effective,
    ])
}
pub(crate) fn convert(runner: &Runner<'_>, history: &Histories) -> Result<String, String> {
    history.check(runner)?;
    runner.run(&["replace", "--convert-graft-file"])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporary_join_has_two_histories_and_removes_with_stale_guard() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("replace-graft")?;
        let first = r.commit("first-history", &[])?;
        r.git(&["checkout", "--orphan", "imported"])?;
        let second = r.commit("second-history", &[])?;
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        let before = read(r.path())?;
        graft(&runner, &before, &second, &first)?;
        let after = read(r.path())?;
        assert_eq!(after.replacements.len(), 1);
        assert_eq!(after.replacements[0].parents, first);
        assert_ne!(after.original, after.effective);
        assert!(remove(&runner, &before, &second, &after.replacements[0].effective).is_err());
        assert!(graft(&runner, &after, &first, &second).is_err());
        remove(&runner, &after, &second, &after.replacements[0].effective)?;
        assert!(read(r.path())?.replacements.is_empty());
        Ok(())
    }
}
