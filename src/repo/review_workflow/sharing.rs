//! Captured Git-ref sharing destination, private to review artifacts. This is independent
//! of forge account/profile schemas: Git credentials remain with the existing runner.
use crate::{git, network};
use std::path::Path;
#[derive(Clone)]
pub(super) struct Target {
    pub name: String,
    fetch: String,
    push: String,
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
        if !git::remotes(repo)?.iter().any(|r| r == observed) {
            return Err("Sharing remote changed; refresh repository remotes and try again".into());
        }
        Ok(Self {
            name: observed.name.clone(),
            fetch: url(repo, &observed.name, false)?,
            push: url(repo, &observed.name, true)?,
        })
    }
    pub fn guard(&self, repo: &Path) -> Result<(), String> {
        if let Some(p) = network::refused() {
            return Err(p.why().into());
        }
        if url(repo, &self.name, false)? != self.fetch || url(repo, &self.name, true)? != self.push
        {
            return Err("Sharing destination changed during the operation; nothing was sent to the replacement destination".into());
        }
        Ok(())
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
#[cfg(test)]
mod tests {
    use super::*;
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
