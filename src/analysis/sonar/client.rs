//! The read-only request sequence for a quality gate. Every call is a GET of a documented
//! Web API v1 endpoint; the transport is injected so the sequence is tested without a server.

use super::super::gate::{Analysis, Observation, ServerGate};
use super::super::{Connected, Failure, Provider, Query, Target};
use super::parse::{self, Version};
use super::{Account, Edition};

/// How the token goes along. `None` for public endpoints (`api/server/version`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum Auth {
    None,
    /// The token as the Basic login with an empty password: SonarQube Server before 10.0
    /// ("The token is sent via the login field of HTTP basic authentication").
    Basic,
    /// `Authorization: Bearer`: SonarQube Server 10.0+ and SonarQube Cloud.
    Bearer,
}

/// Oldest Server with `branch`/`pullRequest` on `api/qualitygates/project_status` (7.7).
const OLDEST: Version = Version { major: 7, minor: 7 };
/// First Server documenting Bearer tokens.
const BEARER_SINCE: u32 = 10;

pub(in crate::analysis) struct Client<F> {
    account: Account,
    send: F,
    auth: Auth,
}

fn encoded(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// A 404 of an endpoint that addresses the project means it is absent or invisible to the token.
fn project(result: Result<String, Failure>) -> Result<String, Failure> {
    result.map_err(|e| match e {
        Failure::Http(404) => Failure::ProjectNotFound,
        other => other,
    })
}

impl<F: FnMut(&str, Auth) -> Result<String, Failure>> Client<F> {
    /// `send(path, auth)` performs `GET {base}/{path}` and returns the body of a 2xx answer.
    pub(in crate::analysis) fn new(account: Account, send: F) -> Self {
        Self {
            account,
            send,
            auth: Auth::Bearer,
        }
    }

    fn get(&mut self, path: &str) -> Result<String, Failure> {
        (self.send)(path, self.auth)
    }

    /// Reads the version and picks the authentication scheme it supports.
    fn prepare(&mut self) -> Result<String, Failure> {
        let raw = match (self.send)("api/server/version", Auth::None) {
            // Digits and dots only: no other server text reaches the UI.
            Ok(text) => text
                .trim()
                .chars()
                .take(32)
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>(),
            // Some locked-down servers refuse even this; assume a current one.
            Err(Failure::Unauthorized | Failure::Forbidden) => String::new(),
            Err(e) => return Err(e),
        };
        if self.account.edition == Edition::Cloud {
            self.auth = Auth::Bearer;
            return Ok(raw);
        }
        match parse::version(&raw) {
            Some(v) if v < OLDEST => Err(Failure::UnsupportedVersion(format!(
                "{}.{}",
                v.major, v.minor
            ))),
            Some(v) if v.major < BEARER_SINCE => {
                self.auth = Auth::Basic;
                Ok(raw)
            }
            _ => {
                self.auth = Auth::Bearer;
                Ok(raw)
            }
        }
    }

    /// SonarQube Cloud keys are global: confirm the project is in the configured organization.
    fn same_organization(&mut self, key: &str) -> Result<(), Failure> {
        if self.account.edition != Edition::Cloud {
            return Ok(());
        }
        let body = project(self.get(&format!("api/components/show?component={key}")))?;
        match parse::organization(&body)? {
            Some(org) if org.eq_ignore_ascii_case(self.account.organization.trim()) => Ok(()),
            _ => Err(Failure::WrongOrganization),
        }
    }

    fn branch_analysis(
        &mut self,
        key: &str,
        name: &str,
        main: bool,
    ) -> Result<(Option<Analysis>, ServerGate, Vec<String>), Failure> {
        let branch = if main {
            String::new()
        } else {
            format!("&branch={}", encoded(name))
        };
        let body = project(self.get(&format!(
            "api/project_analyses/search?project={key}{branch}&ps=1"
        )))?;
        let Some(analysis) = parse::latest_analysis(&body)? else {
            return Ok((None, ServerGate::Unknown, Vec::new()));
        };
        let id = analysis.key.as_deref().map(encoded).unwrap_or_default();
        let (gate, failing) = self.status(&format!("analysisId={id}"))?;
        Ok((Some(analysis), gate, failing))
    }

    fn pull_request_analysis(
        &mut self,
        key: &str,
        pr: &str,
    ) -> Result<(Option<Analysis>, ServerGate, Vec<String>), Failure> {
        let Some(row) = self.pull_request_row(key, pr)? else {
            return Ok((None, ServerGate::Unknown, Vec::new()));
        };
        let (gate, failing) =
            self.status(&format!("projectKey={key}&pullRequest={}", encoded(pr)))?;
        // The PR gate cannot be asked by analysis id: read the row again, and if another
        // analysis landed in between, the commit is not claimed for this gate.
        let unchanged = self.pull_request_row(key, pr)?.as_ref() == Some(&row);
        let analysis = Analysis {
            key: None,
            date: row.date,
            revision: row.sha.filter(|_| unchanged),
        };
        Ok((Some(analysis), gate, failing))
    }

    fn pull_request_row(
        &mut self,
        key: &str,
        pr: &str,
    ) -> Result<Option<parse::PullRequest>, Failure> {
        let body = self
            .get(&format!("api/project_pull_requests/list?project={key}"))
            .map_err(|e| match e {
                // Editions without PR analysis answer "Unknown url" or reject the call.
                Failure::Http(400 | 404) => Failure::PullRequestsUnsupported,
                other => other,
            })?;
        Ok(parse::pull_requests(&body)?
            .into_iter()
            .find(|row| row.key == pr))
    }

    fn status(&mut self, selector: &str) -> Result<(ServerGate, Vec<String>), Failure> {
        match self.get(&format!("api/qualitygates/project_status?{selector}")) {
            Ok(body) => parse::project_status(&body),
            // "Returns an HTTP code 404 if the analysis ... is not found or does not exist."
            Err(Failure::Http(404)) => Ok((ServerGate::Unknown, Vec::new())),
            Err(e) => Err(e),
        }
    }

    /// Pending and failed background tasks of the target. Reading tasks may need more than
    /// 'Browse'; when it is refused the observation simply does not claim either.
    fn tasks(
        &mut self,
        key: &str,
        target: &Target,
        main: Option<&str>,
    ) -> Result<(bool, bool), Failure> {
        let tasks = match self.get(&format!("api/ce/component?component={key}")) {
            Ok(body) => parse::tasks(&body)?,
            // Only a refusal of this one endpoint is tolerated; a timeout or rate limit must
            // not silently drop the pending and failed-task checks.
            Err(Failure::Forbidden | Failure::Http(_)) => return Ok((false, false)),
            Err(e) => return Err(e),
        };
        let ours = |t: &&parse::Task| match target {
            Target::PullRequest(pr) => t.pull_request.as_deref() == Some(pr),
            Target::Branch(name) => {
                t.pull_request.is_none()
                    && match t.branch.as_deref() {
                        Some(branch) => branch == name,
                        None => main == Some(name.as_str()),
                    }
            }
        };
        let pending = tasks
            .iter()
            .filter(ours)
            .any(|t| !t.current && matches!(t.status.as_str(), "PENDING" | "IN_PROGRESS"));
        let failed = tasks
            .iter()
            .filter(ours)
            .any(|t| t.current && t.status == "FAILED");
        Ok((pending, failed))
    }

    fn dashboard(&self, key: &str, target: &Target, main: Option<&str>) -> String {
        let scope = match target {
            Target::Branch(name) if main == Some(name.as_str()) => String::new(),
            Target::Branch(name) => format!("&branch={}", encoded(name)),
            Target::PullRequest(pr) => format!("&pullRequest={}", encoded(pr)),
        };
        format!("{}/dashboard?id={key}{scope}", self.account.base())
    }

    /// Verifies that the server answers and accepts the token (Profile → Connections).
    pub(in crate::analysis) fn check(&mut self) -> Result<Connected, Failure> {
        let version = self.prepare()?;
        if !parse::valid(&self.get("api/authentication/validate")?)? {
            return Err(Failure::Unauthorized);
        }
        Ok(Connected { version })
    }
}

impl<F: FnMut(&str, Auth) -> Result<String, Failure>> Provider for Client<F> {
    fn quality_gate(&mut self, query: &Query) -> Result<Observation, Failure> {
        if !super::project_key(&query.project) {
            return Err(Failure::ProjectNotFound);
        }
        self.prepare()?;
        let key = encoded(&query.project);
        self.same_organization(&key)?;
        let branches = parse::branches(&project(
            self.get(&format!("api/project_branches/list?project={key}")),
        )?)?;
        let main = branches.iter().find(|b| b.is_main).map(|b| b.name.clone());
        let (analysis, gate, failing) = match &query.target {
            Target::Branch(name) => match branches.iter().find(|b| &b.name == name) {
                Some(branch) => self.branch_analysis(&key, name, branch.is_main)?,
                None => (None, ServerGate::Unknown, Vec::new()),
            },
            Target::PullRequest(pr) => self.pull_request_analysis(&key, pr)?,
        };
        let (pending, last_task_failed) = self.tasks(&key, &query.target, main.as_deref())?;
        Ok(Observation {
            dashboard: self.dashboard(&key, &query.target, main.as_deref()),
            target: query.target.clone(),
            gate,
            analysis,
            pending,
            last_task_failed,
            failing,
        })
    }
}
