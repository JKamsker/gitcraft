pub(crate) struct Fixture {
    pub root: PathBuf,
    pub repo: PathBuf,
    pub receiver: PathBuf,
}
impl Fixture {
    pub(crate) fn new() -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("gitcraft-preflight-{}-{stamp}", std::process::id()));
        let repo = root.join("repo");
        let receiver = root.join("receiver.git");
        std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
        let f = Self {
            root,
            repo,
            receiver,
        };
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@work.example"],
            vec!["config", "commit.gpgsign", "false"],
            vec!["config", "core.autocrlf", "false"],
        ] {
            f.git(&args)?;
        }
        f.commit("base.txt", "base\n", "Base")?;
        git::run(
            None,
            &["init", "--bare", "-q", &f.receiver.to_string_lossy()],
        )?;
        f.git(&["remote", "add", "origin", &f.receiver.to_string_lossy()])?;
        f.git(&["push", "-qu", "origin", "main"])?;
        Ok(f)
    }
    pub(crate) fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(&self.repo), args).map(|s| s.trim_end().to_owned())
    }
    pub(crate) fn commit(&self, path: &str, text: &str, message: &str) -> Result<String, String> {
        std::fs::write(self.repo.join(path), text).map_err(|e| e.to_string())?;
        self.git(&["add", "--", path])?;
        self.git(&["commit", "-qm", message])?;
        self.git(&["rev-parse", "HEAD"])
    }
    pub(crate) fn profile(&self) -> UserProfile {
        UserProfile {
            author_name: "Fixture".into(),
            author_email: "fixture@work.example".into(),
            ..Default::default()
        }
    }
    pub(crate) fn plan(&self, profile: UserProfile) -> Result<Plan, String> {
        scan(&self.repo, vec![vec!["push".into()]], profile, &|| true)
    }
    pub(crate) fn receiver_head(&self) -> Result<String, String> {
        git::run(Some(&self.receiver), &["rev-parse", "refs/heads/main"]).map(|s| s.trim().into())
    }
    pub(crate) fn advance_receiver(&self) -> Result<String, String> {
        let before = self.receiver_head()?;
        let tree = git::run(
            Some(&self.receiver),
            &["rev-parse", &format!("{before}^{{tree}}")],
        )?;
        let new = git::run(
            Some(&self.receiver),
            &[
                "-c",
                "user.name=Other",
                "-c",
                "user.email=other@example.org",
                "commit-tree",
                tree.trim(),
                "-p",
                &before,
                "-m",
                "Remote work",
            ],
        )?;
        git::run(
            Some(&self.receiver),
            &["update-ref", "refs/heads/main", new.trim(), &before],
        )?;
        Ok(new.trim().into())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
