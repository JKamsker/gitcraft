//! Collect review findings without retaining matched secret values.
use super::*;

pub(super) fn collect(
    repo: &Path,
    route: &routing::Route,
    ids: &[String],
    profile: &UserProfile,
    expected_email: &str,
    signs: bool,
    active: &dyn Fn() -> bool,
) -> Result<(Vec<Commit>, Vec<Finding>), String> {
    let routing::Route {
        deleting,
        skip_hooks,
        source,
        destination,
        push_oid,
        head,
        urls,
        ..
    } = route;
    let (deleting, skip_hooks) = (*deleting, *skip_hooks);
    let mut commits = Vec::new();
    let mut findings = Vec::new();
    if !deleting {
        let names = read(
            repo,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)",
                "refs/heads/entire/checkpoints/",
                "refs/entire/checkpoints/",
                "refs/gitcraft/agent/",
            ],
            active,
        )?;
        if names.len() > 1024 * 1024 {
            return Err(
                "Checkpoint reference list exceeds 1 MiB; narrow publication before pushing".into(),
            );
        }
        if let Some(detail) = crate::git::prompt_session::entire_automatic_publication(
            repo, &names, skip_hooks, active,
        )? {
            findings.push(Finding {
                kind: Kind::Transcript,
                commit: push_oid.clone(),
                path: String::new(),
                detail: detail.into(),
            });
        }
        if crate::git::prompt_session::checkpoint_publication(
            source,
            destination,
            push_oid,
            head,
            &names,
        ) {
            findings.push(Finding { kind: Kind::Transcript, commit: push_oid.clone(), path: String::new(), detail: "Checkpoint publication may disclose prompts, local paths, assistant summaries and tool activity. This exact object or route contains agent checkpoint history. Review its contents and recipients explicitly before sending; local prompt/session viewing itself never publishes transcripts.".into() });
        }
    }

    if !deleting && push_oid != head {
        let tag = read(repo, &["cat-file", "tag", push_oid], active)?;
        if secret_lines(tag.split_once("\n\n").map_or("", |x| x.1))
            .next()
            .is_some()
        {
            findings.push(Finding{kind:Kind::Secret,commit:push_oid.clone(),path:String::new(),detail:"Likely secret in annotated tag message; value withheld. Recreate the tag before pushing.".into()});
        }
    }
    let mut metadata_bytes = 0usize;
    for oid in ids {
        let metadata = read(
            repo,
            &[
                "show",
                "--no-show-signature",
                "-s",
                "--format=%P%x00%an%x00%ae%x00%s",
                oid,
            ],
            active,
        )?;
        metadata_bytes += metadata.len();
        if metadata.len() > 16 * 1024 || metadata_bytes > 4 * 1024 * 1024 {
            return Err("Outgoing commit metadata exceeds the bounded preview size; shorten large messages or split the push.".into());
        }
        let f: Vec<_> = metadata.trim_end().split('\0').collect();
        if f.len() != 4 {
            return Err("Commit metadata could not be read safely.".into());
        }
        let raw = read(repo, &["cat-file", "commit", oid], active)?;
        let signed = raw
            .split_once("\n\n")
            .map_or(raw.as_str(), |x| x.0)
            .lines()
            .any(|l| l.starts_with("gpgsig ") || l.starts_with("gpgsig-sha256 "));
        let commit = Commit {
            oid: oid.clone(),
            parents: f[0].split_whitespace().map(str::to_owned).collect(),
            name: f[1].into(),
            email: f[2].into(),
            subject: f[3].into(),
            signed,
        };
        for url in urls {
            let (_, email) = profile.expected_author(url);
            let email = if email.is_empty() {
                expected_email
            } else {
                email
            };
            if !email.is_empty() && !commit.email.eq_ignore_ascii_case(email) {
                findings.push(Finding {
                    kind: Kind::Identity,
                    commit: oid.clone(),
                    path: String::new(),
                    detail: format!(
                        "Author email differs from the profile policy for {}.",
                        target_label(url)
                    ),
                });
            }
        }
        if signs && !signed {
            findings.push(Finding {
                kind: Kind::Signature,
                commit: oid.clone(),
                path: String::new(),
                detail: "Commit has no embedded signature; this profile signs commits.".into(),
            });
        }
        let lower = commit.subject.trim().to_ascii_lowercase();
        if ["fixup!", "squash!", "wip", "[wip]"]
            .iter()
            .any(|p| lower.starts_with(p))
        {
            findings.push(Finding {
                kind: Kind::Message,
                commit: oid.clone(),
                path: String::new(),
                detail: "Temporary WIP/fixup message remains in outgoing history.".into(),
            });
        }
        if secret_lines(raw.split_once("\n\n").map_or("", |x| x.1))
            .next()
            .is_some()
        {
            findings.push(Finding {
                kind: Kind::Secret,
                commit: oid.clone(),
                path: String::new(),
                detail: "Likely secret in commit message (value withheld).".into(),
            });
        }
        commits.push(commit);
        if findings.len() > MAX_FINDINGS {
            return Err("More than 2,000 findings need review. Repair or split this history before pushing.".into());
        }
    }
    Ok((commits, findings))
}

pub(super) fn remote_history(
    repo: &Path,
    deleting: bool,
    base: &Option<String>,
    head: &str,
    expected_email: &str,
    findings: &mut Vec<Finding>,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    if !deleting && let Some(base) = base {
        let lost = read(
            repo,
            &[
                "log",
                "--no-show-signature",
                "--format=%H%x00%ae",
                base,
                "--not",
                head,
            ],
            active,
        )?;
        let mut count = 0;
        let mut others = 0;
        for line in lost.lines() {
            if let Some((_, email)) = line.split_once('\0') {
                count += 1;
                if !email.eq_ignore_ascii_case(expected_email) {
                    others += 1;
                }
            }
        }
        if count > 0 {
            findings.push(Finding{kind:Kind::RemoteHistory,commit:base.clone(),path:String::new(),detail:format!("Force push would drop {count} remote commits, {others} by other authors. Based on the fetched target; the lease must still match.")});
        }
    }
    Ok(())
}
