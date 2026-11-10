//! Immutable push authorization, route binding, and preflight dispatch.

use super::*;

pub(crate) struct PushTarget<'a> {
    pub address: &'a str,
    pub skip_hooks: bool,
    pub alias: &'a str,
    pub original_remote: &'a str,
    pub lfs_config: &'a [(String, String)],
    pub lfs_objects: &'a [String],
    pub receive_pack: &'a str,
}

pub(crate) fn push_preflight_with(
    dir: &Path,
    args: &[String],
    target: PushTarget<'_>,
    creds: Option<&Credentials>,
    control: Option<&std::sync::Arc<OpControl>>,
) -> Result<String, String> {
    let PushTarget {
        address,
        skip_hooks,
        alias: opaque,
        original_remote,
        lfs_config,
        lfs_objects,
        receive_pack,
    } = target;
    let (source, destination) =
        args.last()
            .and_then(|s| s.split_once(':'))
            .ok_or(crate::i18n::text(
                "backend-missing-inspected-refspec-fa73fb",
            ))?;
    let remote_tip = args.iter().find_map(|a| {
        a.strip_prefix("--force-with-lease=")
            .and_then(|v| v.rsplit_once(':'))
            .map(|v| v.1)
    });
    let hooks = super::super::push_hooks::Hooks::new(
        dir,
        original_remote,
        super::super::push_hooks::Upload {
            alias: opaque,
            skip_original: skip_hooks,
            objects: lfs_objects,
            source,
            destination,
            remote_tip,
        },
    )?;
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut extra = reviewed_push_extra(
        dir,
        &words,
        PushTarget {
            address,
            skip_hooks,
            alias: opaque,
            original_remote,
            lfs_config,
            lfs_objects,
            receive_pack,
        },
        hooks.path(),
        creds,
    )?;
    let mapping: Vec<_> = extra
        .env
        .iter()
        .filter(|(name, _)| name.starts_with("GIT_CONFIG_"))
        .cloned()
        .collect();
    if forecast_run(
        dir,
        // get-url requires a locally configured remote. The original and private
        // alias now have the same single explicit URL/pushurl (empty entries
        // clear inherited multi-address values). Prove push expansion using the
        // configured original, while transport/hooks use the private alias.
        &["remote", "get-url", "--push", "--all", original_remote],
        &mapping,
        &[0],
        &|| true,
    )
    .map_err(|_| {
        crate::i18n::text("backend-the-reviewed-address-could-not-be-pinned-no-push-was-run-8a62d8")
    })?
    .trim_end()
        != address
    {
        return Err(crate::i18n::text(
            "backend-the-reviewed-address-could-not-be-pinned-no-push-was-run-8a62d8",
        )
        .into());
    }
    // Preserve only Git's structured local hook identity/status. Arbitrary hook and
    // server output remains private, as it can contain credentials or secrets.
    let trace = (!remote::is_remote(dir))
        .then(super::super::hooks::TraceFile::new)
        .flatten();
    if let Some(trace) = &trace {
        extra.env.push((
            super::super::hooks::TRACE_VAR,
            trace.path().to_string_lossy().into_owned(),
        ));
    }
    let out = execute_preflight(dir, &extra, &words, control)?;
    if out.code == Some(0) {
        Ok(crate::i18n::text("backend-push-completed-5fccd0").into())
    } else if let Some(hook) = trace.and_then(|trace| trace.failed_hook(&words)) {
        Err(hook.explain(
            "Hook output withheld because it can contain credentials or secrets.",
            &words,
        ))
    } else if is_auth_error(&String::from_utf8_lossy(&out.stderr)) {
        Err(
            crate::i18n::text("backend-authentication-failed-for-the-reviewed-push-target-75e906")
                .into(),
        )
    } else {
        Err(reviewed_refusal(&out.stdout,source,destination).unwrap_or_else(||crate::i18n::text("backend-the-reviewed-push-was-refused-or-failed-fetch-and-reopen-prefligh-59a6f0").into()))
    }
}

/// Pure construction shared by watched and ordinary reviewed-push runners.
pub(super) fn reviewed_push_extra(
    dir: &Path,
    words: &[&str],
    target: PushTarget<'_>,
    hooks_path: String,
    creds: Option<&Credentials>,
) -> Result<Extra, String> {
    let PushTarget {
        address,
        alias: opaque,
        original_remote,
        lfs_config,
        receive_pack,
        ..
    } = target;
    let placeholder = format!("gitcraft-preflight-address://{opaque}");
    let mut values = vec![
        (format!("url.{address}.insteadOf"), placeholder.clone()),
        (format!("remote.{opaque}.url"), String::new()),
        (format!("remote.{opaque}.url"), placeholder.clone()),
        // An explicit pushurl disables pushInsteadOf for this private remote.
        // Otherwise a push-only rule for the placeholder prefix can bypass the
        // ordinary URL expansion proof while leaving its config stamp unchanged.
        (format!("remote.{opaque}.pushurl"), String::new()),
        (format!("remote.{opaque}.pushurl"), placeholder.clone()),
        (format!("remote.{original_remote}.url"), String::new()),
        (format!("remote.{original_remote}.url"), placeholder.clone()),
        (format!("remote.{original_remote}.pushurl"), String::new()),
        (format!("remote.{original_remote}.pushurl"), placeholder),
        ("core.hooksPath".into(), hooks_path),
    ];
    for (source, value) in lfs_config {
        if value.is_empty() {
            continue;
        }
        let target = source
            .strip_prefix(&format!("remote.{original_remote}."))
            .map(|key| format!("remote.{opaque}.{key}"))
            .unwrap_or_else(|| source.clone());
        values.push((target.clone(), value.clone()));
        if target != *source {
            values.push((source.clone(), value.clone()));
        }
    }
    if !receive_pack.is_empty() {
        values.push((format!("remote.{opaque}.receivepack"), receive_pack.into()));
    }
    const VARS: [(&str, &str); 16] = [
        ("GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_0"),
        ("GIT_CONFIG_KEY_1", "GIT_CONFIG_VALUE_1"),
        ("GIT_CONFIG_KEY_2", "GIT_CONFIG_VALUE_2"),
        ("GIT_CONFIG_KEY_3", "GIT_CONFIG_VALUE_3"),
        ("GIT_CONFIG_KEY_4", "GIT_CONFIG_VALUE_4"),
        ("GIT_CONFIG_KEY_5", "GIT_CONFIG_VALUE_5"),
        ("GIT_CONFIG_KEY_6", "GIT_CONFIG_VALUE_6"),
        ("GIT_CONFIG_KEY_7", "GIT_CONFIG_VALUE_7"),
        ("GIT_CONFIG_KEY_8", "GIT_CONFIG_VALUE_8"),
        ("GIT_CONFIG_KEY_9", "GIT_CONFIG_VALUE_9"),
        ("GIT_CONFIG_KEY_10", "GIT_CONFIG_VALUE_10"),
        ("GIT_CONFIG_KEY_11", "GIT_CONFIG_VALUE_11"),
        ("GIT_CONFIG_KEY_12", "GIT_CONFIG_VALUE_12"),
        ("GIT_CONFIG_KEY_13", "GIT_CONFIG_VALUE_13"),
        ("GIT_CONFIG_KEY_14", "GIT_CONFIG_VALUE_14"),
        ("GIT_CONFIG_KEY_15", "GIT_CONFIG_VALUE_15"),
    ];
    if values.len() > VARS.len() {
        return Err(crate::i18n::text(
            "backend-too-many-pinned-transport-settings-no-push-was-run-f9ebc3",
        )
        .into());
    }
    let mut extra = Extra {
        pre: Vec::new(),
        unset: PREFLIGHT_REMOVE.to_vec(),
        env: vec![
            ("GIT_CONFIG_COUNT", values.len().to_string()),
            ("GIT_NO_REPLACE_OBJECTS", "1".into()),
        ],
        ..Default::default()
    };
    for ((key, value), (key_var, value_var)) in values.into_iter().zip(VARS) {
        extra.env.extend([(key_var, key), (value_var, value)]);
    }
    preflight_credentials(dir, &mut extra, words, address, creds)?;
    Ok(extra)
}

pub(super) fn preflight_credentials(
    dir: &Path,
    extra: &mut Extra,
    words: &[&str],
    address: &str,
    creds: Option<&Credentials>,
) -> Result<(), String> {
    let Some(creds) = creds else {
        return Ok(());
    };
    // Validate the captured account even when this is another reviewed endpoint.
    // An unbound or edited cached secret must not acquire a new endpoint scope.
    creds.helper_environment()?;
    if creds
        .scope
        .as_ref()
        .is_some_and(|scope| scope.matches_url(address))
    {
        let policy = credential_command_policy(Some(dir), creds, words)?;
        extra.normalize_repository = true;
        extra.owned_pre.extend(policy.arguments);
        extra.env.extend(policy.environment);
    }
    Ok(())
}

pub(super) const PREFLIGHT_REMOVE: &[&str] = &[
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_SHALLOW_FILE",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_ATTR_SOURCE",
];

pub(super) fn execute_preflight(
    dir: &Path,
    extra: &Extra,
    words: &[&str],
    control: Option<&std::sync::Arc<OpControl>>,
) -> Result<Ran, String> {
    if let Some(control) = control {
        return watched::reviewed(dir, extra, words, control);
    }
    if let Some(target) = remote::parse(dir) {
        let mut request = remote_request(&target, extra, words);
        request
            .unset
            .extend(PREFLIGHT_REMOVE.iter().map(|s| (*s).to_owned()));
        let out = remote::exec(&target.host, &request)?;
        Ok(Ran {
            code: Some(out.code),
            status: out.code.to_string(),
            stdout: out.stdout,
            stderr: out.stderr,
            failed_hook: None,
        })
    } else {
        let cmd = local_command(Some(dir), extra, words);
        let out = crate::processes::run(
            cmd,
            crate::processes::Options {
                background: true,
                ..Default::default()
            },
        )?
        .complete()?;
        Ok(Ran {
            code: out.status.code(),
            status: out.status.to_string(),
            stdout: out.stdout,
            stderr: out.stderr,
            failed_hook: None,
        })
    }
}

pub(super) fn reviewed_refusal(bytes: &[u8], source: &str, destination: &str) -> Option<String> {
    let out = std::str::from_utf8(bytes).ok()?;
    for line in out.lines() {
        let mut fields = line.splitn(3, '\t');
        if fields.next() != Some("!") {
            continue;
        }
        let refs = fields.next()?;
        let status = fields.next()?;
        if refs != format!("{source}:{destination}") {
            continue;
        }
        let safe = match status {
            "[rejected] (non-fast-forward)"
            | "[rejected] (fetch first)"
            | "[rejected] (stale info)"
            | "[rejected] (remote ref updated since checkout)" => status,
            s if s.starts_with("[remote rejected]") => {
                "[remote rejected] (remote policy refused the update; details withheld)"
            }
            _ => "[rejected] (update refused; details withheld)",
        };
        return Some(format!("!\t{source}:{destination}\t{safe}"));
    }
    None
}
