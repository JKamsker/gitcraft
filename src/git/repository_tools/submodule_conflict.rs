use super::guard::{Runner, Snapshot, text};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub oid: String,
    pub label: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Conflict {
    pub snapshot: Snapshot,
    pub path: String,
    pub child: PathBuf,
    pub stages: String,
    pub base: Option<String>,
    pub ours: String,
    pub theirs: String,
    pub graph: String,
    pub candidates: Vec<Candidate>,
    pub head: String,
}
fn contains(repo: &Path, ancestor: &str, commit: &str) -> Result<bool, String> {
    let out = crate::git::run_captured(
        repo,
        &[
            "--no-replace-objects",
            "merge-base",
            "--is-ancestor",
            ancestor,
            commit,
        ],
        None,
    )?;
    match out.code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(out.stderr),
    }
}
pub(crate) fn read(repo: &Path, path: &str) -> Result<Conflict, String> {
    if path.is_empty()
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Choose a conflicted submodule path inside this repository".into());
    }
    let stages = text(
        repo,
        &[
            "--literal-pathspecs",
            "ls-files",
            "--unmerged",
            "-z",
            "--",
            path,
        ],
    )?;
    let mut base: Option<String> = None;
    let mut ours = None;
    let mut theirs = None;
    for record in stages.split('\0').filter(|s| !s.is_empty()) {
        let (meta, name) = record
            .split_once('\t')
            .ok_or("Invalid conflicted index entry")?;
        let f: Vec<_> = meta.split_whitespace().collect();
        if name != path || f.len() != 3 || f[0] != "160000" {
            return Err("This is not a pure submodule-pointer conflict".into());
        }
        match f[2] {
            "1" => base = Some(f[1].into()),
            "2" => ours = Some(f[1].to_owned()),
            "3" => theirs = Some(f[1].to_owned()),
            _ => return Err("Unexpected conflict stage".into()),
        }
    }
    let ours =
        ours.ok_or("Ours has no submodule commit; handle this add/delete conflict in Changes")?;
    let theirs = theirs
        .ok_or("Theirs has no submodule commit; handle this add/delete conflict in Changes")?;
    let child = crate::remote::join(repo, path);
    if !text(&child, &["rev-parse", "--show-prefix"])?
        .trim()
        .is_empty()
    {
        return Err("Initialize/open this submodule before resolving its graph".into());
    }
    for oid in [&ours, &theirs] {
        text(
            &child,
            &[
                "--no-replace-objects",
                "cat-file",
                "-e",
                &format!("{oid}^{{commit}}"),
            ],
        )?;
    }
    let head = text(&child, &["rev-parse", "--verify", "HEAD^{commit}"])?
        .trim()
        .to_owned();
    let refs = text(
        &child,
        &[
            "for-each-ref",
            "--contains",
            &ours,
            "--format=%(objectname)%00%(refname)",
        ],
    )?;
    let mut candidates = Vec::new();
    for row in refs.lines() {
        let Some((oid, label)) = row.split_once('\0') else {
            continue;
        };
        if candidates.iter().any(|c: &Candidate| c.oid == oid) {
            continue;
        }
        if contains(&child, &ours, oid)? && contains(&child, &theirs, oid)? {
            candidates.push(Candidate {
                oid: oid.into(),
                label: label.into(),
            });
        }
        if candidates.len() >= 100 {
            break;
        }
    }
    if contains(&child, &ours, &head)?
        && contains(&child, &theirs, &head)?
        && !candidates.iter().any(|c| c.oid == head)
    {
        candidates.push(Candidate {
            oid: head.clone(),
            label: "Submodule HEAD".into(),
        });
    }
    let mut tips = vec![ours.clone(), theirs.clone()];
    if let Some(base) = &base {
        tips.push(base.clone());
    }
    tips.extend(candidates.iter().map(|c| c.oid.clone()));
    let mut args = vec![
        "--no-replace-objects",
        "--no-pager",
        "log",
        "--graph",
        "--no-color",
        "--no-show-signature",
        "--format=%H %s",
        "--max-count=200",
        "--boundary",
    ];
    args.extend(tips.iter().map(String::as_str));
    let mut graph = text(&child, &args)?;
    for (oid, label) in [
        (Some(ours.as_str()), "OURS"),
        (Some(theirs.as_str()), "THEIRS"),
        (base.as_deref(), "BASE"),
    ] {
        if let Some(oid) = oid {
            graph = graph.replace(oid, &format!("{} [{label}]", &oid[..12.min(oid.len())]));
        }
    }
    Ok(Conflict {
        snapshot: Snapshot::capture(repo)?,
        path: path.into(),
        child,
        stages,
        base,
        ours,
        theirs,
        graph,
        candidates,
        head,
    })
}
pub(crate) fn resolve(
    runner: &Runner<'_>,
    conflict: &Conflict,
    oid: &str,
) -> Result<String, String> {
    conflict.snapshot.check(runner.repo)?;
    if text(
        runner.repo,
        &[
            "--literal-pathspecs",
            "ls-files",
            "--unmerged",
            "-z",
            "--",
            &conflict.path,
        ],
    )? != conflict.stages
    {
        return Err("Conflict stages changed. Reopen the submodule graph".into());
    }
    if !super::object::valid_oid(oid)
        || !contains(&conflict.child, &conflict.ours, oid)?
        || !contains(&conflict.child, &conflict.theirs, oid)?
    {
        return Err("Choose an existing submodule commit containing both ours and theirs".into());
    }
    text(
        &conflict.child,
        &[
            "--no-replace-objects",
            "cat-file",
            "-e",
            &format!("{oid}^{{commit}}"),
        ],
    )?;
    runner.run(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{oid},{}", conflict.path),
    ])?;
    Ok(format!(
        "Recorded submodule {} at {oid} in the parent index. The submodule checkout was not moved; commit the resolution in the parent when ready.",
        conflict.path
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_contains_sides_and_records_a_real_merge_only_after_stage_check() -> Result<(), String>
    {
        let sub = crate::git::fixture::Repo::new("sub-conflict-source")?;
        let base = sub.commit("base", &[])?;
        sub.git(&["branch", "side"])?;
        let ours = sub.commit("ours", &[])?;
        sub.git(&["checkout", "side"])?;
        let theirs = sub.commit("theirs", &[])?;
        let tree = sub.tree("HEAD")?;
        let merged = sub
            .git(&[
                "commit-tree",
                &tree,
                "-p",
                &ours,
                "-p",
                &theirs,
                "-m",
                "merge both",
            ])?
            .trim()
            .to_owned();
        sub.git(&["update-ref", "refs/heads/resolved", &merged])?;
        let r = crate::git::fixture::Repo::new("sub-conflict-parent")?;
        r.git(&[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            &sub.path().to_string_lossy(),
            "lib",
        ])?;
        r.commit("parent", &[])?;
        crate::git::run_input(
            Some(r.path()),
            &["update-index", "--index-info"],
            &format!(
                "0 {}\tlib\n160000 {base} 1\tlib\n160000 {ours} 2\tlib\n160000 {theirs} 3\tlib\n",
                "0".repeat(40)
            ),
        )?;
        let conflict = read(r.path(), "lib")?;
        assert!(conflict.graph.contains("[OURS]"));
        assert!(conflict.graph.contains("[THEIRS]"));
        assert!(conflict.candidates.iter().any(|c| c.oid == merged));
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        assert!(resolve(&runner, &conflict, &ours).is_err());
        resolve(&runner, &conflict, &merged)?;
        assert!(r.git(&["ls-files", "--unmerged"])?.is_empty());
        assert!(r.git(&["ls-files", "--stage", "lib"])?.contains(&merged));
        assert!(resolve(&runner, &conflict, &merged).is_err());
        Ok(())
    }
}
