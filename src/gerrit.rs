//! Gerrit's commit-based review flow. Git handles transport and credentials; no API login.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::git::{self, Remote};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Server {
    pub remote: String,
    pub web: String,
    pub project: String,
    pub web_warning: Option<String>,
}

impl Server {
    /// A project-qualified search also works before a change has its numeric id locally.
    pub fn change_url(&self, id: &str) -> String {
        format!(
            "{}/q/{}",
            self.web,
            encode(&format!("project:{} change:{id}", self.project))
        )
    }
}

/// Read on the reload thread, never while drawing. Explicit configuration wins over names.
pub(crate) fn detect(repo: &Path, remotes: &[Remote]) -> Option<Server> {
    detect_with(remotes, &crate::forge::gitcraft_config(repo))
}

fn detect_with(remotes: &[Remote], config: &HashMap<String, String>) -> Option<Server> {
    let remote = remotes
        .iter()
        .find(|r| r.name == "origin")
        .or_else(|| remotes.first())?;
    let (host, project, web) = remote_location(&remote.url)?;
    let configured = config
        .get("gitcraft.forge")
        .or_else(|| config.get(&format!("gitcraft.{host}.forge")));
    let gerrit = configured.map_or_else(
        || host.starts_with("gerrit.") || host.ends_with(".googlesource.com"),
        |v| v.eq_ignore_ascii_case("gerrit"),
    );
    if !gerrit {
        return None;
    }
    // A reverse proxy can serve Gerrit under a path. The override is an explicit web URL,
    // not the SSH port (often 29418), and never contains user info or a query.
    let (web, web_warning) = match config.get("gitcraft.gerriturl") {
        Some(url) if valid_web(url) => (url.trim_end_matches('/').to_owned(), None),
        Some(_) => (web, Some("Invalid gitcraft.gerritUrl: use an HTTP(S) URL without credentials, query or fragment. Using the remote's web address; Gerrit review submission remains enabled.".to_owned())),
        None => (web, None),
    };
    let mut project = project.as_str();
    if remote.url.starts_with("http://") || remote.url.starts_with("https://") {
        let prefix = web
            .split_once("://")
            .and_then(|(_, rest)| rest.split_once('/'))
            .map(|(_, path)| path);
        if let Some(prefix) = prefix {
            project = project
                .strip_prefix(&format!("{prefix}/"))
                .unwrap_or(project);
        }
        project = project.strip_prefix("a/").unwrap_or(project);
    }
    Some(Server {
        remote: remote.name.clone(),
        web,
        project: project.to_owned(),
        web_warning,
    })
}

fn valid_web(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    !rest.is_empty()
        && !rest.starts_with('/')
        && !rest
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '@' | '?' | '#' | '\\'))
}

/// Unlike other forges, Gerrit projects can have just one path component.
fn remote_location(url: &str) -> Option<(String, String, String)> {
    let (scheme, authority, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if !matches!(scheme, "ssh" | "http" | "https") {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        (scheme, authority, path)
    } else {
        let (authority, path) = url.split_once(':')?;
        ("ssh", authority, path)
    };
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    let host = if authority.starts_with('[') {
        &authority[..=authority.find(']')?]
    } else {
        authority.split(':').next()?
    }
    .to_lowercase();
    if host.len() <= 1 || authority.is_empty() {
        return None;
    }
    let project = path.trim_matches('/');
    let project = project.strip_suffix(".git").unwrap_or(project);
    if project.is_empty()
        || project
            .chars()
            .any(|c| c.is_control() || matches!(c, '?' | '#'))
    {
        return None;
    }
    let web = if let Some(name) = host.strip_suffix(".googlesource.com") {
        let name = name.strip_suffix("-review").unwrap_or(name);
        format!("https://{name}-review.googlesource.com")
    } else if scheme == "ssh" {
        format!("https://{host}")
    } else {
        format!("{scheme}://{authority}")
    };
    if !valid_web(&web) {
        return None;
    }
    Some((host, project.to_owned(), web))
}

/// Exactly one nonzero Change-Id in the final trailer paragraph, as Gerrit requires.
pub(crate) fn change_id(message: &str) -> Result<&str, String> {
    if !message.contains("\nChange-Id:") {
        return Err("missing Change-Id in the message footer".to_owned());
    }
    let mut lines = message.trim_end().lines().rev();
    let footer: Vec<&str> = lines
        .by_ref()
        .take_while(|line| !line.trim().is_empty())
        .collect();
    // take_while consumed the separator. At least a subject must remain before it.
    if lines.next().is_none() {
        return Err("missing Change-Id in the message footer".to_owned());
    }
    let mut id = None;
    let mut trailer = false;
    let mut change = false;
    for line in footer.into_iter().rev() {
        if line.starts_with([' ', '\t']) && trailer {
            if change {
                return Err("Change-Id must fit on one line".to_owned());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err("Change-Id must be in the final trailer paragraph".to_owned());
        };
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err("Change-Id must be in the final trailer paragraph".to_owned());
        }
        trailer = true;
        change = key == "Change-Id";
        if change {
            if id.is_some() {
                return Err("more than one Change-Id in the footer".to_owned());
            }
            id = Some(value.trim());
        }
    }
    let id = id.ok_or_else(|| "missing Change-Id in the message footer".to_owned())?;
    if id.len() != 41
        || !id.starts_with('I')
        || !id[1..].bytes().all(|b| b.is_ascii_hexdigit())
        || id[1..].bytes().all(|b| b == b'0')
    {
        return Err(
            "Change-Id must be I followed by 40 hexadecimal digits (not all zero)".to_owned(),
        );
    }
    Ok(id)
}

/// Keeps a reviewed commit's identity even if its amended description was replaced.
pub(crate) fn amended_message(original: &str, edited: &str) -> Result<String, String> {
    let Ok(id) = change_id(original) else {
        return Ok(edited.to_owned());
    };
    match change_id(edited) {
        Ok(new) if new == id => Ok(edited.to_owned()),
        Ok(_) => Err("Keep the original Change-Id when amending a Gerrit change".to_owned()),
        Err(_) if !edited.lines().any(|l| l.starts_with("Change-Id:")) => {
            let edited = edited.trim_end();
            let footer = edited.rsplit_once("\n\n").map(|(_, footer)| footer);
            let mut trailer = false;
            let has_trailers = footer.is_some_and(|footer| {
                footer.lines().all(|line| {
                    if line.starts_with([' ', '\t']) {
                        return trailer;
                    }
                    trailer = line.split_once(':').is_some_and(|(key, _)| {
                        !key.is_empty()
                            && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    });
                    trailer
                })
            });
            let separator = if has_trailers { "\n" } else { "\n\n" };
            Ok(format!("{edited}{separator}Change-Id: {id}"))
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn valid_branch(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && name != "@"
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name
            .split('/')
            .any(|p| p.starts_with('.') || p.ends_with(".lock"))
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '%')
        })
}

pub(crate) fn valid_topic(topic: &str) -> bool {
    !topic.chars().any(|c| c.is_control())
}

/// The push's exact object and base are pinned when the user confirms it. This internal
/// expectation is repeated after signing in, and never passed to git as a command.
pub(crate) const CHECK: &str = "\0gerrit-review";
pub(crate) const AMEND: &str = "\0gerrit-amend";

pub(crate) fn amend_args(repo: &Path, args: &[String]) -> Result<Vec<String>, String> {
    let mut prepared = Vec::new();
    let mut paragraphs = Vec::new();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        if word == "-m" {
            paragraphs.push(words.next().ok_or("Missing amend message")?.as_str());
        } else {
            prepared.push(word.clone());
        }
    }
    if prepared.first().map(String::as_str) != Some("commit")
        || !prepared.iter().any(|word| word == "--amend")
        || paragraphs.is_empty()
    {
        return Err("Invalid Gerrit amendment".to_owned());
    }
    let message = amended_message(&git::head_message(repo)?, &paragraphs.join("\n\n"))?;
    prepared.extend(["-m".to_owned(), message]);
    Ok(prepared)
}

pub(crate) fn review_ops(
    remote: &str,
    target: &str,
    topic: &str,
    head: &str,
    base: &str,
) -> Result<Vec<Vec<String>>, String> {
    if remote.is_empty()
        || remote.starts_with('-')
        || !valid_branch(target)
        || !valid_topic(topic)
        || ![head, base]
            .iter()
            .all(|s| matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Choose a remote, a fetched target branch and a valid topic".to_owned());
    }
    let mut push = vec!["push".to_owned()];
    if !topic.is_empty() {
        push.push(format!("--push-option=topic={topic}"));
    }
    push.extend([
        "--".to_owned(),
        remote.to_owned(),
        format!("{head}:refs/for/{target}"),
    ]);
    Ok(vec![
        vec![CHECK.to_owned(), base.to_owned(), head.to_owned()],
        push,
    ])
}

/// Validate every commit being submitted, including ancestors outside the visible graph.
/// A missing hook is reported with instructions, never downloaded or executed by gitcraft.
#[cfg(test)]
pub(crate) fn check_stack(repo: &Path, base: &str, head: &str) -> Result<(), String> {
    check_stack_with(base, head, |args| git::run(Some(repo), args))
}

pub(crate) fn check_stack_with(
    base: &str,
    head: &str,
    mut run: impl FnMut(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    run(&["merge-base", "--is-ancestor", base, head])
        .map_err(|error| if error == git::CANCELLED { error } else { "The target branch is not an ancestor of this commit. Fetch and rebase onto the target before pushing for review.".to_owned() })?;
    let range = format!("{base}..{head}");
    let out = run(&[
        "log",
        "--no-show-signature",
        "--no-notes",
        "--encoding=UTF-8",
        "--format=%H%x00%P%x00%B%x00",
        &range,
        "--",
    ])?;
    let mut ids = HashSet::new();
    let mut count = 0;
    for fields in out.split('\0').collect::<Vec<_>>().as_chunks::<3>().0 {
        let hash = fields[0].trim();
        if fields[1].split_whitespace().count() > 1 {
            return Err(format!(
                "Merge commit {} cannot be pushed with this review flow; rebase the series first",
                &hash[..hash.len().min(8)]
            ));
        }
        let id = change_id(fields[2]).map_err(|e| format!("{}: {e}. Install Gerrit's commit-msg hook from your server, then amend this commit to add its Change-Id.", &hash[..hash.len().min(8)]))?;
        if !ids.insert(id.to_owned()) {
            return Err(format!(
                "Two commits have the same Change-Id ({id}); each change needs its own"
            ));
        }
        count += 1;
    }
    if count == 0 {
        return Err("There are no commits above the target branch to review".to_owned());
    }
    Ok(())
}

fn encode(text: &str) -> String {
    let mut encoded = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests;
