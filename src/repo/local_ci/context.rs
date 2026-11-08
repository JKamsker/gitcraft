//! Context is captured from the displayed remote and revalidated on the worker.
//! Explicit act environment pins the repository even if Git configuration changes later.
use crate::git::Remote;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Context {
    pub name: String,
    url: String,
    pub repository: String,
}
impl Context {
    pub fn capture(name: &str, remotes: &[Remote]) -> Result<Self, String> {
        if name.is_empty() || name.starts_with('-') || name.chars().any(char::is_control) {
            return Err("Choose a configured context remote".into());
        }
        let remote = remotes
            .iter()
            .find(|r| r.name == name)
            .ok_or("Context remote is missing; refresh repository remotes and choose again")?;
        let path = remote.url.strip_prefix("https://github.com/")
            .or_else(|| remote.url.strip_prefix("ssh://git@github.com/"))
            .or_else(|| remote.url.strip_prefix("git@github.com:"))
            .ok_or("Local Actions currently supports direct GitHub.com HTTPS or git SSH fetch URLs only. Enterprise and other hosts, URL aliases, credentials and custom ports require running act directly with an explicit GitHub instance.")?;
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, repo) = path
            .split_once('/')
            .ok_or("Context URL must identify owner/repository")?;
        if owner.is_empty()
            || repo.is_empty()
            || matches!(repo, "." | "..")
            || !owner
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || !repo
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        {
            return Err("Context URL must identify a plain GitHub owner/repository without URL parameters or credentials".into());
        }
        Ok(Self {
            name: name.into(),
            url: remote.url.clone(),
            repository: path.into(),
        })
    }
    pub fn display(&self) -> String {
        format!(
            "{} · host github.com · repository {} · fetch URL {}",
            self.name, self.repository, self.url
        )
    }
    pub fn revalidate_with(
        &self,
        mut read: impl FnMut(&[&str]) -> Result<String, String>,
    ) -> Result<(), String> {
        // act's go-git reads the first literal local URL. Refuse aliases/includes-only
        // remotes and multiple URLs rather than silently deriving a different context.
        let key = format!("remote.{}.url", self.name);
        let value =
            read(&["config", "--local", "--no-includes", "--get-all", &key]).map_err(|_| {
                "Cannot confirm the local context remote; refresh remotes before running".to_owned()
            })?;
        if value.trim_end_matches('\n') != self.url {
            return Err("Context remote URL changed or is ambiguous. Refresh remotes, review repository context and run again.".into());
        }
        Ok(())
    }
}
