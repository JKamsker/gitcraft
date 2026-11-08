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
mod tests {
    use super::*;
    #[test]
    fn r3_pure_sharing_selection_is_bound_to_the_frozen_endpoint_snapshot() -> Result<(), String> {
        use std::cell::RefCell;
        let observed = git::Remote {
            name: "origin".into(),
            url: "https://example.test/selected A".into(),
        };
        // Recording only: live selection was A, but capture consistently freezes B.
        // Even an internally unchanged B snapshot must not become the destination.
        let events = RefCell::new(Vec::new());
        let changed = capture_destination(
            &observed,
            || {
                events.borrow_mut().push("freeze B");
                Ok("https://example.test/replacement B")
            },
            |frozen, push| {
                assert!(!push, "must refuse before reading the replacement push URL");
                events.borrow_mut().push("frozen fetch B");
                Ok((*frozen).into())
            },
            |_| {
                events.borrow_mut().push("unexpected verification");
                Err("unchanged B cannot validate the observed A selection".into())
            },
        );
        assert!(
            changed
                .err()
                .ok_or("accepted replacement")?
                .contains("changed")
        );
        assert_eq!(*events.borrow(), ["freeze B", "frozen fetch B"]);

        // A split push URL is legitimate: select using frozen effective fetch semantics,
        // and retain the exact (possibly space-containing) effective endpoint strings.
        let push = "ssh://push.example.test/team repo";
        let (source, fetch, captured_push) = capture_destination(
            &observed,
            || Ok((observed.url.clone(), push.to_owned())),
            |source, push| Ok(if push { &source.1 } else { &source.0 }.clone()),
            |_| Ok(()),
        )?;
        assert_eq!(fetch, observed.url);
        assert_eq!(captured_push, push);
        assert_eq!(source, (fetch, captured_push));
        assert!(
            capture_destination(
                &observed,
                || Ok(observed.url.clone()),
                |source, _| Ok(source.clone()),
                |_| Err("context changed after capture".into()),
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn r2_pure_artifact_cache_import_is_atomic_cas_and_cannot_cross_source() -> Result<(), String> {
        let root = "refs/gitcraft/intent-remotes/source/";
        let old = "a".repeat(40);
        let new = "b".repeat(40);
        let transaction = cache_transaction(
            root,
            &[(format!("{root}old"), old.clone())],
            &[(format!("{root}new"), new.clone())],
        )?;
        assert!(transaction.starts_with("start\n"));
        assert!(transaction.ends_with("prepare\ncommit\n"));
        assert!(transaction.contains(&format!("update {root}new {new} {}", "0".repeat(40))));
        assert!(transaction.contains(&format!("delete {root}old {old}")));
        // A refresh replaces an existing live cache ref, even when the transfer's
        // captured private view originally had no such ref (or retained an older one).
        let reference = format!("{root}same");
        let refresh = cache_transaction(
            root,
            &[(reference.clone(), old.clone())],
            &[(reference.clone(), new.clone())],
        )?;
        assert_eq!(
            refresh,
            format!("start\nupdate {reference} {new} {old}\nprepare\ncommit\n")
        );
        assert!(
            cache_transaction(
                root,
                &[],
                &[("refs/gitcraft/intent-remotes/foreign/id".into(), new)]
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    #[ignore = "R2 runtime hold: local Git routing/credential/ref fixture authored, not executed"]
    fn r2_runtime_frozen_endpoint_survives_name_rewrite_and_profile_configuration_changes()
    -> Result<(), String> {
        let source = git::stack::test_repo::TestRepo::new("frozen-artifact-source")?;
        let a = git::stack::test_repo::TestRepo::new("frozen-artifact-a")?;
        let b = git::stack::test_repo::TestRepo::new("frozen-artifact-b")?;
        let oid = source.tip("HEAD")?;
        source.git(&["remote", "add", "team", a.path().to_str().ok_or("path")?])?;
        let observed = git::remotes(source.path())?
            .into_iter()
            .find(|r| r.name == "team")
            .ok_or("remote")?;
        let captured = Target::capture(source.path(), &observed)?;
        let reference = "refs/gitcraft/recorded-artifact/owned";
        let spec = format!("{oid}:{reference}");
        // Construct the exact runner argv first, then change routing before the child runs.
        let args = captured
            .source
            .bind(&["push", "--", captured.push_url(), &spec]);
        source.git(&[
            "remote",
            "set-url",
            "team",
            b.path().to_str().ok_or("path")?,
        ])?;
        source.git(&[
            "config",
            &format!("url.{}.insteadOf", b.path().display()),
            a.path().to_str().ok_or("path")?,
        ])?;
        source.git(&[
            "config",
            &format!("url.{}.pushInsteadOf", b.path().display()),
            a.path().to_str().ok_or("path")?,
        ])?;
        let outcome = git::run_unattended(
            Some(source.path()),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            None,
        );
        assert!(outcome.is_err(), "worker must refuse changed configuration");
        assert!(a.git(&["rev-parse", "--verify", reference]).is_err());
        assert!(b.git(&["rev-parse", "--verify", reference]).is_err());
        assert!(captured.guard(source.path()).is_err());
        source.git(&["remote", "remove", "team"])?;
        let args = captured
            .source
            .bind(&["ls-remote", "--refs", captured.push_url(), reference]);
        assert!(
            git::run_unattended(
                Some(source.path()),
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
                None
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn replacement_remote_cannot_reuse_a_captured_destination() -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("sharing-target")?;
        r.git(&["remote", "add", "team", "https://invalid.example/first"])?;
        let observed = git::remotes(r.path())?
            .into_iter()
            .find(|r| r.name == "team")
            .ok_or("remote")?;
        let captured = Target::capture(r.path(), &observed)?;
        r.git(&[
            "remote",
            "set-url",
            "team",
            "https://invalid.example/second",
        ])?;
        assert!(captured.guard(r.path()).is_err());
        assert!(Target::capture(r.path(), &observed).is_err());
        Ok(())
    }
}
