//! Resolve exact local source, configured endpoints, destination and fetched lease.
use super::*;
mod input;
mod lease;

pub(super) struct Route {
    pub(super) initial_config: String,
    pub(super) config_before_push: String,
    pub(super) checkout: String,
    pub(super) checkout_oid: String,
    pub(super) local_refs: BTreeMap<String, String>,
    pub(super) remote: String,
    pub(super) source: String,
    pub(super) destination: String,
    pub(super) head: String,
    pub(super) push_oid: String,
    pub(super) base: Option<String>,
    pub(super) tracking_ref: Option<String>,
    pub(super) urls: Vec<String>,
    pub(super) force: bool,
    pub(super) tracking: bool,
    pub(super) extras: Vec<String>,
    pub(super) deleting: bool,
    pub(super) skip_hooks: bool,
}

pub(super) fn resolve(
    repo: &Path,
    original: &[Vec<String>],
    active: &dyn Fn() -> bool,
) -> Result<Route, String> {
    let input::Input {
        initial_config,
        config_before_push,
        checkout,
        checkout_oid,
        local_refs,
        skip_hooks,
        tracking,
        extras,
        mut force,
        remote,
        spec,
        implicit_route,
        delete_option,
        words,
    } = input::capture(repo, original, active)?;
    // Explicit refs (notably release tags) are independent of the checkout. A
    // detached HEAD is still pinned below and checked again before transmission.
    let current = checkout.strip_prefix("refs/heads/").unwrap_or("");
    let remote = match remote {
        Some(r) => r,
        None => {
            if current.is_empty() {
                return Err(
                    "Choose an explicit remote and source ref when HEAD is detached.".into(),
                );
            }
            let mut selected = String::new();
            for key in [
                format!("branch.{current}.pushRemote"),
                "remote.pushDefault".into(),
                format!("branch.{current}.remote"),
            ] {
                selected = config(repo, &key, active)?;
                if !selected.is_empty() {
                    break;
                }
            }
            if selected.is_empty() {
                let remotes = read(repo, &["remote"], active)?;
                selected = if remotes.lines().any(|r| r == "origin") {
                    "origin".into()
                } else if remotes.lines().count() == 1 {
                    remotes.trim().into()
                } else {
                    return Err("Choose a remote before pushing this branch.".into());
                };
            }
            selected
        }
    };
    let remotes = read(repo, &["remote"], active)?;
    if !remotes.lines().any(|r| r == remote) || remote.starts_with('-') {
        return Err("The push remote is no longer configured; reopen the preview.".into());
    }
    if config(repo, &format!("remote.{remote}.mirror"), active)?.eq_ignore_ascii_case("true") {
        return Err("Mirror pushes need their own multi-ref review; no push was run.".into());
    }
    let urls: Vec<String> = read(
        repo,
        &["remote", "get-url", "--push", "--all", &remote],
        active,
    )?
    .lines()
    .map(str::to_owned)
    .collect();
    if urls.len() > 8 {
        return Err("Preview at most eight configured push addresses; split larger mirror groups into separate remotes.".into());
    }
    if urls.is_empty()
        || urls.iter().any(|u| {
            u.chars().any(char::is_control)
                || if let Some((scheme, _)) = u.split_once("://") {
                    !["https", "http", "ssh", "git", "file"].contains(&scheme)
                } else {
                    u.contains("::")
                }
        })
    {
        return Err("The remote has no supported push address.".into());
    }
    let default_source = format!("refs/heads/{current}");
    let saved_branch = if implicit_route && !current.is_empty() {
        config(
            repo,
            &format!("branch.{current}.gitcraftPushBranch"),
            active,
        )?
    } else {
        String::new()
    };
    let spec = spec.or_else(|| saved_branch_spec(current, &saved_branch, implicit_route));
    let spec = match spec {
        Some(s) => s,
        None => {
            if current.is_empty() {
                return Err("Choose an explicit source ref when HEAD is detached.".into());
            }
            let configured = optional(
                repo,
                &["config", "--get-all", &format!("remote.{remote}.push")],
                active,
            )?;
            if configured.lines().count() > 1 {
                return Err("The remote pushes multiple refs. Choose one explicit branch in its menu for preflight.".into());
            }
            if !configured.trim().is_empty() {
                configured.trim().into()
            } else {
                let mode = config(repo, "push.default", active)?;
                let upstream = config(repo, &format!("branch.{current}.merge"), active)?;
                match mode.as_str() {
                    "nothing" => return Err("push.default=nothing: choose an explicit branch push.".into()),
                    "matching" => return Err("Matching pushes need a separate preview for each branch.".into()),
                    "upstream" | "tracking" if !upstream.is_empty() => format!("{default_source}:{upstream}"),
                    "" | "simple" if !upstream.is_empty() && upstream != default_source => return Err("The upstream branch has a different name; choose its explicit branch push from the menu.".into()),
                    _ => default_source.clone(),
                }
            }
        }
    };
    let spec = if delete_option {
        format!(":{spec}")
    } else {
        spec
    };
    let spec = if let Some(s) = spec.strip_prefix('+') {
        force = true;
        s
    } else {
        &spec
    };
    let (source, destination) = spec.split_once(':').unwrap_or((spec, spec));
    let deleting = source.is_empty();
    if source.contains('*') || destination.contains('*') || destination.is_empty() {
        return Err("Choose one complete ref; wildcard pushes are not sent.".into());
    }
    if deleting && tracking {
        return Err("A deletion cannot set an upstream.".into());
    }
    let source = if deleting {
        String::new()
    } else if source == "HEAD" {
        checkout.clone()
    } else if matches!(source.len(), 40 | 64) && source.bytes().all(|b| b.is_ascii_hexdigit()) {
        let refs = read(
            repo,
            &[
                "for-each-ref",
                "--format=%(refname)",
                &format!("--points-at={source}"),
                "refs/heads",
            ],
            active,
        )?;
        refs.lines()
            .find(|r| *r == checkout)
            .or_else(|| refs.lines().next())
            .unwrap_or(source)
            .to_owned()
    } else if source.starts_with("refs/") {
        source.to_owned()
    } else {
        format!("refs/heads/{source}")
    };
    if !deleting && !valid_object_id(&source) {
        valid_ref(repo, &source, active)?;
    }
    let destination = if !deleting && destination == spec && !destination.starts_with("refs/") {
        source.clone()
    } else {
        destination.to_owned()
    };
    valid_ref(repo, &destination, active)?;
    let head = if deleting {
        checkout_oid.clone()
    } else {
        read(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{source}^{{commit}}"),
            ],
            active,
        )?
        .trim()
        .to_owned()
    };
    let push_oid = if deleting {
        String::new()
    } else {
        read(
            repo,
            &["rev-parse", "--verify", "--end-of-options", &source],
            active,
        )?
        .trim()
        .to_owned()
    };
    let (mut base, tracking_ref) =
        lease::resolve(repo, &remote, &destination, &words, deleting, active)?;
    if destination.starts_with("refs/for/")
        && let Some(check) = original
            .iter()
            .find(|o| o.first().is_some_and(|x| x == crate::gerrit::CHECK))
    {
        base = check.get(1).cloned();
    }
    Ok(Route {
        initial_config,
        config_before_push,
        checkout,
        checkout_oid,
        local_refs,
        remote,
        source,
        destination,
        head,
        push_oid,
        base,
        tracking_ref,
        urls,
        force,
        tracking,
        extras,
        deleting,
        skip_hooks,
    })
}
