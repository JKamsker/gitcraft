//! Bounded non-fetching recovery reads and create-only mutation dispatch.

use super::*;

pub(in crate::git) struct Reader<'a> {
    pub repo: &'a Path,
    pub(super) active: &'a dyn Fn() -> bool,
    pub(super) began: Instant,
}

impl<'a> Reader<'a> {
    pub fn new(repo: &'a Path, active: &'a dyn Fn() -> bool) -> Self {
        Self {
            repo,
            active,
            began: Instant::now(),
        }
    }
    pub fn active(&self) -> bool {
        (self.active)() && self.began.elapsed() < Duration::from_secs(120)
    }
    pub fn output(&self, args: &[&str], input: Option<&[u8]>) -> Result<remote::Output, String> {
        let null = if remote::is_remote(self.repo) || !cfg!(windows) {
            "/dev/null"
        } else {
            "NUL"
        };
        let hooks = format!("core.hooksPath={null}");
        let mut pre = vec![
            "--no-lazy-fetch",
            "--no-replace-objects",
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            &hooks,
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "-c",
            "commit.gpgSign=false",
        ];
        pre.extend_from_slice(args);
        let remaining = Duration::from_secs(120).saturating_sub(self.began.elapsed());
        git::command::recovery_output(
            self.repo,
            &pre,
            input,
            &|| self.active(),
            remaining.min(Duration::from_secs(30)),
        )
    }
    pub fn run(&self, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let out = self.output(args, input).map_err(|error| {
            if args.first() == Some(&"update-ref") && (error.to_ascii_lowercase().contains("cancel") || error.contains("timed out")) {
                crate::i18n::message("backend-ref-recovery-was-interrupted-and-may-have-completed-inspect-the-b-333dce", &[("detail", (error).to_string().into())])
            } else { error }
        })?;
        if out.code == 0 && out.stderr.is_empty() {
            Ok(out.stdout)
        } else {
            Err(crate::i18n::message(
                "backend-content-recovery-git-failed-bb7f44",
                &[
                    ("code", (out.code).to_string().into()),
                    (
                        "from_utf8_lossy",
                        (String::from_utf8_lossy(&out.stderr)
                            .chars()
                            .take(2000)
                            .collect::<String>())
                        .to_string()
                        .into(),
                    ),
                ],
            ))
        }
    }
    pub fn identity(&self) -> Result<Identity, String> {
        let text = |args: &[&str]| -> Result<String, String> {
            let bytes = self.run(args, None)?;
            let bytes = bytes.strip_suffix(b"\n").ok_or(crate::i18n::text(
                "backend-truncated-repository-metadata-04adb4",
            ))?;
            let bytes = if cfg!(windows) && !remote::is_remote(self.repo) {
                bytes.strip_suffix(b"\r").unwrap_or(bytes)
            } else {
                bytes
            };
            let value = std::str::from_utf8(bytes).map_err(|_| {
                crate::i18n::text("backend-repository-metadata-is-not-utf-8-aa6912")
            })?;
            if value.chars().any(char::is_control) {
                return Err(
                    crate::i18n::text("backend-repository-metadata-paths-contain-unsupported-control-characters-37b6ea").into(),
                );
            }
            Ok(value.to_owned())
        };
        let bare = text(&["rev-parse", "--is-bare-repository"])? == "true";
        let mut identity = Identity {
            host: remote::parse(self.repo).map(|target| target.host),
            root_pin: None,
            common: text(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?,
            root: if bare {
                None
            } else {
                Some(text(&["rev-parse", "--show-toplevel"])?)
            },
            format: text(&["rev-parse", "--show-object-format"])?,
        };
        identity.root_pin = identity
            .root
            .as_deref()
            .and_then(|root| file::capture(self, root).ok());
        if !matches!(identity.format.as_str(), "sha1" | "sha256") {
            return Err(crate::i18n::text("backend-unsupported-object-format-f8189c").into());
        }
        Ok(identity)
    }
}
