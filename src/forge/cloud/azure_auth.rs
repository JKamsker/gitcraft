//! Azure CLI token acquisition and expiry validation.
use super::*;

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |t| t.as_secs())
}
pub(super) fn azure_token(a: &Account) -> Result<(String, Option<u64>), String> {
    let mut command = std::process::Command::new(connections::program("az"));
    azure_command(&mut command, a);
    crate::shell_path::apply(&mut command);
    let out = crate::processes::run(
        command,
        crate::processes::Options {
            background: true,
            timeout: Some(Duration::from_secs(30)),
            output_limit: 65536,
            stderr_limit: Some(65536),
            ..Default::default()
        },
    )?;
    if !out.status.success() {
        return Err(crate::i18n::text(
            "backend-selected-azure-cli-login-unavailable-sign-in-with-az-in-the-selec-0daa14",
        )
        .into());
    }
    let value: Value = serde_json::from_slice(&out.stdout).map_err(|_| {
        crate::i18n::text("backend-azure-cli-returned-invalid-token-metadata-d65a19")
    })?;
    parse_azure_token(&value, now())
}
pub(in crate::forge) fn azure_command(c: &mut std::process::Command, a: &Account) {
    c.current_dir(std::env::temp_dir()).args([
        "account",
        "get-access-token",
        "--resource",
        "499b84ac-1321-427f-aa17-267ca6975798",
        "--output",
        "json",
    ]);
    for key in [
        "AZURE_DEVOPS_EXT_PAT",
        "AZURE_ACCESS_TOKEN",
        "AZURE_CLIENT_SECRET",
        "AZURE_FEDERATED_TOKEN_FILE",
        "AZURE_CONFIG_DIR",
    ] {
        c.env_remove(key);
    }
    c.env("AZURE_CORE_COLLECT_TELEMETRY", "false")
        .env("AZURE_CORE_ONLY_SHOW_ERRORS", "true");
    if !a.config_dir.is_empty() {
        c.env("AZURE_CONFIG_DIR", &a.config_dir);
    }
}
pub(super) fn parse_azure_token(v: &Value, now: u64) -> Result<(String, Option<u64>), String> {
    let token = required(v, "/accessToken")?;
    let exp = v
        .get("expires_on")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .ok_or(crate::i18n::text(
            "backend-azure-cli-must-provide-expires-on-update-az-or-use-an-explicitly-26eb57",
        ))?;
    if exp <= now + 15 || token.len() > 65536 || token.chars().any(char::is_whitespace) {
        return Err(crate::i18n::text(
            "backend-azure-cli-returned-an-expired-or-unusable-token-bdfc27",
        )
        .into());
    }
    Ok((format!("Bearer {token}"), Some(exp)))
}
