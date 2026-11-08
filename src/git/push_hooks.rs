//! Preserve the original remote name given to pre-push hooks while the transport uses a
//! private, immutable remote alias. Hook contents are neither read nor changed.

use crate::{git, remote};
use std::path::{Path, PathBuf};

pub(super) struct Upload<'a> {
    pub alias: &'a str,
    pub skip_original: bool,
    pub objects: &'a [String],
    pub source: &'a str,
    pub destination: &'a str,
    pub remote_tip: Option<&'a str>,
}

pub(super) struct Hooks {
    dir: PathBuf,
}
impl Hooks {
    pub(super) fn new(repo: &Path, name: &str, upload: Upload<'_>) -> Result<Self, String> {
        let original = git::forecast_run(
            repo,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "hooks/pre-push",
            ],
            &[],
            &[0],
            &|| true,
        )
        .map_err(|_| "Could not locate the configured pre-push hook")?;
        let original = original.trim_end();
        let original_dir = original.rsplit_once('/').map(|v| v.0).unwrap_or(original);
        let mut payload =
            String::from("refs_file=\"$(dirname \"$0\")/refs\"\ncat >\"$refs_file\" || exit 1\n");
        // Let the original hook veto the update before our wrapper can upload LFS
        // objects. Keep its remote name/address and the exact advertised ref input.
        if !upload.skip_original {
            payload.push_str(&format!(
                "if [ -x {} ]; then {} {} \"$2\" <\"$refs_file\" || exit $?; fi\n",
                remote::sh_quote(original),
                remote::sh_quote(original),
                remote::sh_quote(name)
            ));
        }
        if !upload.objects.is_empty() {
            let remote = upload.remote_tip.map(|tip| {
                if tip.is_empty() {
                    "0".repeat(upload.source.len())
                } else {
                    tip.to_owned()
                }
            });
            let test = remote
                .map(|r| format!(" && [ \"$remote_oid\" = {} ]", remote::sh_quote(&r)))
                .unwrap_or_default();
            payload.push_str(&format!("valid=0\nwhile read -r local_ref local_oid remote_ref remote_oid; do\nif [ \"$local_oid\" = {} ] && [ \"$remote_ref\" = {} ]{test}; then valid=1; fi\ndone <\"$refs_file\"\n[ \"$valid\" = 1 ] || exit 0\ngit lfs push --object-id {} {} </dev/null || exit 1\n",remote::sh_quote(upload.source),remote::sh_quote(upload.destination),remote::sh_quote(upload.alias),upload.objects.iter().map(|o|remote::sh_quote(o)).collect::<Vec<_>>().join(" ")));
        }
        let wrapper = format!(
            "#!/bin/sh\nn=${{GIT_CONFIG_COUNT:-0}}\nexec env \"GIT_CONFIG_KEY_${{n}}=core.hooksPath\" \"GIT_CONFIG_VALUE_${{n}}=\"{} \"GIT_CONFIG_COUNT=$((n+1))\" sh -c {} \"$0\" \"$@\"\n",
            remote::sh_quote(original_dir),
            remote::sh_quote(&payload)
        );
        if let Some(target) = remote::parse(repo) {
            let out = git::forecast_remote(
                &target.host,
                remote::Request {
                    script: "umask 077; d=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-push-hooks.XXXXXXXX\") || exit 1; cat >\"$d/pre-push\" && chmod 700 \"$d/pre-push\" || exit 1; printf '%s' \"$d\"".into(),
                    stdin: Some(wrapper.into_bytes()),
                    ..Default::default()
                },
                &|| true,
            )?;
            let path = String::from_utf8(out.stdout).map_err(|_| "Invalid hook temporary path")?;
            if out.code != 0
                || !path.starts_with('/')
                || path.contains(['\n', '\r', '\0'])
                || !path
                    .rsplit('/')
                    .next()
                    .is_some_and(|n| n.starts_with("gitcraft-push-hooks."))
            {
                return Err("Could not prepare the pre-push hook".into());
            }
            return Ok(Self {
                dir: remote::encode(&target.host, &path),
            });
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "No clock")?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-push-hooks-{}-{stamp}",
            std::process::id()
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&dir)
            .map_err(|_| "Could not prepare the pre-push hook")?;
        let this = Self { dir };
        let hook = this.dir.join("pre-push");
        std::fs::write(&hook, wrapper).map_err(|_| "Could not prepare the pre-push hook")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "Could not prepare the pre-push hook")?;
        }
        Ok(this)
    }
    pub(super) fn path(&self) -> String {
        remote::git_path(&self.dir)
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        if let Some(target) = remote::parse(&self.dir) {
            {
                let _ = git::forecast_remote(
                    &target.host,
                    remote::Request {
                        script: format!("rm -rf -- {}", remote::sh_path(&target.path)),
                        ..Default::default()
                    },
                    &|| true,
                );
            }
        } else {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
