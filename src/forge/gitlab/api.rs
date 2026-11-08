//! Explicit repository/host routing; selected Profile authentication is applied by cli.
use super::super::{Forge, Hosting};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[cfg(test)]
type Recording =
    std::sync::Arc<dyn Fn(&str, &str, Option<&Value>) -> Result<Value, String> + Send + Sync>;

pub(super) struct Api {
    pub hosting: Hosting,
    pub context: crate::connections::Context,
    deadline: Instant,
    bytes: AtomicUsize,
    #[cfg(test)]
    pub record: Option<Recording>,
}
impl Api {
    pub fn new(_repo: &Path, hosting: &Hosting) -> Result<Self, String> {
        if hosting.forge != Forge::GitLab {
            return Err(crate::i18n::text("backend-this-isn-t-a-gitlab-repository-f90a12").into());
        }
        if let Some(e) = crate::connections::current().error {
            return Err(e);
        }
        Ok(Self {
            hosting: hosting.clone(),
            context: crate::connections::current(),
            deadline: Instant::now() + Duration::from_secs(120),
            bytes: AtomicUsize::new(0),
            #[cfg(test)]
            record: None,
        })
    }
    pub fn project(&self) -> String {
        format!("projects/{}", segment(&self.hosting.path))
    }
    pub fn mr(&self, number: u64) -> String {
        format!("{}/merge_requests/{number}", self.project())
    }
    pub fn get(&self, endpoint: &str) -> Result<Value, String> {
        self.send("GET", endpoint, None)
    }
    pub fn send(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        self.context.run(|| {
            if Instant::now() >= self.deadline {
                return Err(crate::i18n::text("backend-gitlab-request-deadline-exceeded-refresh-to-try-again-3eebb5").into());
            }
            if crate::tasks::cancellation().is_cancelled() {
                return Err("GitLab operation cancelled".into());
            }
            #[cfg(test)]
            if let Some(record) = &self.record { return record(method, endpoint, body); }
            let mut cmd = command(Command::new(crate::connections::program("glab")), &self.hosting, method, endpoint, body.is_some())?;
            // Account resolution happens only here, on the worker, never in a recording builder.
            crate::connections::configure_cli(&mut cmd, "glab", &super::api_authority(&self.hosting))?;
            crate::shell_path::apply(&mut cmd);
            let input = body.map(serde_json::to_vec).transpose().map_err(|e| e.to_string())?;
            if input.as_ref().is_some_and(|b| b.len() > 1 << 20) { return Err(crate::i18n::text("backend-gitlab-request-exceeds-1-mib-0da4c3").into()); }
            let out = crate::processes::run(cmd, crate::processes::Options {
                input,
                background: true,
                timeout: Some(self.deadline.saturating_duration_since(Instant::now())),
                output_limit: 4 << 20,
                stderr_limit: Some(64 << 10),
                ..Default::default()
            })?.complete()?;
            self.bytes.fetch_add(out.stdout.len(), Ordering::Relaxed);
            if self.bytes.load(Ordering::Relaxed) > 32 << 20 { return Err(crate::i18n::text("backend-gitlab-request-exceeds-the-32-mib-aggregate-response-limit-10dc9e").into()); }
            if !out.status.success() {
                // No automatic retry: a write may have reached GitLab before a disconnect.
                return Err(crate::i18n::message("backend-gitlab-request-failed-a-write-may-already-have-reached-gitlab-ref-5c5ff0", &[("status", (out.status).to_string().into()), ("crate", (crate::diagnostics::crash::scrub(&String::from_utf8_lossy(&out.stderr))).to_string().into())]));
            }
            if out.stdout.iter().all(u8::is_ascii_whitespace) { Ok(Value::Null) }
            else { serde_json::from_slice(&out.stdout).map_err(|_| crate::i18n::text("backend-gitlab-returned-an-invalid-json-response-a8a37b").into()) }
        })
    }
    pub fn all(&self, endpoint: &str) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        for page in 1..=11 {
            let sep = if endpoint.contains('?') { '&' } else { '?' };
            let v = self.get(&format!("{endpoint}{sep}per_page=100&page={page}"))?;
            let items = v.as_array().ok_or(crate::i18n::text(
                "backend-gitlab-returned-an-invalid-paged-response-2dc665",
            ))?;
            if out.len() + items.len() > 1000 {
                return Err(
                    crate::i18n::text("backend-gitlab-returned-more-than-1-000-items-open-the-complete-discussio-ad9b05")
                        .into(),
                );
            }
            out.extend(items.iter().cloned());
            if items.len() < 100 {
                return Ok(out);
            }
        }
        Err(crate::i18n::text(
            "backend-gitlab-returned-more-than-1-000-items-open-the-complete-discussio-ad9b05",
        )
        .into())
    }
}
pub(super) fn segment(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Keep the web authority, including a nondefault port, bound to the repository snapshot.
/// The generic PR URL parser deliberately removes ports and cannot establish this alone.
pub(super) fn source_matches(hosting: &Hosting, url: &str) -> bool {
    fn origin(url: &str) -> Option<String> {
        let (_, rest) = url.split_once("://")?;
        if rest.split('/').next()?.contains('@') {
            return None;
        }
        let base = super::super::web_base(url)?;
        let (scheme, authority) = base.split_once("://")?;
        let authority = authority
            .strip_suffix(if scheme == "https" { ":443" } else { ":80" })
            .unwrap_or(authority);
        Some(format!("{scheme}://{authority}"))
    }
    let expected = origin(&hosting.web);
    expected.is_some()
        && expected == origin(url)
        && crate::pull_request::repo_of_url(url)
            == Some((hosting.host.clone(), hosting.path.clone()))
}

/// Pure production command transformation; tests supply a fixed inert executable.
pub(super) fn command(
    mut cmd: Command,
    hosting: &Hosting,
    method: &str,
    endpoint: &str,
    input: bool,
) -> Result<Command, String> {
    let authority = super::api_authority(hosting);
    if !hosting.web.starts_with("https://")
        || authority.is_empty()
        || authority.starts_with(['-', '.'])
        || !authority
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
    {
        return Err(crate::i18n::text(
            "backend-the-native-gitlab-page-requires-an-explicit-https-authority-with-964194",
        )
        .into());
    }
    if !["GET", "POST", "PUT"].contains(&method)
        || endpoint.starts_with('/')
        || endpoint.contains("://")
        || endpoint.chars().any(char::is_whitespace)
        || endpoint.contains('\\')
    {
        return Err(crate::i18n::text("backend-invalid-gitlab-api-operation-311ceb").into());
    }
    cmd.current_dir(std::env::temp_dir())
        .args([
            "api",
            "--hostname",
            &authority,
            "--method",
            method,
            endpoint,
        ])
        .env("NO_COLOR", "1")
        .env("NO_PROMPT", "1")
        .env("GLAB_ENABLE_CI_AUTOLOGIN", "false")
        .env("GITLAB_API_HOST", &authority)
        .env("GITLAB_HOST", &authority)
        .env("GLAB_API_PROTOCOL", "https")
        .env_remove("GITLAB_SUBFOLDER")
        .env_remove("API_PROTOCOL")
        .env_remove("GITLAB_REPO")
        .env_remove("GITLAB_URI")
        .env_remove("GL_HOST")
        .env_remove("GITLAB_HEAD_REPO")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("GLAB_DEBUG")
        .env_remove("GLAB_DEBUG_HTTP");
    for key in [
        "GITLAB_TOKEN",
        "GITLAB_ACCESS_TOKEN",
        "OAUTH_TOKEN",
        "GLAB_TOKEN",
        "CI_JOB_TOKEN",
        "JOB_TOKEN",
        "OAUTH2_REFRESH_TOKEN",
        "OAUTH2_EXPIRY_DATE",
        "GITLAB_CLIENT_ID",
        "CLIENT_ID",
    ] {
        cmd.env_remove(key);
    }
    if input {
        cmd.args(["--input", "-"]);
    }
    Ok(cmd)
}
