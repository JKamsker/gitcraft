//! Remote request construction and transport execution for selected repositories.

use super::*;

pub(in crate::git) const CONNECTION_REDIRECT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_CONFIG",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_NOSYSTEM",
];

pub(super) fn normalize_connection_command(cmd: &mut Command, args: &[&str]) {
    for name in CONNECTION_REDIRECT_ENV {
        cmd.env_remove(name);
    }
    for (name, _) in std::env::vars_os() {
        let text = name.to_string_lossy();
        if text.starts_with("GIT_CONFIG_KEY_") || text.starts_with("GIT_CONFIG_VALUE_") {
            cmd.env_remove(name);
        }
    }
    // Intentional immutable profile config is reinstalled after inherited config is removed.
    with_profile(cmd, args);
}

/// The request that runs git with `args` in `target`'s folder, on its machine.
pub(in crate::git) fn remote_request(
    target: &Target,
    extra: &Extra,
    args: &[&str],
) -> remote::Request {
    let guard = super::super::clone_options::passive_object_read(args).then_some("--no-lazy-fetch");
    let words: Vec<String> = guard
        .into_iter()
        .chain(
            extra
                .pre
                .iter()
                .copied()
                .chain(extra.owned_pre.iter().map(String::as_str))
                .chain(args.iter().copied()),
        )
        .map(remote::sh_quote)
        .collect();
    let credential_transport = extra
        .owned_pre
        .iter()
        .any(|arg| arg == "gitcraft-credential-transport");
    let pin = if credential_transport {
        CREDENTIAL_REMOTE_PIN
    } else {
        ""
    };
    let script = if extra.trace_hooks {
        let program = if credential_transport {
            "\"$GITCRAFT_GIT_ORIGINAL\""
        } else {
            "git"
        };
        format!(
            "{pin}{}",
            super::super::hooks::traced_script_with_program(&words.join(" "), program)
        )
    } else if credential_transport {
        format!("{pin}exec \"$GITCRAFT_GIT_ORIGINAL\" {}", words.join(" "))
    } else {
        format!("exec git {}", words.join(" "))
    };
    remote::Request {
        cwd: Some(target.path.clone()),
        script,
        env: GIT_ENV
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .chain(std::iter::once((
                "GIT_NO_LAZY_FETCH".to_owned(),
                super::super::clone_options::download_environment(args).to_owned(),
            )))
            .chain(extra.env.iter().map(|(n, v)| ((*n).to_owned(), v.clone())))
            .chain(offline_env().map(|(n, v)| (n.to_owned(), v.to_owned())))
            .collect(),
        unset: GIT_ENV_REMOVED
            .iter()
            .chain(REPOSITORY_ENV_REMOVED.iter())
            .chain(
                extra
                    .normalize_repository
                    .then_some(CONNECTION_REDIRECT_ENV)
                    .into_iter()
                    .flatten(),
            )
            .chain(extra.unset.iter())
            .map(|n| (*n).to_owned())
            .collect(),
        stdin: extra.stdin.clone(),
        // A tool stays open as long as the user works in it, apart from ordinary git reads.
        long: extra.tool || extra.long || goes_over_network(args),
        timeout: extra
            .timeout
            .or(extra.test)
            .or_else(|| extra.tool.then_some(TOOL_FOR)),
        test: extra.test.is_some(),
        apart: false,
    }
}

// Kept raw for recent-status/across classification. Translate only at display_error.
pub(in crate::git) fn missing_folder(target: &Target) -> String {
    format!("{} isn't a folder on {}", target.path, target.host.label())
}

pub(super) fn execute_there(target: &Target, extra: &Extra, args: &[&str]) -> Result<Ran, String> {
    let out = remote::exec(&target.host, &remote_request(target, extra, args))?;
    // The folder is gone (or never was) on that machine.
    if out.code == 1 && remote::is_missing_folder(&out.stderr) {
        return Err(missing_folder(target));
    }
    let (stderr, events) = if extra.trace_hooks {
        super::super::hooks::split_trace(&out.stderr)
    } else {
        (out.stderr, String::new())
    };
    Ok(Ran {
        code: Some(out.code),
        status: crate::i18n::message(
            "backend-exit-status-c5b63a",
            &[("code", (out.code).to_string().into())],
        ),
        stdout: out.stdout,
        stderr,
        failed_hook: (out.code != 0)
            .then(|| super::super::hooks::failed_hook(&events, args))
            .flatten(),
    })
}
