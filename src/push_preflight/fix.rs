mod transaction;

use super::*;
use std::collections::HashMap;
use std::path::PathBuf;

struct Files {
    repo: PathBuf,
    local: Option<PathBuf>,
}
impl Files {
    fn new(repo: &Path) -> Result<Self, String> {
        if crate::remote::is_remote(repo) {
            return Ok(Self {
                repo: repo.into(),
                local: None,
            });
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "No clock")?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("gitcraft-push-fix-{}-{stamp}", std::process::id()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&path)
            .map_err(|_| "Could not prepare the history fix")?;
        Ok(Self {
            repo: repo.into(),
            local: Some(path),
        })
    }
    fn write(&self, name: &str, text: &str) -> Result<String, String> {
        if let Some(target) = crate::remote::parse(&self.repo) {
            return crate::remote::host_temp_file(&target, name, text.as_bytes())
                .map_err(|_| "Could not prepare the remote history fix".into());
        }
        let path = self.local.as_ref().ok_or("No fix folder")?.join(name);
        std::fs::write(&path, text).map_err(|_| "Could not prepare the history fix")?;
        Ok(path.display().to_string())
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        if let Some(path) = &self.local {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

pub(crate) fn fix(repo: &Path, plan: &Plan, kind: &str) -> Result<String, String> {
    // INTERIM fail-closed limitation: no remote repair request may survive its caller's
    // timeout. Read-only preview and reviewed pushes use their existing paths.
    if crate::remote::is_remote(repo) {
        return Err("Automatic remote history repair is temporarily unavailable: this transport cannot prove owned completion. Preview and reviewed push remain available; perform an explicitly supervised repair on the repository host instead.".into());
    }
    check(repo, plan)?;
    if plan.branch != plan.checkout || !plan.branch.starts_with("refs/heads/") {
        return Err(
            "Check out this branch before fixing its history, then reopen preflight.".into(),
        );
    }
    if !read(
        repo,
        &["status", "--porcelain=v1", "--untracked-files=all"],
        &|| true,
    )?
    .trim()
    .is_empty()
    {
        return Err("Commit or stash all working changes and untracked files before rewriting outgoing history.".into());
    }
    if kind == "remote" {
        let base = plan.base.as_deref().ok_or("Fetch the target first")?;
        return git::run(Some(repo),&["merge","--no-edit","--no-autostash",base]).map(|_|"Merged the fetched target; reopen preflight. Ctrl+Z undoes this merge.".into()).map_err(|_|"The merge needs attention. Resolve conflicts or abort it before reopening preflight.".into());
    }
    if !["author", "sign", "secrets", "lfs"].contains(&kind) {
        return Err("Unknown history fix".into());
    }
    if kind == "lfs" && plan.findings.iter().any(|f| f.kind == Kind::Secret) {
        return Err("Resolve likely-secret findings before moving files to LFS; LFS must not conceal a known secret.".into());
    }
    let files = Files::new(repo)?;
    let index = format!("{}.index", files.write("index-name", "")?);
    let env = vec![("GIT_INDEX_FILE", index)];
    let run = |args: &[&str], extra: &[(&'static str, String)]| -> Result<String, String> {
        git::forecast_run(repo,args,extra,&[0],&||true).map_err(|_|"History rewrite failed before the branch was updated; check signing availability or missing local objects.".into())
    };
    let remove: BTreeSet<_> = plan
        .findings
        .iter()
        .filter(|f| f.kind == Kind::Secret && !f.path.is_empty())
        .map(|f| f.path.clone())
        .collect();
    let secret_messages: BTreeSet<_> = plan
        .findings
        .iter()
        .filter(|f| f.kind == Kind::Secret && f.path.is_empty())
        .map(|f| f.commit.clone())
        .collect();
    let lfs_paths: BTreeSet<_> = plan
        .findings
        .iter()
        .filter(|f| f.kind == Kind::Large)
        .map(|f| f.path.clone())
        .collect();
    let wrong_authors: BTreeSet<_> = plan
        .findings
        .iter()
        .filter(|f| f.kind == Kind::Identity)
        .map(|f| f.commit.clone())
        .collect();
    let mut rewritten: HashMap<String, String> = HashMap::new();
    for (i, commit) in plan.commits.iter().enumerate() {
        let raw = read(repo, &["cat-file", "commit", &commit.oid], &|| true)?;
        let (headers, message) = raw.split_once("\n\n").ok_or("Malformed commit")?;
        let mut tree = headers
            .lines()
            .find_map(|l| l.strip_prefix("tree "))
            .ok_or("No commit tree")?
            .to_owned();
        if kind == "secrets" && !remove.is_empty() {
            run(&["read-tree", &tree], &env)?;
            for path in &remove {
                run(
                    &[
                        "--literal-pathspecs",
                        "update-index",
                        "--force-remove",
                        "--",
                        path,
                    ],
                    &env,
                )?;
            }
            tree = run(&["write-tree"], &env)?.trim().to_owned();
        }
        if kind == "lfs" {
            run(&["read-tree", &tree], &env)?;
            let entries = read(repo, &["ls-tree", "-r", "-z", &tree], &|| true)?;
            for (j, item) in entries.split('\0').filter(|i| !i.is_empty()).enumerate() {
                let Some((meta, path)) = item.split_once('\t') else {
                    continue;
                };
                if path == ".gitattributes"
                    && !meta.starts_with("100644 ")
                    && !meta.starts_with("100755 ")
                {
                    return Err(".gitattributes is not a regular file; resolve that before converting to LFS.".into());
                }
                if !lfs_paths.contains(path) {
                    continue;
                }
                let parts: Vec<_> = meta.split_whitespace().collect();
                if parts.len() != 3 || !matches!(parts[0], "100644" | "100755") {
                    return Err(
                        "LFS conversion supports regular files only; no history was changed."
                            .into(),
                    );
                }
                let command = format!(
                    "!git --no-lazy-fetch cat-file blob {} | git lfs clean -- {}",
                    parts[2],
                    crate::remote::sh_quote(path)
                );
                let pointer = run(
                    &[
                        "-c",
                        &format!("alias.gitcraft-preflight-lfs={command}"),
                        "gitcraft-preflight-lfs",
                    ],
                    &[],
                )?;
                if git::parse_lfs_pointer(&pointer).is_none() {
                    return Err("git-lfs did not produce a pointer; install git-lfs before converting history.".into());
                }
                let file = files.write(&format!("pointer-{i}-{j}"), &pointer)?;
                let oid = run(&["hash-object", "-w", "--no-filters", "--", &file], &[])?;
                run(
                    &[
                        "update-index",
                        "--add",
                        "--cacheinfo",
                        parts[0],
                        oid.trim(),
                        path,
                    ],
                    &env,
                )?;
            }
            let attributes = git::forecast_run(
                repo,
                &["show", &format!("{tree}:.gitattributes")],
                &[("GIT_NO_LAZY_FETCH", "1".into())],
                &[0, 128],
                &|| true,
            )
            .map_err(|_| "Could not read attributes")?;
            let mut attributes = attributes;
            if !attributes.ends_with('\n') && !attributes.is_empty() {
                attributes.push('\n');
            }
            for path in &lfs_paths {
                let mut pattern = String::from("/");
                for c in path.chars() {
                    if matches!(c, '*' | '?' | '[' | ']' | '\\') {
                        pattern.push('\\');
                    }
                    pattern.push(c);
                }
                attributes.push_str(&format!(
                    "{} filter=lfs diff=lfs merge=lfs -text\n",
                    serde_json::to_string(&pattern).map_err(|_| "Could not quote attributes")?
                ));
            }
            let file = files.write(&format!("attributes-{i}"), &attributes)?;
            let oid = run(&["hash-object", "-w", "--no-filters", "--", &file], &[])?;
            run(
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    "100644",
                    oid.trim(),
                    ".gitattributes",
                ],
                &env,
            )?;
            tree = run(&["write-tree"], &env)?.trim().to_owned();
        }
        let author = read(
            repo,
            &[
                "show",
                "--no-show-signature",
                "-s",
                "--format=%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI",
                &commit.oid,
            ],
            &|| true,
        )?;
        let fields: Vec<_> = author.trim_end().split('\0').collect();
        if fields.len() != 6 {
            return Err("Malformed author metadata".into());
        }
        let change_author = kind == "author" && wrong_authors.contains(&commit.oid);
        if change_author
            && (plan.expected_name.trim().is_empty() || plan.expected_email.trim().is_empty())
        {
            return Err("Set the expected host author name and email in Profile settings before amending history.".into());
        }
        let metadata = vec![
            (
                "GIT_AUTHOR_NAME",
                if change_author {
                    plan.expected_name.clone()
                } else {
                    fields[0].into()
                },
            ),
            (
                "GIT_AUTHOR_EMAIL",
                if change_author {
                    plan.expected_email.clone()
                } else {
                    fields[1].into()
                },
            ),
            ("GIT_AUTHOR_DATE", fields[2].into()),
            ("GIT_COMMITTER_NAME", fields[3].into()),
            ("GIT_COMMITTER_EMAIL", fields[4].into()),
            ("GIT_COMMITTER_DATE", fields[5].into()),
        ];
        let text = if kind == "secrets" && secret_messages.contains(&commit.oid) {
            "Commit message removed by push preflight because it contained a likely secret.\n"
        } else {
            message
        };
        let file = files.write(&format!("message-{i}"), text)?;
        let mut args = vec!["commit-tree".into(), tree, "-F".into(), file];
        args.push(
            if kind == "sign" || plan.profile.own_signing && plan.profile.signing.sign_commits {
                "-S".into()
            } else {
                "--no-gpg-sign".into()
            },
        );
        for parent in &commit.parents {
            args.extend(["-p".into(), rewritten.get(parent).unwrap_or(parent).clone()]);
        }
        let words: Vec<_> = args.iter().map(String::as_str).collect();
        rewritten.insert(
            commit.oid.clone(),
            run(&words, &metadata)?.trim().to_owned(),
        );
    }
    let new = rewritten
        .get(&plan.head)
        .ok_or("There are no outgoing commits to fix")?;
    check(repo, plan)?;
    // Metadata-only repairs also need HEAD ownership so the existing undo journal
    // cannot bind a different checkout after an original reference hook runs.
    transaction::Hook::new(repo, &plan.branch, &plan.head, new)?.execute(repo)?;
    Ok("Outgoing history fixed. Reopen preflight to verify it; Ctrl+Z restores the previous history.".into())
}
