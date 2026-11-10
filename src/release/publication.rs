//! Execute release steps with destination checks and recoverable file writes.
use super::*;

/// What the local steps left, for undoing them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Done {
    /// The version commit made.
    pub bump_commit: Option<String>,
    /// The tag object made (not the commit it points to).
    pub tag_object: Option<String>,
    /// The commit the tag points to.
    pub tag_commit: Option<String>,
    /// The URL of the release on the forge.
    pub url: Option<String>,
}

/// Runs `step` of `plan` in `repo`, with what the steps before left in `done`.
pub(crate) fn run_step(
    repo: &Path,
    plan: &Plan,
    step: Step,
    done: &mut Done,
    creds: Option<&git::Credentials>,
    hosting: Option<&Hosting>,
) -> Result<(), String> {
    if matches!(step, Step::Bump | Step::Tag | Step::Push) {
        preflight(repo, plan, hosting)?;
    }
    match step {
        Step::Bump => {
            let bump = plan.bump.as_ref().ok_or("nothing to move")?;
            done.bump_commit = Some(commit_bump(repo, plan, bump)?);
        }
        Step::Tag => {
            let target = done
                .bump_commit
                .clone()
                .unwrap_or_else(|| plan.commit.clone());
            let kind = if plan.sign { "-s" } else { "-a" };
            let message = plan.tag_message();
            // Unticked, not signed even where tag.gpgSign asks for every tag to be.
            let sign = if plan.sign {
                "tag.gpgSign=true"
            } else {
                "tag.gpgSign=false"
            };
            git::run(
                Some(repo),
                &[
                    "-c",
                    sign,
                    "tag",
                    kind,
                    "--cleanup=whitespace",
                    "-m",
                    &message,
                    "--",
                    &plan.tag,
                    &target,
                ],
            )?;
            let tag_ref = format!("refs/tags/{}", plan.tag);
            // The tag object itself: putting it back on redo keeps the notes and signature.
            done.tag_object = Some(
                git::run(Some(repo), &["rev-parse", "--verify", &tag_ref])?
                    .trim()
                    .to_owned(),
            );
            done.tag_commit = Some(git::resolve(repo, &tag_ref)?);
        }
        Step::Push => {
            let remote = plan.push.as_deref().ok_or("no remote to push to")?;
            let mut args = vec![
                "push".to_owned(),
                "--atomic".to_owned(),
                "--".to_owned(),
                remote.to_owned(),
            ];
            // The version commit itself (not what the branch has become since), to the branch
            // it goes to there.
            if let (Some(bump), Some(commit)) = (&plan.bump, &done.bump_commit) {
                args.push(format!("{commit}:{}", bump.remote_ref));
            }
            args.push(format!("refs/tags/{0}:refs/tags/{0}", plan.tag));
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            git::run_with(Some(repo), &args, creds)?;
        }
        Step::Publish => {
            let hosting = hosting.ok_or("The repository isn't on a forge gitcraft knows")?;
            check_publication_destination(repo, plan, hosting)?;
            let url = hosting.forge.provider().create_release(
                repo,
                hosting,
                &crate::forge::NewRelease {
                    tag: &plan.tag,
                    title: &plan.tag,
                    notes: &plan.notes,
                    prerelease: plan.prerelease,
                },
            )?;
            done.url = Some(url);
        }
        Step::Upload => {
            let hosting = hosting.ok_or("The repository isn't on a forge gitcraft knows")?;
            check_publication_destination(repo, plan, hosting)?;
            hosting
                .forge
                .provider()
                .upload_assets(repo, hosting, &plan.tag, &plan.assets)?;
        }
    }
    Ok(())
}

/// Validate a publishing plan before executing it, and again immediately before each
/// local mutation and push. Resuming preserves the completed steps even when this refuses.
pub(crate) fn preflight(repo: &Path, plan: &Plan, hosting: Option<&Hosting>) -> Result<(), String> {
    if !plan.publish {
        return Ok(());
    }
    let hosting = hosting.ok_or("The repository isn't on a forge gitcraft knows")?;
    check_publication_destination(repo, plan, hosting)
}

/// A push URL, URL rewrite, or a remote edited during the release must not receive a tag
/// intended for a different publishing repository. Also checked before forge mutations,
/// including resumed uploads; Git's configured transport routing itself is never rewritten.
pub(super) fn check_publication_destination(
    repo: &Path,
    plan: &Plan,
    hosting: &Hosting,
) -> Result<(), String> {
    let remote = plan
        .push
        .as_deref()
        .ok_or("Publication needs the release's push destination")?;
    let urls = git::run(
        Some(repo),
        &["remote", "get-url", "--push", "--all", "--", remote],
    )?;
    let expected = format!("{}/{}", hosting.web.trim_end_matches('/'), hosting.path);
    let urls = urls
        .lines()
        .filter(|url| !url.trim().is_empty())
        .collect::<Vec<_>>();
    if urls.is_empty()
        || !urls
            .iter()
            .all(|url| same_publication_repository(url, hosting))
    {
        return Err(format!(
            "Release publication stopped: the push destination of {remote} does not match the selected forge repository {expected}. Local release changes remain undoable."
        ));
    }
    Ok(())
}

/// Compare the web authority separately from the repository path. DNS names are case
/// insensitive; GitLab's captured subgroup/project spelling must remain exact. GitHub,
/// Forgejo and Gitea identify owner/repository names without regard to ASCII case.
pub(super) fn same_publication_repository(url: &str, hosting: &Hosting) -> bool {
    let Some((host, path)) = crate::forge::location(url) else {
        return false;
    };
    if !host.eq_ignore_ascii_case(&hosting.host) {
        return false;
    }
    // SSH identifies the Git repository independently of the forge's configured
    // web port or reverse-proxy prefix. HTTP transports must also match the web
    // authority and scheme; a different port can serve another repository.
    let web_transport = url.split_once("://").is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    });
    if web_transport {
        return hosting.matches_http_repository(url);
    }
    hosting.same_repository_path(&path)
}

/// Writes the new version (and the notes in the changelog) and commits those files only.
pub(super) fn commit_bump(repo: &Path, plan: &Plan, bump: &Bump) -> Result<String, String> {
    let checked_out = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    if checked_out.trim() != format!("refs/heads/{}", bump.branch) || git::head(repo) != plan.commit
    {
        return Err(format!(
            "{} isn't checked out at {} any more: plan the release again",
            bump.branch,
            crate::repo::short(&plan.commit)
        ));
    }
    // Changes made to those files since the dialog opened are never written over.
    let touched = bump_files(repo, bump);
    let mut args = vec!["status", "--porcelain", "--"];
    args.extend(touched.iter().map(String::as_str));
    if !git::run(Some(repo), &args)?.trim().is_empty() {
        return Err(format!(
            "{} changed since the release was planned: commit or stash that first",
            touched.join(", ")
        ));
    }
    let written = write_bump(repo, plan, bump);
    let files = match written {
        Ok(files) => files,
        Err(e) => {
            restore(repo, &touched);
            return Err(e);
        }
    };
    let message = format!("chore(release): {}", plan.tag);
    let mut args = vec!["commit", "-q", "-m", &message, "--"];
    args.extend(files.iter().map(String::as_str));
    if let Err(e) = git::run(Some(repo), &args) {
        // A hook refused it: the files go back as they were.
        restore(repo, &files);
        return Err(e);
    }
    Ok(git::head(repo))
}

/// The files the version commit may change (those of them at HEAD).
pub(super) fn bump_files(repo: &Path, bump: &Bump) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for (kind, _) in &bump.manifests {
        let (manifest, lock) = kind.files();
        files.push(manifest.to_owned());
        if git::file_at(repo, Some("HEAD"), lock).is_ok() {
            files.push(lock.to_owned());
        }
    }
    if bump.changelog {
        files.push(CHANGELOG.to_owned());
    }
    files
}

/// Puts `files` back as they are at HEAD.
pub(super) fn restore(repo: &Path, files: &[String]) {
    let mut args = vec!["checkout", "HEAD", "--"];
    args.extend(files.iter().map(String::as_str));
    let _ = git::run(Some(repo), &args);
}

/// Writes the new version and notes; the files written.
pub(super) fn write_bump(repo: &Path, plan: &Plan, bump: &Bump) -> Result<Vec<String>, String> {
    let mut files: Vec<String> = Vec::new();
    for (kind, old) in &bump.manifests {
        let (manifest, lock) = kind.files();
        let text = read_at_head(repo, manifest)?;
        let new = kind
            .bump(&text, &bump.version)
            .ok_or_else(|| format!("Couldn't find the version in {manifest}"))?;
        write(repo, manifest, &new)?;
        files.push(manifest.to_owned());
        let names: Vec<String> = kind.own_name(&text).into_iter().collect();
        if let Ok(text) = read_at_head(repo, lock)
            && let Some(new) = kind.bump_lock(&text, &names, old, &bump.version)
            && new != text
        {
            write(repo, lock, &new)?;
            files.push(lock.to_owned());
        }
    }
    if bump.changelog {
        let text = read_at_head(repo, CHANGELOG)?;
        write(
            repo,
            CHANGELOG,
            &with_entry(&text, &plan.tag, &plan.notes, &today()),
        )?;
        files.push(CHANGELOG.to_owned());
    }
    Ok(files)
}

pub(super) fn read_at_head(repo: &Path, file: &str) -> Result<String, String> {
    git::file_at(repo, Some("HEAD"), file).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Writes `file` of the working tree, on whichever machine the repository is.
pub(super) fn write(repo: &Path, file: &str, text: &str) -> Result<(), String> {
    crate::remote::write(&crate::remote::join(repo, file), text.as_bytes())
        .map_err(|e| format!("Couldn't write {file}: {e}"))
}
