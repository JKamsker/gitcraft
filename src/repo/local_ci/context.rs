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
            return Err(crate::i18n::text("repo-workflows-local_ci-capture-3a6208be").into());
        }
        let remote = remotes
            .iter()
            .find(|r| r.name == name)
            .ok_or(crate::i18n::text(
                "repo-workflows-local_ci-capture-06580fa5",
            ))?;
        let path = remote
            .url
            .strip_prefix("https://github.com/")
            .or_else(|| remote.url.strip_prefix("ssh://git@github.com/"))
            .or_else(|| remote.url.strip_prefix("git@github.com:"))
            .ok_or(crate::i18n::text(
                "repo-workflows-local_ci-capture-eb158ced",
            ))?;
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, repo) = path.split_once('/').ok_or(crate::i18n::text(
            "repo-workflows-local_ci-capture-cbef0bd4",
        ))?;
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
            return Err(crate::i18n::text("repo-workflows-local_ci-capture-534b1c84").into());
        }
        Ok(Self {
            name: name.into(),
            url: remote.url.clone(),
            repository: path.into(),
        })
    }
    pub fn display(&self) -> String {
        crate::i18n::message(
            "repo-workflows-local_ci-display-8dc95fe7",
            &[
                ("self-name-1", (self.name).clone().into()),
                ("self-repository-2", (self.repository).clone().into()),
                ("self-url-3", (self.url).clone().into()),
            ],
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
                crate::i18n::text("repo-workflows-local_ci-revalidate-with-e02bce32").to_owned()
            })?;
        if value.trim_end_matches('\n') != self.url {
            return Err(
                crate::i18n::text("repo-workflows-local_ci-revalidate-with-d12b2aa8").into(),
            );
        }
        Ok(())
    }
}
