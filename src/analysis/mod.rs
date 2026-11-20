//! Code-analysis providers: read-only viewers of results a server already computed (issue #203).
//!
//! Deliberately separate from forges (`crate::forge`) and issue trackers (`crate::trackers`):
//! a provider never receives repository contents and never writes. A provider answers one
//! question per [`Query`] with an [`gate::Observation`]; whether that observation may be shown
//! as current is decided by [`gate::evaluate`], independently of the provider.

pub(crate) mod gate;
pub(crate) mod sonar;

/// Which analysis of a project is asked about.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Target {
    /// A branch by its name on the analysis server.
    Branch(String),
    /// A pull request by the key the analysis server uses (the forge's number).
    PullRequest(String),
}

/// A project and one of its analyzed branches or pull requests.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Query {
    pub project: String,
    pub target: Target,
}

/// What a connection check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Connected {
    /// The server's reported version; SonarQube Cloud reports its own build number.
    pub version: String,
}

/// Why a provider could not answer. Values carry no server text, URL or credential, so they
/// can be shown, logged or compared without redaction; [`Failure::message_id`] localizes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// No analysis connection is configured in the profile.
    NotConfigured,
    /// The connection settings do not validate (see the account's `validate`).
    InvalidConnection,
    /// No token is saved, or the credential store cannot provide it.
    MissingToken,
    /// Offline mode or a metered connection pauses network work.
    Offline,
    Cancelled,
    /// HTTP 401: the token was refused.
    Unauthorized,
    /// HTTP 403: the token lacks 'Browse' on the project.
    Forbidden,
    /// HTTP 404 for the project, or a private project the token cannot see.
    ProjectNotFound,
    /// SonarQube Cloud: the project belongs to another organization than the configured one.
    WrongOrganization,
    /// The repository's binding was made for another analysis connection.
    BindingMismatch,
    /// HTTP 429; seconds from `Retry-After` when the server sent them.
    RateLimited(Option<u64>),
    /// The server is older than the oldest version with branch and PR quality gates (7.7).
    UnsupportedVersion(String),
    /// The server's edition has no pull request analysis.
    PullRequestsUnsupported,
    /// Any other HTTP status.
    Http(u16),
    /// A redirect: refused, so that the token is never replayed to another origin.
    Redirected,
    Timeout,
    Unreachable,
    /// The answer was not the documented shape, or exceeded the size bound.
    Malformed,
    /// The background queue refused the work.
    Busy,
}

impl Failure {
    pub(crate) fn message_id(&self) -> &'static str {
        match self {
            Self::NotConfigured => "analysis-error-not-configured",
            Self::InvalidConnection => "analysis-error-invalid-connection",
            Self::MissingToken => "analysis-error-missing-token",
            Self::Offline => "analysis-error-offline",
            Self::Cancelled => "analysis-error-cancelled",
            Self::Unauthorized => "analysis-error-unauthorized",
            Self::Forbidden => "analysis-error-forbidden",
            Self::ProjectNotFound => "analysis-error-project-not-found",
            Self::WrongOrganization => "analysis-error-wrong-organization",
            Self::BindingMismatch => "analysis-error-binding-mismatch",
            Self::RateLimited(_) => "analysis-error-rate-limited",
            Self::UnsupportedVersion(_) => "analysis-error-unsupported-version",
            Self::PullRequestsUnsupported => "analysis-error-pull-requests-unsupported",
            Self::Http(_) => "analysis-error-http",
            Self::Redirected => "analysis-error-redirected",
            Self::Timeout => "analysis-error-timeout",
            Self::Unreachable => "analysis-error-unreachable",
            Self::Malformed => "analysis-error-malformed",
            Self::Busy => "analysis-error-busy",
        }
    }

    /// The localized sentence for the UI.
    pub(crate) fn text(&self) -> String {
        let id = self.message_id();
        match self {
            Self::RateLimited(Some(seconds)) => crate::i18n::message(
                "analysis-error-rate-limited-retry",
                &[("seconds", (*seconds).into())],
            ),
            Self::UnsupportedVersion(version) => {
                crate::i18n::message(id, &[("version", version.clone().into())])
            }
            Self::Http(status) => crate::i18n::message(id, &[("status", (*status).into())]),
            _ => crate::i18n::text(id).to_owned(),
        }
    }
}

/// A read-only analysis provider. Every method may block on the network: call it from a
/// background worker, never the UI thread.
pub(crate) trait Provider {
    /// Verifies that the server answers and accepts the credential.
    fn check(&mut self) -> Result<Connected, Failure>;
    /// The quality gate of the newest analysis of `query`, with the identity of that analysis.
    fn quality_gate(&mut self, query: &Query) -> Result<gate::Observation, Failure>;
}
