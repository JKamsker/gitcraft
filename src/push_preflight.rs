//! Explicit push previews. Reads are local, bounded and never ask a credential helper.
//! Findings contain object IDs, paths and line numbers, never matched secret values.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::git;
use crate::settings::profiles::UserProfile;

pub(crate) const APPROVED: &str = "\0push-preflight";
pub(crate) const FIX: &str = "\0push-preflight-fix";
mod fix;
pub(crate) use fix::fix;
#[cfg(test)]
pub(crate) mod tests;
const MAX_COMMITS: usize = 10_000;
const MAX_OBJECTS: usize = 50_000;
const MAX_FINDINGS: usize = 2_000;
const LARGE: u64 = 5 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Identity,
    Signature,
    Message,
    Secret,
    Large,
    RemoteHistory,
}

#[derive(Clone, Debug)]
pub(crate) struct Finding {
    pub kind: Kind,
    pub commit: String,
    pub path: String,
    pub detail: String,
}

#[derive(Clone)]
pub(crate) struct Commit {
    pub oid: String,
    pub parents: Vec<String>,
    pub name: String,
    pub email: String,
    pub subject: String,
    pub signed: bool,
}

#[derive(Clone)]
pub(crate) struct Plan {
    pub branch: String,
    pub remote: String,
    pub destination: String,
    pub head: String,
    pub base: Option<String>,
    pub commits: Vec<Commit>,
    pub findings: Vec<Finding>,
    pub targets: Vec<String>,
    pub expected_name: String,
    pub expected_email: String,
    pub profile: UserProfile,
    pub force: bool,
    pub deleting: bool,
    pub tracking: bool,
    pub lfs_objects: BTreeSet<String>,
    push_oid: String,
    original: Vec<Vec<String>>,
    urls: Vec<String>,
    checkout: String,
    checkout_oid: String,
    config: String,
    config_before_push: String,
    local_refs: BTreeMap<String, String>,
    extras: Vec<String>,
    tracking_ref: Option<String>,
    lfs_config: Vec<(String, String)>,
    receive_pack: String,
}

fn read(repo: &Path, args: &[&str], active: &dyn Fn() -> bool) -> Result<String, String> {
    let args: Vec<&str> = ["--no-lazy-fetch"]
        .into_iter()
        .chain(args.iter().copied())
        .collect();
    git::forecast_run(repo, &args, &[("GIT_NO_LAZY_FETCH", "1".into())], &[0], active)
        .map_err(|_| "The local push preview could not be completed. Fetch missing history explicitly, then try again; no push was run.".into())
}

fn optional(repo: &Path, args: &[&str], active: &dyn Fn() -> bool) -> Result<String, String> {
    let args: Vec<&str> = ["--no-lazy-fetch"]
        .into_iter()
        .chain(args.iter().copied())
        .collect();
    git::forecast_run(
        repo,
        &args,
        &[("GIT_NO_LAZY_FETCH", "1".into())],
        &[0, 1],
        active,
    )
    .map_err(|_| "The local push configuration could not be read; no push was run.".into())
}

fn valid_ref(repo: &Path, name: &str, active: &dyn Fn() -> bool) -> Result<(), String> {
    if !name.starts_with("refs/") || read(repo, &["check-ref-format", name], active).is_err() {
        return Err("Choose one complete destination ref, such as refs/heads/main.".into());
    }
    Ok(())
}

fn config(repo: &Path, key: &str, active: &dyn Fn() -> bool) -> Result<String, String> {
    optional(repo, &["config", "--get", key], active).map(|s| s.trim_end().to_owned())
}

fn config_stamp(repo: &Path, active: &dyn Fn() -> bool) -> Result<String, String> {
    // Deliberately excludes credential helpers, HTTP headers and token stores. Signing
    // identifiers/paths are compared, never opened. The private plan is not Debug/Serialize.
    let values = optional(
        repo,
        &[
            "config",
            "--null",
            "--get-regexp",
            r"^(remote\..*\.(url|pushurl|fetch|push|mirror|receivepack)|branch\..*\.(remote|merge|pushremote)|remote\.pushdefault|push\.(default|followtags|recursesubmodules)|url\..*\.(insteadof|pushinsteadof)|user\.(name|email|signingkey)|commit\.gpgsign|gpg\.(format|program)|gpg\..*\.program|core\.(sshcommand|hookspath)|ssh\.variant|lfs\.(url|pushurl|storage)|remote\..*\.(lfsurl|lfspushurl))$",
        ],
        active,
    )?;
    if values.len() > 256 * 1024 {
        return Err("The push routing configuration exceeds the preview's size limit.".into());
    }
    for item in values.split('\0').filter(|s| !s.is_empty()) {
        let (key, value) = item.split_once('\n').unwrap_or((item, ""));
        if key.chars().any(char::is_control) || value.chars().any(char::is_control) {
            return Err("An identity or routing value contains control characters. Correct it before opening a push preview.".into());
        }
    }
    Ok(values)
}

pub(crate) fn has_push(ops: &[Vec<String>]) -> bool {
    ops.iter().any(|op| push_words(op).is_some())
}

fn push_words(op: &[String]) -> Option<&[String]> {
    let mut words = op;
    while matches!(words.first().map(String::as_str), Some("-c")) {
        words = words.get(2..)?;
    }
    (words.first()?.as_str() == "push").then_some(&words[1..])
}

type ParsedPush = (
    Option<String>,
    Option<String>,
    bool,
    bool,
    Vec<String>,
    bool,
);

fn parse(words: &[String]) -> Result<ParsedPush, String> {
    let (mut positional, mut force, mut tracking, mut extras) =
        (Vec::new(), false, false, Vec::new());
    let mut ended = false;
    let mut deleting = false;
    for word in words {
        if !ended && word == "--" {
            ended = true;
            continue;
        }
        if !ended && word.starts_with('-') {
            match word.as_str() {
                "-u" | "--set-upstream" => tracking = true,
                "-f" | "--force" | "--force-with-lease" => force = true,
                "--no-follow-tags" => {},
                "--delete" | "-d" => deleting = true,
                _ if word.starts_with("--force-with-lease=") => force = true,
                _ if word.starts_with("--push-option=") => extras.push(word.clone()),
                _ => return Err("This push option needs an explicit single-ref preview; multi-ref, matching, mirror, atomic and recursive pushes are not sent by this checklist.".into()),
            }
        } else {
            positional.push(word.clone());
        }
    }
    if positional.len() > 2 {
        return Err("Preview one destination at a time; no push was run.".into());
    }
    Ok((
        positional.first().cloned(),
        positional.get(1).cloned(),
        force,
        tracking,
        extras,
        deleting,
    ))
}

fn companion_branch(op: &[String]) -> Option<&str> {
    match op {
        [command, flag, end, name]
            if command == "branch" && matches!(flag.as_str(), "-d" | "-D") && end == "--" =>
        {
            Some(name)
        }
        [command, end, name, oid] if command == "branch" && end == "--" && valid_object_id(oid) => {
            Some(name)
        }
        [command, upstream, end, name]
            if command == "branch" && upstream.starts_with("--set-upstream-to=") && end == "--" =>
        {
            Some(name)
        }
        [command, key, _] if command == "config" => key
            .strip_prefix("branch.")
            .and_then(|key| key.rsplit_once('.'))
            .and_then(|(name, option)| matches!(option, "remote" | "merge").then_some(name)),
        _ => None,
    }
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn scan(
    repo: &Path,
    original: Vec<Vec<String>>,
    profile: UserProfile,
    active: &dyn Fn() -> bool,
) -> Result<Plan, String> {
    let initial_config = config_stamp(repo, active)?;
    for op in &original {
        if push_words(op).is_some() {
            let mut prefix = op.as_slice();
            while let [flag, value, rest @ ..] = prefix {
                if flag != "-c" {
                    break;
                }
                if value != "push.default=simple" {
                    return Err(
                        "Preview explicit refs without temporary push configuration overrides."
                            .into(),
                    );
                }
                prefix = rest;
            }
        } else if !op.first().is_some_and(|v| v.starts_with('\0')) && companion_branch(op).is_none()
        {
            return Err(
                "Complete this local operation first, then preview its push separately.".into(),
            );
        }
    }
    let pushes: Vec<_> = original.iter().filter_map(|o| push_words(o)).collect();
    if pushes.len() != 1 {
        return Err("Preview one push at a time; no push was run.".into());
    }
    let (remote, spec, mut force, tracking, extras, delete_option) = parse(pushes[0])?;
    let mut local_refs = BTreeMap::new();
    for op in &original {
        if let Some(name) = companion_branch(op) {
            let reference = format!("refs/heads/{name}");
            valid_ref(repo, &reference, active)?;
            let oid = optional(
                repo,
                &[
                    "rev-parse",
                    "--verify",
                    "-q",
                    "--end-of-options",
                    &reference,
                ],
                active,
            )?;
            local_refs.insert(reference, oid.trim().to_owned());
        }
    }
    let removed_before_push: Vec<_> = original
        .iter()
        .take_while(|op| push_words(op).is_none())
        .filter_map(|op| match op.as_slice() {
            [command, flag, end, name]
                if command == "branch" && matches!(flag.as_str(), "-d" | "-D") && end == "--" =>
            {
                Some(format!("branch.{name}."))
            }
            _ => None,
        })
        .collect();
    let config_before_push = initial_config
        .split('\0')
        .filter(|item| {
            let key = item.split_once('\n').map_or(*item, |(key, _)| key);
            !removed_before_push
                .iter()
                .any(|prefix| key.starts_with(prefix))
        })
        .filter(|item| !item.is_empty())
        .map(|item| format!("{item}\0"))
        .collect::<String>();
    let checkout = optional(repo, &["symbolic-ref", "-q", "HEAD"], active)?
        .trim()
        .to_owned();
    let checkout_oid = read(repo, &["rev-parse", "--verify", "HEAD"], active)?
        .trim()
        .to_owned();
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
            }
        }
    }
    base = base.filter(|b| !b.is_empty());
    if deleting {
        if destination.starts_with("refs/tags/") && base.is_none() {
            base = optional(
                repo,
                &[
                    "rev-parse",
                    "--verify",
                    "-q",
                    "--end-of-options",
                    &destination,
                ],
                active,
            )?
            .trim()
            .to_owned()
            .into();
            base = base.filter(|oid| !oid.is_empty());
        }
        for lease in pushes[0]
            .iter()
            .filter_map(|arg| arg.strip_prefix("--force-with-lease="))
        {
            let (reference, expected) = lease
                .split_once(':')
                .ok_or("Deletion needs an exact ref lease")?;
            if reference != destination || !valid_object_id(expected) {
                return Err("The deletion lease must name this exact ref and object.".into());
            }
            if base.as_deref().is_some_and(|tip| tip != expected) {
                return Err("The fetched target differs from the deletion lease; reopen its original action.".into());
            }
            base = Some(expected.to_owned());
        }
        if base.is_none() {
            return Err(
                "Fetch this exact target before deleting it; no remote tip was inspected.".into(),
            );
        }
    }
    if destination.starts_with("refs/for/")
        && let Some(check) = original
            .iter()
            .find(|o| o.first().is_some_and(|x| x == crate::gerrit::CHECK))
    {
        base = check.get(1).cloned();
    }
    let mut args = vec![
        "rev-list",
        "--reverse",
        "--topo-order",
        "--max-count=10001",
        head.as_str(),
    ];
    if let Some(base) = &base {
        args.extend(["--not", base]);
    }
    let ids: Vec<String> = if deleting {
        Vec::new()
    } else {
        read(repo, &args, active)?
            .lines()
            .map(str::to_owned)
            .collect()
    };
    if ids.len() > MAX_COMMITS {
        return Err("The outgoing history exceeds 10,000 commits; split the push or fetch its exact target before checking.".into());
    }
    clean_lfs_config(repo, &checkout_oid, active)?;
    let lfs_file = read(
        repo,
        &["ls-tree", "-z", &checkout_oid, "--", ".lfsconfig"],
        active,
    )?;
    if !lfs_file.is_empty() && !lfs_file.starts_with("100644 ") && !lfs_file.starts_with("100755 ")
    {
        return Err(
            "Committed .lfsconfig must be a regular file before its endpoint can be reviewed."
                .into(),
        );
    }
    let receive_pack = config(repo, &format!("remote.{remote}.receivepack"), active)?;
    let mut lfs_config = Vec::new();
    for key in [
        format!("remote.{remote}.lfsurl"),
        format!("remote.{remote}.lfspushurl"),
        "lfs.url".into(),
        "lfs.pushurl".into(),
    ] {
        let mut value = config(repo, &key, active)?;
        if value.is_empty() && !lfs_file.is_empty() && key.starts_with("lfs.") {
            value = optional(
                repo,
                &[
                    "config",
                    "--blob",
                    &format!("{checkout_oid}:.lfsconfig"),
                    "--get",
                    &key,
                ],
                active,
            )?
            .trim_end()
            .to_owned();
        }
        if value.chars().any(char::is_control) {
            return Err("The LFS endpoint contains control characters.".into());
        }
        // Empty higher-priority values freeze the default too: a later working-tree
        // .lfsconfig edit must not redirect an already approved upload.
        lfs_config.push((key, value));
    }
    let first_url = urls.first().ok_or("No push target")?;
    let (expected_name, expected_email) = profile.expected_author(first_url);
    let expected_name = if expected_name.is_empty() {
        config(repo, "user.name", active)?
    } else {
        expected_name.to_owned()
    };
    let expected_email = if expected_email.is_empty() {
        config(repo, "user.email", active)?
    } else {
        expected_email.to_owned()
    };
    let signs = profile.own_signing && profile.signing.sign_commits
        || config(repo, "commit.gpgSign", active)?.eq_ignore_ascii_case("true");
    let mut commits = Vec::new();
    let mut findings = Vec::new();
    if !deleting && push_oid != head {
        let tag = read(repo, &["cat-file", "tag", &push_oid], active)?;
        if secret_lines(tag.split_once("\n\n").map_or("", |x| x.1))
            .next()
            .is_some()
        {
            findings.push(Finding{kind:Kind::Secret,commit:push_oid.clone(),path:String::new(),detail:"Likely secret in annotated tag message; value withheld. Recreate the tag before pushing.".into()});
        }
    }
    let mut metadata_bytes = 0usize;
    for oid in &ids {
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
        for url in &urls {
            let (_, email) = profile.expected_author(url);
            let email = if email.is_empty() {
                expected_email.as_str()
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
    let mut objects_args = vec!["rev-list", "--objects", "--no-object-names", head.as_str()];
    if let Some(b) = &base {
        objects_args.extend(["--not", b]);
    }
    let objects: BTreeSet<String> = read(repo, &objects_args, active)?
        .lines()
        .map(str::to_owned)
        .collect();
    if objects.len() > MAX_OBJECTS {
        return Err("More than 50,000 outgoing objects need checking. Split the push or fetch the exact target.".into());
    }
    let mut seen = BTreeSet::new();
    let mut lfs_objects = BTreeSet::new();
    for commit in &commits {
        let tree = read(
            repo,
            &["ls-tree", "-r", "-l", "-z", "--full-tree", &commit.oid],
            active,
        )?;
        for item in tree.split('\0').filter(|s| !s.is_empty()) {
            let Some((meta, path)) = item.split_once('\t') else {
                continue;
            };
            let words: Vec<_> = meta.split_whitespace().collect();
            if words.len() != 4
                || words[1] != "blob"
                || !objects.contains(words[2])
                || !seen.insert((words[2].to_owned(), path.to_owned()))
            {
                continue;
            }
            let size = words[3]
                .parse::<u64>()
                .map_err(|_| "Unknown outgoing blob size")?;
            if size <= 1024 {
                let text = read(repo, &["cat-file", "blob", words[2]], active)?;
                if let Some(pointer) = git::parse_lfs_pointer(&text) {
                    let oid = pointer
                        .oid
                        .strip_prefix("sha256:")
                        .ok_or("Unsupported LFS object hash")?;
                    if oid.len() != 64 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err("Malformed LFS object hash".into());
                    }
                    lfs_objects.insert(oid.to_owned());
                }
            }
            if size > LARGE {
                findings.push(Finding {
                    kind: Kind::Large,
                    commit: commit.oid.clone(),
                    path: path.into(),
                    detail: format!("{} Git blob; consider Git LFS.", git::human_size(size)),
                });
            }
            for line in blob_secret_lines(repo, words[2], active)? {
                findings.push(Finding {
                    kind: Kind::Secret,
                    commit: commit.oid.clone(),
                    path: path.into(),
                    detail: format!("Likely secret at line {line}; value withheld."),
                });
            }
            if findings.len() > MAX_FINDINGS {
                return Err("More than 2,000 findings need review. Repair or split this history before pushing.".into());
            }
        }
    }
    if !deleting && let Some(base) = &base {
        let lost = read(
            repo,
            &[
                "log",
                "--no-show-signature",
                "--format=%H%x00%ae",
                base,
                "--not",
                &head,
            ],
            active,
        )?;
        let mut count = 0;
        let mut others = 0;
        for line in lost.lines() {
            if let Some((_, email)) = line.split_once('\0') {
                count += 1;
                if !email.eq_ignore_ascii_case(&expected_email) {
                    others += 1;
                }
            }
        }
        if count > 0 {
            findings.push(Finding{kind:Kind::RemoteHistory,commit:base.clone(),path:String::new(),detail:format!("Force push would drop {count} remote commits, {others} by other authors. Based on the fetched target; the lease must still match.")});
        }
    }
    let config = config_stamp(repo, active)?;
    if config != initial_config {
        return Err("The push configuration changed while checking. Reopen preflight.".into());
    }
    let targets = urls.iter().map(|u| target_label(u)).collect();
    let plan = Plan {
        branch: source,
        remote,
        destination,
        head,
        base,
        commits,
        findings,
        targets,
        expected_name,
        expected_email,
        profile,
        force,
        deleting,
        tracking,
        lfs_objects,
        push_oid,
        original,
        urls,
        checkout,
        checkout_oid,
        config,
        config_before_push,
        local_refs,
        extras,
        tracking_ref,
        lfs_config,
        receive_pack,
    };
    check_active(repo, &plan, active)?;
    if !plan.deleting
        && read(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{}^{{commit}}", plan.push_oid),
            ],
            active,
        )?
        .trim()
            != plan.head
    {
        return Err("The source changed while checking. Reopen preflight.".into());
    }
    Ok(plan)
}

pub(crate) fn target_label(url: &str) -> String {
    location(url)
        .map(|(h, p)| {
            if secret_lines(&p).next().is_some() {
                format!("{h}/[sensitive path]")
            } else {
                format!("{h}/{p}")
            }
        })
        .unwrap_or_else(|| {
            let path = if let Some(rest) = url.strip_prefix("file://") {
                let rest = rest.split(['?', '#']).next().unwrap_or(rest);
                if let Some((authority, path)) = rest.split_once('/') {
                    format!(
                        "{}/{}",
                        authority.rsplit('@').next().unwrap_or(authority),
                        path
                    )
                } else {
                    rest.rsplit('@').next().unwrap_or(rest).to_owned()
                }
            } else {
                url.to_owned()
            };
            format!("Local repository: {}", display_path(&path))
        })
}

pub(crate) fn display_path(path: &str) -> String {
    if secret_lines(path).next().is_some() {
        "[sensitive path]".into()
    } else {
        path.chars()
            .flat_map(|c| {
                if c.is_control() {
                    c.escape_default().collect::<Vec<_>>()
                } else {
                    vec![c]
                }
            })
            .collect()
    }
}

impl Plan {
    pub(crate) fn is_current_branch(&self) -> bool {
        !self.deleting && self.branch == self.checkout && self.branch.starts_with("refs/heads/")
    }
    pub(crate) fn lfs_targets(&self) -> Vec<String> {
        let values: BTreeSet<_> = self
            .lfs_config
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(_, v)| target_label(v))
            .collect();
        if values.is_empty() {
            self.targets.clone()
        } else {
            values.into_iter().collect()
        }
    }

    pub(crate) fn local_steps(&self) -> Vec<(bool, String)> {
        let mut before = true;
        let mut steps = Vec::new();
        for op in &self.original {
            if push_words(op).is_some() {
                before = false;
            } else if !op.first().is_some_and(|value| value.starts_with('\0')) {
                steps.push((before, display_path(&op.join(" "))));
            }
        }
        steps
    }

    pub(crate) fn again(&self) -> Vec<Vec<String>> {
        self.original.clone()
    }
}

pub(crate) fn location(url: &str) -> Option<(String, String)> {
    let url = url.split(['?', '#']).next()?.trim();
    let (host, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if !["https", "http", "ssh", "git"].contains(&scheme) {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        (authority.rsplit('@').next()?, path)
    } else {
        if url.contains('\\') || url.starts_with('/') || url.starts_with('.') {
            return None;
        }
        let (authority, path) = url.split_once(':')?;
        (authority.rsplit('@').next()?, path)
    };
    if host.len() < 2 || path.is_empty() || host.chars().any(char::is_control) {
        return None;
    }
    let host = match host.to_ascii_lowercase().as_str() {
        "ssh.github.com" | "ssh.github.com:443" => "github.com".to_owned(),
        "altssh.gitlab.com" | "altssh.gitlab.com:443" => "gitlab.com".to_owned(),
        host => host.to_owned(),
    };
    Some((host, path.trim_matches('/').trim_end_matches(".git").into()))
}

fn secret_lines(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.lines().enumerate().filter_map(|(i, line)| {
        let l = line.trim();
        let lower = l.to_ascii_lowercase();
        let credential = [
            "password",
            "passwd",
            "secret",
            "api_key",
            "apikey",
            "access_token",
            "auth_token",
        ]
        .iter()
        .any(|k| lower.contains(k))
            && (l.contains('=') || l.contains(':'))
            && l.split_once(['=', ':'])
                .is_some_and(|(_, v)| v.trim_matches([' ', '\'', '"', ';', ',']).len() >= 8);
        let token = ["ghp_", "github_pat_", "glpat-", "sk_live_", "xoxb-", "AKIA"]
            .iter()
            .any(|p| {
                l.find(p).is_some_and(|n| {
                    l[n + p.len()..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                        .count()
                        >= 12
                })
            });
        (credential || token || l.contains("PRIVATE KEY-----")).then_some(i + 1)
    })
}

fn blob_secret_lines(
    repo: &Path,
    oid: &str,
    active: &dyn Fn() -> bool,
) -> Result<Vec<usize>, String> {
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid outgoing object".into());
    }
    // Streaming on the repository's machine avoids downloading/allocating huge blobs. Only
    // line numbers leave this pipeline: neither a matched line nor its secret enters a UI result.
    let pattern = r#"PRIVATE KEY-----|ghp_[A-Za-z0-9_]{12,}|github_pat_[A-Za-z0-9_]{12,}|glpat-[A-Za-z0-9_-]{12,}|sk_live_[A-Za-z0-9_]{12,}|xoxb-[A-Za-z0-9-]{12,}|AKIA[A-Z0-9]{16}|(password|passwd|secret|api_key|apikey|access_token|auth_token)[[:space:]\"']*[:=].{8,64}"#;
    let script = format!(
        "!command -v grep >/dev/null && command -v cut >/dev/null && command -v head >/dev/null || exit 1; {{ {{ git --no-lazy-fetch cat-file blob {} || printf 'READ_ERROR\\n' >&3; }} | grep -ainE {} | cut -d: -f1 | head -n 257; }} 3>&1",
        oid,
        crate::remote::sh_quote(pattern)
    );
    let output = read(
        repo,
        &[
            "-c",
            &format!("alias.gitcraft-preflight-secrets={script}"),
            "gitcraft-preflight-secrets",
        ],
        active,
    )?;
    output
        .lines()
        .map(|n| {
            n.parse().map_err(|_| {
                "The content scan failed or exceeded its finding limit; no push was run.".into()
            })
        })
        .collect()
}

static PLANS: OnceLock<Mutex<BTreeMap<u64, Arc<Plan>>>> = OnceLock::new();
pub(crate) fn register(plan: Plan) -> Vec<String> {
    static IDS: AtomicU64 = AtomicU64::new(1);
    let id = IDS.fetch_add(1, Ordering::Relaxed);
    let mut plans = PLANS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while plans.len() >= 64 {
        plans.pop_first();
    }
    plans.insert(id, Arc::new(plan));
    vec![APPROVED.into(), id.to_string()]
}

pub(crate) fn registered(id: &str) -> Result<Arc<Plan>, String> {
    PLANS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&id.parse::<u64>().map_err(|_| "Reopen push preflight")?)
        .cloned()
        .ok_or_else(|| "This preview expired; reopen push preflight.".into())
}

fn clean_lfs_config(repo: &Path, head: &str, active: &dyn Fn() -> bool) -> Result<(), String> {
    let untracked = read(repo, &["ls-files", "--others", "--", ".lfsconfig"], active)?;
    let flags = read(repo, &["ls-files", "-v", "--", ".lfsconfig"], active)?;
    if !untracked.is_empty() || flags.lines().any(|l| !l.starts_with("H ")) {
        return Err("Commit or remove .lfsconfig changes, and clear skip/assume-unchanged flags on it before reviewing a push.".into());
    }
    let filter = read(repo, &["check-attr", "filter", "--", ".lfsconfig"], active)?;
    if !filter.trim_end().ends_with(": unspecified") && !filter.trim_end().ends_with(": unset") {
        return Err(".lfsconfig must not pass through a content filter when reviewing endpoint configuration.".into());
    }
    let change = read(
        repo,
        &[
            "-c",
            "core.fsmonitor=false",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--raw",
            "--no-renames",
            head,
            "--",
            ".lfsconfig",
        ],
        active,
    )?;
    if !change.is_empty() {
        return Err("The working .lfsconfig differs from the inspected commit. Commit or stash that routing change before pushing.".into());
    }
    Ok(())
}

pub(crate) fn check(repo: &Path, plan: &Plan) -> Result<(), String> {
    check_active(repo, plan, &|| true)
}

fn check_active(repo: &Path, plan: &Plan, active: &dyn Fn() -> bool) -> Result<(), String> {
    clean_lfs_config(repo, &plan.checkout_oid, active)?;
    if read(repo, &["rev-parse", "--verify", "HEAD"], active)?.trim() != plan.checkout_oid {
        return Err("The checked-out branch moved. Reopen push preflight.".into());
    }
    let config = config_stamp(repo, active)?;
    if (config != plan.config && config != plan.config_before_push)
        || optional(repo, &["symbolic-ref", "-q", "HEAD"], active)?.trim() != plan.checkout
        || (!plan.deleting
            && read(
                repo,
                &["rev-parse", "--verify", "--end-of-options", &plan.branch],
                active,
            )?
            .trim()
                != plan.push_oid)
    {
        return Err("The branch, checkout, identity or push configuration changed. Reopen push preflight; nothing was pushed.".into());
    }
    let urls: Vec<String> = read(
        repo,
        &["remote", "get-url", "--push", "--all", &plan.remote],
        active,
    )?
    .lines()
    .map(str::to_owned)
    .collect();
    if urls != plan.urls {
        return Err("The push addresses changed. Reopen push preflight.".into());
    }
    Ok(())
}

pub(crate) fn push_args(plan: &Plan) -> Result<Vec<String>, String> {
    if (plan.force || plan.deleting) && plan.base.is_none() {
        return Err(
            "Fetch this exact target before force pushing: no remote lease was inspected.".into(),
        );
    }
    let mut args = vec![
        "push".into(),
        "--no-follow-tags".into(),
        "--recurse-submodules=no".into(),
    ];
    if !plan.destination.starts_with("refs/for/") {
        if !plan.deleting
            && !plan.force
            && plan.findings.iter().any(|f| f.kind == Kind::RemoteHistory)
        {
            return Err("The fetched target is not an ancestor. Keep its commits or deliberately choose force-with-lease.".into());
        }
        // Normal mode is still a fast-forward (or absent-ref creation). The explicit
        // expected tip makes the inspected outgoing range valid at the server too.
        args.push(format!(
            "--force-with-lease={}:{}",
            plan.destination,
            plan.base.as_deref().unwrap_or_default()
        ));
    }
    args.extend(plan.extras.clone());
    args.extend([
        "--".into(),
        plan.remote.clone(),
        format!("{}:{}", plan.push_oid, plan.destination),
    ]);
    Ok(args)
}

pub(crate) fn approved_ops(plan: Plan) -> Vec<Vec<String>> {
    let mut ops: Vec<_> = plan
        .local_refs
        .iter()
        .map(|(name, oid)| crate::repo::expect_ref_at(name, oid))
        .collect();
    let original = plan.original.clone();
    let approved = register(plan);
    ops.extend(original.into_iter().map(|step| {
        if push_words(&step).is_some() {
            approved.clone()
        } else {
            step
        }
    }));
    ops
}

pub(crate) fn execute(
    repo: &Path,
    plan: &Plan,
    creds: Option<&git::Credentials>,
) -> Result<String, String> {
    check(repo, plan)?;
    let mut args = push_args(plan)?;
    for (i, url) in plan.urls.iter().enumerate() {
        check(repo, plan)?;
        let opaque = format!(
            "gitcraft-preflight-{}-{}-{i}",
            std::process::id(),
            git::now()
        );
        let remote_at = args.len() - 2;
        args[remote_at] = opaque.clone();
        let objects: Vec<_> = plan.lfs_objects.iter().cloned().collect();
        git::push_preflight_with(
            repo,
            &args,
            git::PushTarget {
                address: url,
                alias: &opaque,
                original_remote: &plan.remote,
                lfs_config: &plan.lfs_config,
                receive_pack: &plan.receive_pack,
                lfs_objects: &objects,
            },
            creds,
        )?;
    }
    if let Some(reference) = &plan.tracking_ref {
        // A background fetch may already have updated it. Never overwrite a newer tip.
        let args = if plan.deleting {
            vec![
                "update-ref",
                "-d",
                reference,
                plan.base.as_deref().unwrap_or(""),
            ]
        } else {
            vec![
                "update-ref",
                reference,
                &plan.head,
                plan.base.as_deref().unwrap_or(""),
            ]
        };
        let _ = git::forecast_run(repo, &args, &[], &[0], &|| true);
    }
    if plan.tracking
        && plan.branch.starts_with("refs/heads/")
        && plan.destination.starts_with("refs/heads/")
    {
        // Setting an upstream is local metadata; it is never used to choose this push's ref.
        let branch = plan.branch.trim_start_matches("refs/heads/");
        git::run(
            Some(repo),
            &["config", &format!("branch.{branch}.remote"), &plan.remote],
        )?;
        git::run(
            Some(repo),
            &[
                "config",
                &format!("branch.{branch}.merge"),
                &plan.destination,
            ],
        )?;
    }
    Ok("Push completed to the reviewed target(s).".into())
}
