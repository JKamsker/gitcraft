//! A private bare Git context keeps checkout, index, info and global attributes out of the
//! forecast. Only the simulated parent tree supplies attributes. Objects stay in the source
//! repository so cached trees survive this short-lived context; source refs are never written.

use std::path::{Path, PathBuf};

use crate::{git, remote};

pub(super) struct ForecastGit {
    path: PathBuf,
    env: Vec<(&'static str, String)>,
}

impl ForecastGit {
    pub(super) fn new(repo: &Path, active: &dyn Fn() -> bool) -> Result<Self, String> {
        let objects = git::forecast_run(
            repo,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "objects",
            ],
            &[],
            &[0],
            active,
        )?;
        let format = git::forecast_run(
            repo,
            &["rev-parse", "--show-object-format"],
            &[],
            &[0],
            active,
        )?;
        if !matches!(format.trim(), "sha1" | "sha256") {
            return Err("Unknown repository object format".into());
        }
        let path = if let Some(target) = remote::parse(repo) {
            let out = git::forecast_remote(&target.host, remote::Request {
                script: "umask 077; d=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-weather.XXXXXXXX\") || exit 1; : >\"$d/empty\"; printf '%s' \"$d\"".into(),
                ..Default::default()
            }, active)?;
            if out.code != 0 {
                return Err("Could not prepare forecast context".into());
            }
            let dir =
                String::from_utf8(out.stdout).map_err(|_| "Invalid forecast temporary path")?;
            if !dir.starts_with('/')
                || dir.contains(['\n', '\r', '\0'])
                || !dir
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.starts_with("gitcraft-weather."))
            {
                return Err("Invalid forecast temporary path".into());
            }
            remote::encode(&target.host, &dir)
        } else {
            let path = super::private_dir(&std::env::temp_dir()).map_err(|e| e.to_string())?;
            std::fs::write(path.join("empty"), "").map_err(|e| e.to_string())?;
            path
        };
        let empty = remote::git_path(&path.join("empty"));
        let mut context = Self {
            path,
            env: vec![
                ("GIT_CONFIG_GLOBAL", empty),
                ("GIT_CONFIG_NOSYSTEM", "1".into()),
                ("GIT_ATTR_NOSYSTEM", "1".into()),
            ],
        };
        git::forecast_run(
            repo,
            &[
                "-c",
                "init.defaultBranch=forecast",
                "init",
                "--bare",
                "--template=",
                &format!("--object-format={}", format.trim()),
                &remote::git_path(&context.path),
            ],
            &context.env,
            &[0],
            active,
        )?;
        context.env.push((
            "GIT_OBJECT_DIRECTORY",
            objects.trim_end_matches(['\r', '\n']).to_owned(),
        ));
        Ok(context)
    }

    pub(super) fn run(
        &self,
        args: &[&str],
        ok: &[i32],
        active: &dyn Fn() -> bool,
    ) -> Result<String, String> {
        self.bytes(args, ok, active)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    pub(super) fn bytes(
        &self,
        args: &[&str],
        ok: &[i32],
        active: &dyn Fn() -> bool,
    ) -> Result<Vec<u8>, String> {
        let attributes = format!(
            "core.attributesFile={}",
            remote::git_path(&self.path.join("empty"))
        );
        let args: Vec<&str> = ["-c", &attributes]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        git::forecast_bytes(&self.path, &args, &self.env, ok, active)
    }
}

impl Drop for ForecastGit {
    fn drop(&mut self) {
        if let Some(target) = remote::parse(&self.path) {
            // Keep cleanup within the shared worker and transport ownership.
            let _ = git::forecast_remote(
                &target.host,
                remote::Request {
                    script: format!("rm -rf -- {}", remote::sh_path(&target.path)),
                    ..Default::default()
                },
                &|| true,
            );
        } else {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
