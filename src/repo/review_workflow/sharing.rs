//! Captured Git-ref sharing destination, private to review artifacts. This is independent
//! of forge account/profile schemas: Git credentials remain with the existing runner.
use crate::{git, network};
use std::{path::Path, sync::Arc};
#[derive(Clone)]
pub(super) struct Target {
    pub name: String,
    fetch: String,
    push: String,
    source: Arc<git::CommandSource>,
}
fn url(repo: &Path, name: &str, push: bool) -> Result<String, String> {
    let args = if push {
        vec!["remote", "get-url", "--push", "--all", name]
    } else {
        vec!["remote", "get-url", "--all", name]
    };
    let urls = git::run(Some(repo), &args)?;
    let rows: Vec<_> = urls.lines().collect();
    match rows.as_slice() {
        [url] if !url.is_empty() => Ok((*url).into()),
        _ => Err("Review-artifact sharing requires exactly one fetch and one push URL".into()),
    }
}
impl Target {
    pub fn capture(repo: &Path, observed: &git::Remote) -> Result<Self, String> {
        let captured_url = |source: &Arc<git::CommandSource>, push| {
            let args = if push {
                vec![
                    "remote",
                    "get-url",
                    "--push",
                    "--all",
                    observed.name.as_str(),
                ]
            } else {
                vec!["remote", "get-url", "--all", observed.name.as_str()]
            };
            let args = source.bind(&args);
            let urls = git::run(
                Some(repo),
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
            let rows: Vec<_> = urls.lines().collect();
            match rows.as_slice() {
                [url] if !url.is_empty() => Ok((*url).to_owned()),
                _ => Err(
                    "Review-artifact sharing requires exactly one fetch and one push URL"
                        .to_owned(),
                ),
            }
        };
        let (source, fetch, push) = capture_destination(
            observed,
            || git::CommandSource::capture(repo),
            captured_url,
            |source| source.check_context(),
        )?;
        Ok(Self {
            source,
            name: observed.name.clone(),
            fetch,
            push,
        })
    }
    pub fn guard(&self, repo: &Path) -> Result<(), String> {
        if let Some(p) = network::refused() {
            return Err(p.why().into());
        }
        self.source.check_context()?;
        if url(repo, &self.name, false)? != self.fetch || url(repo, &self.name, true)? != self.push
        {
            return Err("Sharing destination changed. A completed transfer may already have affected the captured destination; refresh before retrying".into());
        }
        Ok(())
    }
    pub fn run(
        &self,
        repo: &Path,
        args: &[&str],
        creds: Option<&git::Credentials>,
    ) -> Result<String, String> {
        self.guard(repo)?;
        let args = self.source.bind(args);
        git::run_unattended(
            Some(repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            creds,
        )
    }
    /// Fetch into the private ref view, then atomically import only this artifact namespace.
    /// A failed transfer/config guard leaves the main repository's cache refs untouched.
    pub fn fetch(
        &self,
        repo: &Path,
        push_endpoint: bool,
        specs: &[String],
        root: &str,
        prune: bool,
        creds: Option<&git::Credentials>,
    ) -> Result<(), String> {
        // The private ref view can lag behind a prior publication's live cache update.
        // CAS must witness the destination being updated, before starting the transfer.
        let before = git::run(
            Some(repo),
            &["for-each-ref", "--format=%(objectname) %(refname)", root],
        )?
        .lines()
        .map(|line| {
            let (oid, reference) = line
                .split_once(' ')
                .ok_or("Invalid artifact cache listing")?;
            Ok((reference.to_owned(), oid.to_owned()))
        })
        .collect::<Result<Vec<_>, String>>()?;
        let mut args = vec!["fetch", "--no-tags", "--no-write-fetch-head"];
        if prune {
            args.push("--prune");
        }
        args.push(if push_endpoint {
            &self.push
        } else {
            &self.fetch
        });
        args.extend(specs.iter().map(String::as_str));
        self.run(repo, &args, creds)?;
        self.guard(repo)?;
        let after = self.source.refs(root)?;
        let transaction = cache_transaction(root, &before, &after)?;
        git::run_input(Some(repo), &["update-ref", "--stdin"], &transaction).map(|_| ())
    }
    pub fn source_refs(&self, root: &str) -> Result<Vec<(String, String)>, String> {
        self.source.refs(root)
    }
    pub fn fetched(&self, reference: &str) -> Result<Option<String>, String> {
        self.source.reference(reference)
    }
    pub fn push_url(&self) -> &str {
        &self.push
    }
    pub fn cache_key(&self) -> String {
        format!(
            "{:x}",
            md5::compute(format!("{}\0{}\0{}", self.name, self.fetch, self.push))
        )
    }
}
/// Selection and effective routing must agree inside the same frozen source. An
/// earlier live `remotes` check cannot establish ownership of the later snapshot.
fn capture_destination<S>(
    observed: &git::Remote,
    capture: impl FnOnce() -> Result<S, String>,
    mut endpoint: impl FnMut(&S, bool) -> Result<String, String>,
    check: impl FnOnce(&S) -> Result<(), String>,
) -> Result<(S, String, String), String> {
    let source = capture()?;
    let fetch = endpoint(&source, false)?;
    // Remote.url is the displayed fetch URL (including Git's URL rewrites).
    if fetch != observed.url {
        return Err("Sharing remote changed; refresh repository remotes and try again".into());
    }
    let push = endpoint(&source, true)?;
    check(&source)?;
    Ok((source, fetch, push))
}
fn cache_transaction(
    root: &str,
    before: &[(String, String)],
    after: &[(String, String)],
) -> Result<String, String> {
    let valid = |reference: &str| {
        (reference == root
            || reference
                .strip_prefix(root)
                .is_some_and(|tail| root.ends_with('/') || tail.starts_with('/')))
            && reference.starts_with("refs/gitcraft/")
            && !reference.contains([' ', '\n', '\r', '\0', '\\'])
            && reference
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != "..")
    };
    let oid = |s: &str| matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit());
    if before
        .iter()
        .chain(after)
        .any(|(r, o)| !valid(r) || !oid(o))
    {
        return Err("Artifact cache update crossed its captured namespace".into());
    }
    let mut text = "start\n".to_owned();
    for (reference, object) in after {
        let old = before
            .iter()
            .find(|(r, _)| r == reference)
            .map(|(_, o)| o.clone())
            .unwrap_or_else(|| "0".repeat(object.len()));
        text.push_str(&format!("update {reference} {object} {old}\n"));
    }
    for (reference, object) in before
        .iter()
        .filter(|(r, _)| !after.iter().any(|(a, _)| a == r))
    {
        text.push_str(&format!("delete {reference} {object}\n"));
    }
    text.push_str("prepare\ncommit\n");
    Ok(text)
}
#[cfg(test)]
#[path = "sharing/tests.rs"]
mod tests;
