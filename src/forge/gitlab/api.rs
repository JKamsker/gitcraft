//! Explicit repository/host routing; selected Profile authentication is applied by cli.
use super::super::{Forge, Hosting, cli};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(super) struct Api {
    pub repo: PathBuf,
    pub hosting: Hosting,
    pub context: crate::connections::Context,
    deadline: Instant,
}
impl Api {
    pub fn new(repo: &Path, hosting: &Hosting) -> Result<Self, String> {
        if hosting.forge != Forge::GitLab {
            return Err("This isn't a GitLab repository".into());
        }
        if let Some(e) = crate::connections::current().error {
            return Err(e);
        }
        Ok(Self {
            repo: repo.to_owned(),
            hosting: hosting.clone(),
            context: crate::connections::current(),
            deadline: Instant::now() + Duration::from_secs(120),
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
                return Err("GitLab request deadline exceeded; refresh to try again".into());
            }
            let out = cli::gitlab_api(
                &self.repo,
                &self.hosting,
                method,
                endpoint,
                body,
                self.deadline,
            )?;
            if out.trim().is_empty() {
                Ok(Value::Null)
            } else {
                serde_json::from_str(&out)
                    .map_err(|_| "GitLab returned an invalid JSON response".into())
            }
        })
    }
    pub fn all(&self, endpoint: &str) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        for page in 1..=10 {
            let sep = if endpoint.contains('?') { '&' } else { '?' };
            let v = self.get(&format!("{endpoint}{sep}per_page=100&page={page}"))?;
            let items = v
                .as_array()
                .ok_or("GitLab returned an invalid paged response")?;
            out.extend(items.iter().cloned());
            if items.len() < 100 {
                return Ok(out);
            }
        }
        Err("GitLab returned more than 1,000 items; open the complete discussion on GitLab".into())
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
