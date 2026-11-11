//! Resolve fetched mappings against the action's immutable lease requirement.
use super::super::*;

pub(super) fn resolve(
    repo: &Path,
    remote: &str,
    destination: &str,
    words: &[String],
    deleting: bool,
    active: &dyn Fn() -> bool,
) -> Result<(Option<String>, Option<String>), String> {
    let mut base = None;
    let mut tracking_ref = None;
    let fetch = optional(
        repo,
        &["config", "--get-all", &format!("remote.{remote}.fetch")],
        active,
    )?;
    for fetch in fetch.lines() {
        if let Some((from, to)) = fetch.trim_start_matches('+').split_once(':') {
            let tracked = if from == destination {
                Some(to.to_owned())
            } else if let (Some((a, b)), Some((c, d))) = (from.split_once('*'), to.split_once('*'))
            {
                destination
                    .strip_prefix(a)
                    .and_then(|v| v.strip_suffix(b))
                    .map(|middle| format!("{c}{middle}{d}"))
            } else {
                None
            };
            if let Some(tracked) = tracked {
                if tracked.starts_with("refs/remotes/") && !tracked.ends_with("/HEAD") {
                    tracking_ref = Some(tracked.clone());
                }
                base = optional(
                    repo,
                    &[
                        "rev-parse",
                        "--verify",
                        "-q",
                        "--end-of-options",
                        &if deleting {
                            tracked.clone()
                        } else {
                            format!("{tracked}^{{commit}}")
                        },
                    ],
                    active,
                )?
                .trim()
                .to_owned()
                .into();
                // Multiple fetch mappings must all corroborate a fixed lease;
                // a later old tracking ref must not hide an earlier refreshed tip.
                base = captured_lease(
                    words,
                    destination,
                    base.filter(|tip| !tip.is_empty()),
                    deleting,
                )?;
            }
        }
    }
    base = base.filter(|b| !b.is_empty());
    if deleting && destination.starts_with("refs/tags/") && base.is_none() {
        base = optional(
            repo,
            &[
                "rev-parse",
                "--verify",
                "-q",
                "--end-of-options",
                destination,
            ],
            active,
        )?
        .trim()
        .to_owned()
        .into();
        base = base.filter(|oid| !oid.is_empty());
    }
    base = captured_lease(words, destination, base, deleting)?;
    if deleting && base.is_none() {
        return Err(
            "Fetch this exact target before deleting it; no remote tip was inspected.".into(),
        );
    }
    Ok((base, tracking_ref))
}
