//! Native workspace backend and fixed no-network/no-hooks command policy.
use super::*;

pub(super) struct Native<'a> {
    pub(super) ws: &'a Workspace,
    pub(super) common: String,
    pub(super) quarantine: String,
}

impl Backend for Native<'_> {
    fn keep_message(&mut self) -> Result<String, String> {
        let mut nonce = [0; 16];
        gitcraft_os::random_bytes(&mut nonce).map_err(|e| e.to_string())?;
        Ok(format!(
            "gitcraft-bundle-keep-v1-{}",
            nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ))
    }
    fn retire_keep(&mut self, keep: &Keep) -> Result<(), String> {
        let words = if remote::is_remote(&self.ws.repo) {
            runner::sh_alias(
                "gitcraft-bundle-retire",
                "exec python3 -c \"$@\"",
                &[
                    crate::git::bundle_helper::RETIRE_KEEP_REMOTE,
                    &self.common,
                    &keep.hash,
                    &keep.message,
                ],
            )
        } else {
            let args = vec![keep.hash.clone(), keep.message.clone()];
            helper_words("--retire-owned-keep", &self.common, &args)?
        };
        self.execute(&words, vec![]).map(|_| ())
    }
    fn unchanged(&mut self) -> Result<(), String> {
        if identity(&self.ws.repo)? != self.common {
            return Err("The destination repository changed; refs were not published".into());
        }
        self.ws
            .authority(remote::workspace_authority::FenceMode::Mutation)
            .map(|_| ())
    }
    fn git(&mut self, store: Store, args: &[&str], input: Vec<u8>) -> Result<Vec<u8>, String> {
        let directory = if store == Store::Destination {
            &self.common
        } else {
            &self.quarantine
        };
        let alternate = if store == Store::Quarantine {
            let objects = format!("{}/objects", self.common);
            if objects.chars().any(char::is_control) {
                return Err("Object directory has unsupported control characters".into());
            }
            serde_json::to_string(&objects).map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        let mut command = policy(directory, &format!("{}/no-hooks", self.ws.base_there));
        command.extend(args.iter().map(|s| (*s).to_owned()));
        let words = wrapper(remote::is_remote(&self.ws.repo), &alternate, &command)?;
        self.execute(&words, input)
    }
}

impl Native<'_> {
    pub(super) fn execute(&self, words: &[String], input: Vec<u8>) -> Result<Vec<u8>, String> {
        let words: Vec<_> = words.iter().map(String::as_str).collect();
        let authority = self
            .ws
            .authority(remote::workspace_authority::FenceMode::Mutation)?;
        let record = self.ws.owner_record(&authority)?;
        let mut owned = git::WorkspaceCommand::new(&authority);
        owned.input = Some(input);
        let out = self.ws.execute(
            &self.ws.repo,
            &words,
            &record,
            Duration::from_secs(120),
            owned,
        )?;
        if out.stdout.len() > 2 << 20 || out.stderr.len() > 2 << 20 {
            return Err(
                "Contained Git output exceeded 2 MiB; no partial success is accepted".into(),
            );
        }
        if out.code != Some(0) {
            return Err(format!(
                "Contained Git failed ({}): {}",
                out.code.map_or("no exit code".into(), |n| n.to_string()),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(out.stdout)
    }
}

pub(super) fn policy(directory: &str, hooks: &str) -> Vec<String> {
    [
        "--no-lazy-fetch",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
        "-c",
        "core.commitGraph=false",
        "-c",
        "core.multiPackIndex=false",
        "-c",
        "protocol.allow=never",
        "-c",
        "credential.helper=",
        "-c",
        "pack.threads=1",
        "-c",
        "pack.windowMemory=32m",
        "-c",
        "pack.deltaCacheSize=32m",
        "-c",
        "core.packedGitWindowSize=8m",
        "-c",
        "core.packedGitLimit=64m",
        "-c",
        "core.bigFileThreshold=16m",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([
        "-c".into(),
        format!("core.hooksPath={hooks}"),
        format!("--git-dir={directory}"),
    ])
    .collect()
}

pub(super) fn wrapper(
    there: bool,
    alternate: &str,
    args: &[String],
) -> Result<Vec<String>, String> {
    let mut owned = Vec::new();
    let script = if there {
        owned.push(crate::git::bundle_helper::REMOTE.to_owned());
        owned.push(alternate.to_owned());
        "command -v python3 >/dev/null 2>&1 || { echo 'Contained bundle import requires Python 3 on this Linux host; no pack was opened.' >&2; exit 78; }; exec python3 -c \"$@\""
    } else {
        return helper_words(&crate::git::bundle_helper::program()?, alternate, args);
    };
    owned.extend_from_slice(args);
    let borrowed: Vec<_> = owned.iter().map(String::as_str).collect();
    Ok(runner::sh_alias(
        "gitcraft-bundle-contained",
        script,
        &borrowed,
    ))
}

pub(super) fn helper_words(
    program: &str,
    alternate: &str,
    args: &[String],
) -> Result<Vec<String>, String> {
    let mut owned = Vec::new();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let exe = crate::app::strip_verbatim(exe);
    let exe = exe
        .to_str()
        .ok_or("Application path is not UTF-8")?
        .to_owned();
    #[cfg(windows)]
    let exe = exe.replace('\\', "/");
    owned.push(exe);
    owned.push(program.into());
    owned.push(alternate.to_owned());
    let script = "helper=$1; git=$2; alternate=$3; shift 3; exec \"$helper\" --gitcraft-contained-git \"$git\" \"$alternate\" -- \"$@\"";
    owned.extend_from_slice(args);
    let borrowed: Vec<_> = owned.iter().map(String::as_str).collect();
    Ok(runner::sh_alias(
        "gitcraft-bundle-contained",
        script,
        &borrowed,
    ))
}
