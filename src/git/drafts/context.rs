//! All worktree-sensitive plumbing runs in an empty private Git context, never through
//! repository/global filters, fsmonitor hooks, signing programs or network helpers.
use crate::{git, remote};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Context {
    pub repo: PathBuf,
    pub path: PathBuf,
    env: Vec<(&'static str, String)>,
    pub worktree: String,
    pub id: String,
    pub index_file: String,
    pub shared_file: String,
}

fn prefix(repo: &Path, hooks: Option<&Path>, lazy: bool) -> Vec<String> {
    let null = if remote::is_remote(repo) || !cfg!(windows) {
        "/dev/null"
    } else {
        "NUL"
    };
    let mut args: Vec<String> = ["--no-replace-objects", "--no-optional-locks"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    if lazy {
        args.push("--no-lazy-fetch".into());
    }
    for setting in [
        "core.fsmonitor=false".to_owned(),
        "core.untrackedCache=false".into(),
        "advice.sparseIndexExpanded=false".into(),
        "core.protectNTFS=true".into(),
        "core.protectHFS=true".into(),
        "gc.auto=0".into(),
        "maintenance.auto=false".into(),
        "core.logAllRefUpdates=false".into(),
        "commit.gpgSign=false".into(),
        format!(
            "core.hooksPath={}",
            hooks.map(remote::git_path).unwrap_or_else(|| null.into())
        ),
    ] {
        args.push("-c".into());
        args.push(setting);
    }
    args
}

pub fn configured(repo: &Path) -> Result<bool, String> {
    let mut args = prefix(repo, None, false);
    args.extend(
        [
            "config",
            "--local",
            "--bool",
            "--get",
            "gitcraft.workingDrafts",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    let value = git::draft_bytes(repo, &args, &[], vec![], &[0, 1], &|| true)?;
    Ok(value == b"true\n")
}

pub fn configure(repo: &Path, on: bool) -> Result<(), String> {
    let mut args = prefix(repo, None, false);
    args.extend(
        [
            "config",
            "--local",
            "gitcraft.workingDrafts",
            if on { "true" } else { "false" },
        ]
        .into_iter()
        .map(str::to_owned),
    );
    git::draft_bytes(
        repo,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        &[],
        vec![],
        &[0],
        &|| true,
    )
    .map(|_| ())
}

impl Context {
    pub fn new(repo: &Path, active: &dyn Fn() -> bool) -> Result<Self, String> {
        let initial = |args: &[&str]| -> Result<String, String> {
            let mut pre = prefix(repo, None, true);
            pre.extend(args.iter().map(|s| (*s).to_owned()));
            let bytes = git::draft_bytes(
                repo,
                &pre.iter().map(String::as_str).collect::<Vec<_>>(),
                &[],
                vec![],
                &[0],
                active,
            )?;
            String::from_utf8(bytes)
                .map(|s| s.trim_end_matches(['\n', '\r']).to_owned())
                .map_err(|_| "Draft metadata paths must be UTF-8".into())
        };
        let objects = initial(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ])?;
        let index_file = initial(&["rev-parse", "--path-format=absolute", "--git-path", "index"])?;
        // Keep Git's automatic read mode here: forcing splitIndex=false hides
        // an existing link, while true invents one for ordinary/sparse indexes.
        // No source command writes the index; only the isolated copy is flattened.
        let shared_file = initial(&["rev-parse", "--path-format=absolute", "--shared-index-path"])?;
        let format = initial(&["rev-parse", "--show-object-format"])?;
        if !matches!(format.as_str(), "sha1" | "sha256") {
            return Err("Unknown Git object format".into());
        }
        let worktree = initial(&["rev-parse", "--show-toplevel"])?;
        let own = initial(&["rev-parse", "--absolute-git-dir"])?;
        let common = initial(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let key = if own == common {
            "main".to_owned()
        } else {
            own.strip_prefix(&(common + "/"))
                .ok_or("Draft worktree directory is not inside the common Git directory")?
                .to_owned()
        };
        let id = format!("{:x}", md5::compute(key));
        let path = private_dir(repo, active)?;
        let mut context = Self {
            repo: repo.to_owned(),
            path,
            env: Vec::new(),
            worktree,
            id,
            index_file,
            shared_file,
        };
        let empty = remote::git_path(&context.path.join("empty"));
        context.env = vec![
            ("GIT_CONFIG_GLOBAL", empty.clone()),
            ("GIT_CONFIG_SYSTEM", empty),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("GIT_ATTR_NOSYSTEM", "1".into()),
            ("GIT_NO_LAZY_FETCH", "1".into()),
        ];
        let mut init = prefix(repo, Some(&context.path.join("hooks")), true);
        init.extend([
            "init".to_owned(),
            "--bare".into(),
            "--template=".into(),
            format!("--object-format={format}"),
            remote::git_path(&context.path),
        ]);
        git::draft_bytes(
            repo,
            &init.iter().map(String::as_str).collect::<Vec<_>>(),
            &context.env,
            vec![],
            &[0],
            active,
        )?;
        context.env.extend([
            ("GIT_OBJECT_DIRECTORY", objects),
            (
                "GIT_INDEX_FILE",
                remote::git_path(&context.path.join("index")),
            ),
            ("GIT_WORK_TREE", context.worktree.clone()),
        ]);
        Ok(context)
    }
    pub fn source(
        &self,
        args: &[&str],
        input: Vec<u8>,
        ok: &[i32],
        active: &dyn Fn() -> bool,
    ) -> Result<Vec<u8>, String> {
        let mut pre = prefix(&self.repo, Some(&self.path.join("hooks")), true);
        pre.extend(args.iter().map(|s| (*s).to_owned()));
        git::draft_bytes(
            &self.repo,
            &pre.iter().map(String::as_str).collect::<Vec<_>>(),
            &[],
            input,
            ok,
            active,
        )
    }
    pub fn run(
        &self,
        args: &[&str],
        input: Vec<u8>,
        ok: &[i32],
        active: &dyn Fn() -> bool,
    ) -> Result<Vec<u8>, String> {
        let mut pre = prefix(&self.path, Some(&self.path.join("hooks")), true);
        pre.extend(args.iter().map(|s| (*s).to_owned()));
        git::draft_bytes(
            &self.path,
            &pre.iter().map(String::as_str).collect::<Vec<_>>(),
            &self.env,
            input,
            ok,
            active,
        )
    }
    pub fn text(
        &self,
        args: &[&str],
        input: Vec<u8>,
        active: &dyn Fn() -> bool,
    ) -> Result<String, String> {
        String::from_utf8(self.run(args, input, &[0], active)?)
            .map(|s| s.trim_end_matches(['\n', '\r']).into())
            .map_err(|_| "Draft Git output is not UTF-8".into())
    }
    /// Git apply resolves relative paths from cwd, even with an explicit work tree.
    /// Use the real root as cwd but retain the empty private config and private index.
    pub fn work(
        &self,
        args: &[&str],
        input: Vec<u8>,
        ok: &[i32],
        active: &dyn Fn() -> bool,
    ) -> Result<Vec<u8>, String> {
        let root = match remote::parse(&self.repo) {
            Some(t) => remote::encode(&t.host, &self.worktree),
            None => PathBuf::from(&self.worktree),
        };
        let mut env = self.env.clone();
        env.push(("GIT_DIR", remote::git_path(&self.path)));
        let mut pre = prefix(&root, Some(&self.path.join("hooks")), true);
        pre.extend(args.iter().map(|s| (*s).to_owned()));
        git::draft_bytes(
            &root,
            &pre.iter().map(String::as_str).collect::<Vec<_>>(),
            &env,
            input,
            ok,
            active,
        )
    }
    pub fn blob(&self, bytes: Vec<u8>, active: &dyn Fn() -> bool) -> Result<String, String> {
        self.text(
            &["hash-object", "--no-filters", "-w", "--stdin"],
            bytes,
            active,
        )
    }
}

fn private_dir(repo: &Path, active: &dyn Fn() -> bool) -> Result<PathBuf, String> {
    if let Some(target) = remote::parse(repo) {
        let request = remote::Request { cwd: Some(target.path), script: "umask 077; d=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-drafts.XXXXXXXX\") || exit 1; mkdir \"$d/hooks\" && : >\"$d/empty\" || exit 1; cd -P -- \"$d\" && pwd -P".into(), ..Default::default() };
        let out = git::forecast_remote(&target.host, request, active)?;
        let path = String::from_utf8(out.stdout).map_err(|_| "Invalid draft temporary path")?;
        let path = path.trim_end_matches(['\n', '\r']);
        if out.code != 0
            || !path.starts_with('/')
            || path.contains(['\n', '\r', '\0'])
            || !path
                .rsplit('/')
                .next()
                .is_some_and(|n| n.starts_with("gitcraft-drafts."))
        {
            return Err("Could not prepare private draft directory".into());
        }
        return Ok(remote::encode(&target.host, path));
    }
    static ID: AtomicU64 = AtomicU64::new(0);
    let root = std::fs::canonicalize(std::env::temp_dir()).map_err(|e| e.to_string())?;
    for _ in 0..100 {
        let path = root.join(format!(
            "gitcraft-drafts.{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&path) {
            Ok(()) => {
                std::fs::create_dir(path.join("hooks")).map_err(|e| e.to_string())?;
                std::fs::write(path.join("empty"), b"").map_err(|e| e.to_string())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Could not prepare private draft directory".into())
}
impl Drop for Context {
    fn drop(&mut self) {
        if let Some(target) = remote::parse(&self.path) {
            std::thread::spawn(move || {
                let _ = git::forecast_remote(
                    &target.host,
                    remote::Request {
                        script: format!("rm -rf -- {}", remote::sh_path(&target.path)),
                        ..Default::default()
                    },
                    &|| true,
                );
            });
        } else {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
