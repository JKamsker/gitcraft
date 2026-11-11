//! Dependency advertisements and reachability inspection.

use super::*;

/// Register immutable expected names/OIDs before fetch. Finalization is independent
/// of operation cancellation and deletes only those exact refs with expected OIDs.
pub(super) struct Advertisement {
    pub(super) namespace: String,
    pub(super) refs: BTreeMap<String, String>,
}

impl Advertisement {
    pub(super) fn cleanup(
        &self,
        mut run: impl FnMut(&[&str], Vec<u8>) -> Result<String, String>,
    ) -> Result<(), String> {
        let rows = run(
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                &self.namespace,
            ],
            Vec::new(),
        )?;
        let mut input = String::new();
        for row in rows.lines() {
            let (name, oid) = row.split_once(' ').ok_or(crate::i18n::text(
                "backend-invalid-owned-dependency-ref-record-ba2eab",
            ))?;
            if self.refs.get(name).map(String::as_str) != Some(oid) {
                return Err(crate::i18n::text(
                    "backend-dependency-ref-changed-or-is-foreign-retained-for-inspection-cab019",
                )
                .into());
            }
            input.push_str(&format!("delete {name} {oid}\n"));
        }
        if !input.is_empty() {
            run(&["update-ref", "--stdin"], input.into_bytes())?;
        }
        Ok(())
    }
    pub(super) fn finish(
        &self,
        fetch: Result<String, String>,
        cleanup: Result<(), String>,
    ) -> Result<Vec<String>, String> {
        match (fetch, cleanup) {
            (Ok(_), Ok(())) => Ok(self.refs.values().cloned().collect()),
            (Err(e), Ok(())) => Err(e),
            (fetch, Err(e)) => Err(crate::i18n::message(
                "backend-owned-ref-cleanup-failed-e-inspect-retained-namespace-before-retr-594151",
                &[
                    (
                        "fetch",
                        (fetch.err().unwrap_or_else(|| {
                            crate::i18n::text("backend-dependency-fetch-completed-0cc4ca").into()
                        }))
                        .to_string()
                        .into(),
                    ),
                    ("detail", (e).to_string().into()),
                    ("name", (self.namespace).to_string().into()),
                ],
            )),
        }
    }
}

pub(super) fn advertised(
    runner: &Runner<'_>,
    node: &Dependency,
    budget: &super::super::ReadBudget,
) -> Result<Vec<String>, String> {
    budget.take()?;
    let rows = runner.endpoint(
        &node.repo,
        &node.fetch_url,
        &node.remote,
        &["ls-remote", "--refs", "--"],
        &["refs/heads/*", "refs/tags/*"],
    )?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let ns = format!(
        "refs/gitcraft/dependency-check/{stamp}-{}-{}",
        std::process::id(),
        CHECKS.fetch_add(1, Ordering::Relaxed)
    );
    let mut owned = Advertisement {
        namespace: ns,
        refs: BTreeMap::new(),
    };
    let mut specs = Vec::new();
    for row in rows.lines() {
        if specs.len() >= 256 {
            return Err(crate::i18n::text(
                "backend-more-than-256-advertised-dependency-refs-narrow-this-remote-befor-a19fa4",
            )
            .into());
        }
        let (oid, name) = row.split_once('\t').ok_or(crate::i18n::text(
            "backend-invalid-dependency-advertisement-181a94",
        ))?;
        if !super::super::object::valid_oid(oid)
            || !(name.starts_with("refs/heads/") || name.starts_with("refs/tags/"))
        {
            return Err(
                crate::i18n::text("backend-invalid-advertised-dependency-ref-61782b").into(),
            );
        }
        let target = format!("{}/{}", owned.namespace, specs.len());
        specs.push(format!("+{oid}:{target}"));
        owned.refs.insert(target, oid.into());
    }
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    // Ownership finalizer exists before the first command capable of creating refs.
    budget.take()?;
    let fetch = runner.endpoint(
        &node.repo,
        &node.fetch_url,
        &node.remote,
        &[
            "fetch",
            "--no-recurse-submodules",
            "--no-tags",
            "--no-write-fetch-head",
            "--",
        ],
        &specs.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let cleanup = crate::tasks::with_cancellation(crate::tasks::Cancellation::default(), || {
        owned.cleanup(|args, input| {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Err(crate::i18n::text("backend-cleanup-deadline-reached-65ec75").into());
            }
            String::from_utf8(
                super::super::capture_with_timeout(
                    &node.repo,
                    args,
                    input,
                    1 << 20,
                    &[],
                    None,
                    left,
                )?
                .checked(&[0])?,
            )
            .map_err(|_| crate::i18n::text("backend-invalid-cleanup-metadata-7bf0ec").into())
        })
    });
    let tips = owned.finish(fetch, cleanup)?;
    budget.take()?;
    Ok(tips)
}

pub(super) fn reachable(
    runner: &Runner<'_>,
    node: &Dependency,
    budget: &super::super::ReadBudget,
) -> Result<bool, String> {
    let tips = advertised(runner, node, budget)?;
    for required in &node.required {
        let mut found = false;
        for tip in &tips {
            if ancestor(runner, budget, &node.repo, required, tip)? {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn inspect_node(
    runner: &Runner<'_>,
    node: &mut Dependency,
    budget: &super::super::ReadBudget,
) -> Result<(), String> {
    let text = |repo: &Path, args: &[&str]| budget.text(repo, args, Some(runner.control.clone()));
    if !text(&node.repo, &["rev-parse", "--show-prefix"])?
        .trim()
        .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-initialize-this-submodule-first-git-currently-resolves-it-as-its-569629",
        )
        .into());
    }
    if !node.remotes.contains(&node.remote) {
        return Err(crate::i18n::text(
            "backend-choose-a-remote-that-collaborators-can-fetch-from-2f32f5",
        )
        .into());
    }
    node.fetch_url = text(&node.repo, &["remote", "get-url", "--", &node.remote])?
        .trim_end_matches(['\n', '\r'])
        .into();
    let urls = text(
        &node.repo,
        &["remote", "get-url", "--push", "--all", "--", &node.remote],
    )?;
    let urls: Vec<_> = urls.lines().collect();
    if urls.len() != 1 {
        return Err(crate::i18n::text(
            "backend-choose-a-submodule-remote-with-exactly-one-push-url-9a7bdb",
        )
        .into());
    }
    node.push_url = urls[0].into();
    super::super::remote_config::validate_url(&node.fetch_url)?;
    super::super::remote_config::validate_url(&node.push_url)?;
    node.published = reachable(runner, node, budget)?;
    let refs = text(
        &node.repo,
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:strip=2)",
            "refs/heads/",
        ],
    )?;
    if refs.lines().count() > 256 {
        return Err(crate::i18n::text(
            "backend-more-than-256-candidate-submodule-branches-narrow-the-remote-bran-3cb46d",
        )
        .into());
    }
    let mut source = None;
    for row in refs.lines() {
        budget.take()?;
        let Some((oid, branch)) = row.split_once('\0') else {
            continue;
        };
        let mut all = true;
        for needed in &node.required {
            if !ancestor(runner, budget, &node.repo, needed, oid)? {
                all = false;
                break;
            }
        }
        if all {
            source = Some((oid.to_owned(), branch.to_owned()));
            break;
        }
    }
    if node.required.len() == 1 {
        node.source = node
            .required
            .iter()
            .next()
            .cloned()
            .ok_or(crate::i18n::text("backend-no-required-commits-361500"))?;
        text(
            &node.repo,
            &["cat-file", "-e", &format!("{}^{{commit}}", node.source)],
        )?;
        if node.target.is_empty() {
            node.target = source
                .as_ref()
                .map(|(_, b)| b.clone())
                .unwrap_or_else(|| format!("gitcraft-dependency-{}", &node.source[..12]));
        }
    } else if let Some((oid, branch)) = source {
        node.source = oid;
        if node.target.is_empty() {
            node.target = branch;
        }
    } else if !node.published {
        return Err(crate::i18n::text(
            "backend-no-local-commit-contains-all-required-gitlinks-open-this-submodul-006459",
        )
        .into());
    }
    node.snapshot = Some(Snapshot::capture(&node.repo)?);
    Ok(())
}
