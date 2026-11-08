//! A provider-neutral command source. Captured configuration and refs live in a private
//! Git directory; commands share only the original object store and working files. Later
//! profile, remote-name and URL-rewrite edits cannot reinterpret the captured routing.
use super::{Extra, run, run_ok_codes};
use crate::remote;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MARKER: &str = "gitcraft.commandSource=";
static SOURCES: OnceLock<Mutex<HashMap<String, Weak<CommandSource>>>> = OnceLock::new();
static IDS: AtomicU64 = AtomicU64::new(0);

type ExecutionGuard = Box<dyn Fn() -> Result<(), String> + Send + Sync>;
pub struct CommandSource {
    id: String,
    repo: PathBuf,
    directory: PathBuf,
    env: Vec<(&'static str, String)>,
    configuration: String,
    initial_refs: Vec<(String, String)>,
    execution_guard: OnceLock<ExecutionGuard>,
}
impl CommandSource {
    pub fn capture(repo: &Path) -> Result<Arc<Self>, String> {
        let before = run(Some(repo), &["config", "--null", "--list"])?;
        if before.len() > 4 * 1024 * 1024 {
            return Err("Git configuration exceeds the 4MiB command-source budget".into());
        }
        let mut entries = parse_config(&before)?;
        let remotes = run(Some(repo), &["remote"])?;
        if remotes.lines().count() > 200 {
            return Err("More than 200 Git remotes; narrow the command source".into());
        }
        let mut endpoints = Vec::new();
        for name in remotes.lines() {
            for (push, key) in [(false, "url"), (true, "pushurl")] {
                let args = if push {
                    vec!["remote", "get-url", "--push", "--all", name]
                } else {
                    vec!["remote", "get-url", "--all", name]
                };
                for url in run(Some(repo), &args)?.lines() {
                    endpoints.push((format!("remote.{name}.{key}"), url.to_owned()));
                }
            }
        }
        entries = freeze_routing(entries, endpoints);
        let objects = absolute_git_path(repo, "objects")?;
        let index = absolute_git_path(repo, "index")?;
        let hooks = absolute_git_path(repo, "hooks")?;
        let bare = run(Some(repo), &["rev-parse", "--is-bare-repository"])?;
        let worktree = if bare.trim() == "true" {
            None
        } else {
            Some(
                run(Some(repo), &["rev-parse", "--show-toplevel"])?
                    .trim_end_matches(['\n', '\r'])
                    .to_owned(),
            )
        };
        let format = run(Some(repo), &["rev-parse", "--show-object-format"])?;
        let listing = run(
            Some(repo),
            &["for-each-ref", "--format=%(objectname) %(refname)"],
        )?;
        if listing.len() > 16 * 1024 * 1024 || listing.lines().count() > 100_000 {
            return Err("Git ref snapshot exceeds its finite command-source budget".into());
        }
        let head = run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
        let head = if head.trim().is_empty() {
            run(Some(repo), &["rev-parse", "HEAD"])?
        } else {
            format!("ref: {}\n", head.trim())
        };
        let config = render_config(entries, worktree.as_deref(), &hooks, format.trim())?;
        if run(Some(repo), &["config", "--null", "--list"])? != before {
            return Err("Git routing/profile changed during capture; try again".into());
        }
        let id = format!(
            "{:x}",
            md5::compute(format!(
                "{}:{}:{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
                IDS.fetch_add(1, Ordering::Relaxed)
            ))
        );
        let directory = private_directory(repo, &id)?;
        let source = Arc::new(Self {
            id,
            repo: repo.to_owned(),
            env: source_env(&directory, &objects, &index, worktree.as_deref()),
            directory,
            configuration: before,
            initial_refs: listing
                .lines()
                .filter_map(|l| {
                    l.split_once(' ')
                        .map(|(oid, r)| (r.to_owned(), oid.to_owned()))
                })
                .collect(),
            execution_guard: OnceLock::new(),
        });
        // The private directory has no links or repository-supplied templates. No init,
        // checkout, filter, hook or extraction command participates in preparation.
        for dir in ["objects", "refs"] {
            remote::create_dir_all(&remote::join(&source.directory, dir))
                .map_err(|e| e.to_string())?;
        }
        remote::write(
            &remote::join(&source.directory, "config"),
            config.as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        remote::write(&remote::join(&source.directory, "HEAD"), head.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut packed = "# pack-refs with: sorted\n".to_owned();
        for line in listing.lines() {
            let (oid, reference) = line.split_once(' ').ok_or("Malformed captured ref")?;
            if !safe_reference(reference) || !object_id(oid) {
                return Err("Invalid captured ref identity".into());
            }
            packed.push_str(line);
            packed.push('\n');
        }
        remote::write(
            &remote::join(&source.directory, "packed-refs"),
            packed.as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        SOURCES
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "Command source registry unavailable")?
            .insert(source.id.clone(), Arc::downgrade(&source));
        Ok(source)
    }
    /// Additional source-specific evidence must still hold inside the operation worker.
    pub fn guard_execution(
        &self,
        guard: impl Fn() -> Result<(), String> + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.execution_guard
            .set(Box::new(guard))
            .map_err(|_| "Execution evidence was already bound".into())
    }
    fn reconcile_push(&self) -> Result<(), String> {
        self.check_context()?;
        let now = self.refs("refs/remotes/")?;
        let mut transaction = "start\n".to_owned();
        let mut changes = 0;
        for (reference, oid) in &now {
            let old = self
                .initial_refs
                .iter()
                .find(|(r, _)| r == reference)
                .map(|(_, oid)| oid.clone())
                .unwrap_or_else(|| "0".repeat(oid.len()));
            if &old != oid {
                transaction.push_str(&format!("update {reference} {oid} {old}\n"));
                changes += 1;
            }
        }
        if changes > 0 {
            transaction.push_str("prepare\ncommit\n");
            super::run_input(Some(&self.repo), &["update-ref", "--stdin"], &transaction)?;
        }
        // `push -u` wrote the private config. Preserve its normal upstream effect only
        // after successful publication, with the original routing still matching capture.
        let before = branch_tracking(parse_config(&self.configuration)?);
        let args = self.bind(&["config", "--null", "--list"]);
        let after = branch_tracking(parse_config(&run(
            Some(&self.repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?)?);
        for (key, values) in after {
            if before.get(&key) == Some(&values) {
                continue;
            }
            let Some((first, rest)) = values.split_first() else {
                continue;
            };
            run(
                Some(&self.repo),
                &["config", "--local", "--replace-all", &key, first],
            )?;
            for value in rest {
                run(
                    Some(&self.repo),
                    &["config", "--local", "--add", &key, value],
                )?;
            }
        }
        Ok(())
    }
    pub fn check_context(&self) -> Result<(), String> {
        if run(Some(&self.repo), &["config", "--null", "--list"])? != self.configuration {
            return Err("Git routing or profile changed during the operation. A completed transfer may already have affected the captured destination; refresh before retrying.".into());
        }
        Ok(())
    }
    /// Validate advisory evidence at the actual reviewed-push dispatch. That transport
    /// already pins its object and endpoint, so it must retain its own command environment.
    pub(crate) fn check_execution(&self) -> Result<(), String> {
        self.check_context()?;
        if let Some(guard) = self.execution_guard.get() {
            guard()?;
        }
        Ok(())
    }
    pub(crate) fn reviewed_binding(repo: &Path, args: &[String]) -> Result<Arc<Self>, String> {
        let words: Vec<_> = args.iter().map(String::as_str).collect();
        resolve(Some(repo), &words)?.ok_or("Reviewed echo source is missing".into())
    }
    pub fn owns(&self, args: &[String]) -> bool {
        args.windows(2)
            .any(|pair| pair[0] == "-c" && pair[1] == format!("{MARKER}{}", self.id))
    }
    pub fn bind(&self, args: &[&str]) -> Vec<String> {
        [
            vec!["-c".into(), format!("{MARKER}{}", self.id)],
            args.iter().map(|s| (*s).to_owned()).collect(),
        ]
        .concat()
    }
    /// The captured Git directory's ref value, without consulting a mutable remote name.
    pub fn reference(&self, reference: &str) -> Result<Option<String>, String> {
        if !safe_reference(reference) {
            return Err("Invalid captured ref name".into());
        }
        let args = self.bind(&["rev-parse", "--verify", "--end-of-options", reference]);
        let out = run_ok_codes(
            Some(&self.repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            &[0, 128],
        )?;
        let oid = out.trim();
        Ok(object_id(oid).then(|| oid.to_owned()))
    }
    pub fn refs(&self, root: &str) -> Result<Vec<(String, String)>, String> {
        let args = self.bind(&["for-each-ref", "--format=%(objectname) %(refname)", root]);
        run(
            Some(&self.repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?
        .lines()
        .map(|line| {
            let (oid, reference) = line.split_once(' ').ok_or("Invalid captured ref listing")?;
            if !object_id(oid) || !safe_reference(reference) {
                return Err("Invalid captured ref identity".into());
            }
            Ok((reference.to_owned(), oid.to_owned()))
        })
        .collect()
    }
}
impl Drop for CommandSource {
    fn drop(&mut self) {
        if let Some(sources) = SOURCES.get()
            && let Ok(mut sources) = sources.lock()
        {
            sources.remove(&self.id);
        }
        let directory = self.directory.clone();
        if let Ok(pool) = crate::tasks::pool() {
            let _ = pool.submit(
                crate::tasks::Cancellation::default(),
                crate::tasks::Priority::Background,
                None,
                Box::new(move || {
                    if let Some(target) = remote::parse(&directory) {
                        let script = format!(
                            "p={}; [ ! -L \"$p\" ] && [ -O \"$p\" ] || exit 73; rm -rf -- \"$p\"",
                            remote::sh_quote(&target.path)
                        );
                        let _ = remote::exec(
                            &target.host,
                            &remote::Request {
                                script,
                                timeout: Some(Duration::from_secs(10)),
                                ..Default::default()
                            },
                        );
                    } else {
                        let _ = std::fs::remove_dir_all(directory);
                    }
                }),
            );
        }
    }
}
fn private_directory(repo: &Path, id: &str) -> Result<PathBuf, String> {
    if let Some(target) = remote::parse(repo) {
        let out = remote::exec(
            &target.host,
            &remote::Request {
                script: "umask 077; mktemp -d \"${TMPDIR:-/tmp}/gitcraft-source.XXXXXXXXXX\""
                    .into(),
                timeout: Some(Duration::from_secs(10)),
                ..Default::default()
            },
        )?;
        let path = String::from_utf8(out.stdout).map_err(|e| e.to_string())?;
        let path = path.trim();
        if out.code != 0
            || !path.starts_with('/')
            || !path.rsplit('/').next().is_some_and(|s| {
                s.starts_with("gitcraft-source.")
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
            })
        {
            return Err("Could not create a private command-source directory".into());
        }
        Ok(remote::encode(&target.host, path))
    } else {
        let path = std::env::temp_dir().join(format!("gitcraft-source-{id}"));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|e| e.to_string())?;
        Ok(path)
    }
}
fn absolute_git_path(repo: &Path, name: &str) -> Result<String, String> {
    Ok(run(
        Some(repo),
        &["rev-parse", "--path-format=absolute", "--git-path", name],
    )?
    .trim_end_matches(['\n', '\r'])
    .to_owned())
}
fn source_env(
    directory: &Path,
    objects: &str,
    index: &str,
    worktree: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut env = vec![
        ("GIT_DIR", remote::git_path(directory)),
        ("GIT_OBJECT_DIRECTORY", objects.into()),
        ("GIT_INDEX_FILE", index.into()),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
        (
            "GIT_CONFIG_GLOBAL",
            remote::git_path(&remote::join(directory, "empty-global")),
        ),
        (
            "GIT_CONFIG_SYSTEM",
            remote::git_path(&remote::join(directory, "empty-system")),
        ),
    ];
    if let Some(path) = worktree {
        env.push(("GIT_WORK_TREE", path.into()));
    }
    env
}
pub(super) fn bind(
    extra: &mut Extra,
    repo: Option<&Path>,
    args: &[&str],
) -> Result<Option<Arc<CommandSource>>, String> {
    let Some(source) = resolve(repo, args)? else {
        return Ok(None);
    };
    if super::goes_over_network(args) {
        source.check_execution()?;
    }
    extra.unset.extend([
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG",
        "GIT_COMMON_DIR",
        "GIT_NAMESPACE",
        "GIT_SHALLOW_FILE",
        "GIT_REPLACE_REF_BASE",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ]);
    extra.env.extend(source.env.clone());
    Ok(Some(source))
}
fn resolve(repo: Option<&Path>, args: &[&str]) -> Result<Option<Arc<CommandSource>>, String> {
    let ids: Vec<_> = args
        .windows(2)
        .filter_map(|pair| {
            (pair[0] == "-c")
                .then(|| pair[1].strip_prefix(MARKER))
                .flatten()
        })
        .collect();
    if ids.is_empty() {
        return Ok(None);
    }
    if ids.len() != 1 {
        return Err("Ambiguous captured command source".into());
    }
    let source = SOURCES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Command source registry unavailable")?
        .get(ids[0])
        .and_then(Weak::upgrade)
        .ok_or("Captured command source expired; prepare the operation again")?;
    if repo != Some(source.repo.as_path()) {
        return Err("Captured command source belongs to another repository".into());
    }
    Ok(Some(source))
}
pub(super) fn finish(
    source: Option<&Arc<CommandSource>>,
    args: &[&str],
    success: bool,
) -> Result<(), String> {
    if success
        && super::after_options(args).first() == Some(&"push")
        && let Some(source) = source
    {
        source.reconcile_push().map_err(|e| format!("Push completed at the captured destination, but local tracking metadata could not be reconciled: {e}. Refresh before retrying."))?;
    }
    Ok(())
}
fn branch_tracking(
    entries: Vec<(String, String)>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut map: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (key, value) in entries {
        let lower = key.to_ascii_lowercase();
        if lower.starts_with("branch.") && (lower.ends_with(".remote") || lower.ends_with(".merge"))
        {
            map.entry(key).or_default().push(value);
        }
    }
    map
}
fn freeze_routing(
    mut entries: Vec<(String, String)>,
    endpoints: Vec<(String, String)>,
) -> Vec<(String, String)> {
    entries.retain(|(key, _)| {
        let lower = key.to_ascii_lowercase();
        !(lower.starts_with("url.")
            || lower.starts_with("include.")
            || lower.starts_with("includeif.")
            || (lower.starts_with("remote.")
                && (lower.ends_with(".url") || lower.ends_with(".pushurl"))))
    });
    entries.extend(endpoints);
    entries
}
fn parse_config(config: &str) -> Result<Vec<(String, String)>, String> {
    config
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(|row| {
            let (key, value) = row.split_once('\n').unwrap_or((row, "true"));
            if key.is_empty() {
                return Err("Invalid captured Git configuration key".into());
            }
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect()
}
fn quote(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
            .replace('\u{8}', "\\b")
    )
}
fn render_config(
    mut entries: Vec<(String, String)>,
    worktree: Option<&str>,
    hooks: &str,
    object_format: &str,
) -> Result<String, String> {
    if !matches!(object_format, "sha1" | "sha256") {
        return Err("Unsupported Git object format".into());
    }
    entries.retain(|(k, _)| {
        !matches!(
            k.to_ascii_lowercase().as_str(),
            "core.bare"
                | "core.worktree"
                | "core.hookspath"
                | "core.repositoryformatversion"
                | "extensions.worktreeconfig"
                | "extensions.objectformat"
        )
    });
    entries.extend([
        ("core.bare".into(), worktree.is_none().to_string()),
        ("core.hooksPath".into(), hooks.into()),
        (
            "core.repositoryFormatVersion".into(),
            if object_format == "sha256" { "1" } else { "0" }.into(),
        ),
    ]);
    if object_format == "sha256" {
        entries.push(("extensions.objectFormat".into(), "sha256".into()));
    }
    if let Some(path) = worktree {
        entries.push(("core.worktree".into(), path.into()));
    }
    let mut out = String::new();
    for (key, value) in entries {
        let (section, rest) = key
            .split_once('.')
            .ok_or("Invalid captured config section")?;
        let (subsection, variable) = rest
            .rsplit_once('.')
            .map_or((None, rest), |(s, v)| (Some(s), v));
        if !section
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !variable
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.contains('\0')
        {
            return Err("Invalid captured configuration value".into());
        }
        out.push_str(&format!(
            "[{section}{}]\n\t{variable} = {}\n",
            subsection
                .map(|s| format!(" {}", quote(s)))
                .unwrap_or_default(),
            quote(&value)
        ));
    }
    Ok(out)
}
fn safe_reference(reference: &str) -> bool {
    reference.starts_with("refs/")
        && !reference.contains(['\\', '\0', '\n', '\r'])
        && reference
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
fn object_id(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r2_pure_captured_config_preserves_credentials_and_cannot_include_external_routing()
    -> Result<(), String> {
        let entries = parse_config(
            "credential.https://example.test.helper\nstore --file=/private/key\0http.proxy\nproxy\0core.bare\nfalse\0include.path\n/changed/profile\0url.foreign.insteadOf\nhttps://example.test/\0url.foreign.pushInsteadOf\nhttps://example.test/\0remote.team.url\nold-destination\0",
        )?;
        let entries = freeze_routing(
            entries,
            vec![
                (
                    "remote.team.url".into(),
                    "https://example.test/captured-fetch".into(),
                ),
                (
                    "remote.team.pushurl".into(),
                    "https://example.test/captured-push".into(),
                ),
            ],
        );
        let rendered = render_config(entries, Some("/owned/work"), "/owned/hooks", "sha256")?;
        assert!(rendered.contains("[credential \"https://example.test\"]"));
        assert!(rendered.contains("store --file=/private/key"));
        for forbidden in [
            "insteadOf",
            "pushInsteadOf",
            "/changed/profile",
            "old-destination",
        ] {
            assert!(!rendered.contains(forbidden));
        }
        assert!(rendered.contains("https://example.test/captured-push"));
        assert!(rendered.contains("objectFormat = \"sha256\""));
        assert!(rendered.contains("hooksPath = \"/owned/hooks\""));
        assert!(render_config(vec![], None, "/hooks", "unknown").is_err());
        Ok(())
    }
    #[test]
    fn r2_pure_command_source_env_separates_configuration_refs_and_shared_objects() {
        let env = source_env(
            Path::new("/owned/private"),
            "/original/objects",
            "/original/index",
            Some("/original/work"),
        );
        assert!(env.contains(&("GIT_DIR", "/owned/private".into())));
        assert!(env.contains(&("GIT_OBJECT_DIRECTORY", "/original/objects".into())));
        assert!(env.contains(&("GIT_CONFIG_NOSYSTEM", "1".into())));
        assert!(!safe_reference("refs/../../other"));
        assert!(!object_id("main"));
    }
}
