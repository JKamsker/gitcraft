//! CLI follow-up target validation, reviewer requests, and commit avatar extraction.
use super::*;

/// Run the captured forge's command-line tool for repository operations.
pub(crate) fn tool(hosting: &Hosting, repo: &Path, args: &[&str]) -> Result<String, String> {
    let tool = tool_name(hosting.forge)?;
    cli::run(tool, hosting.forge.noun(), repo, hosting, args)
}

impl Hosting {
    /// [`tool`], with the tool's answer captured up to `limit` bytes instead of the usual
    /// limit: a longer answer comes back cut there (`true`), not refused.
    pub(crate) fn tool_capped(
        &self,
        repo: &Path,
        args: &[&str],
        limit: usize,
    ) -> Result<(String, bool), String> {
        let tool = tool_name(self.forge)?;
        cli::run_capped(tool, self.forge.noun(), repo, self, args, limit)
    }
}

fn tool_name(forge: Forge) -> Result<&'static str, String> {
    match forge {
        Forge::GitHub => Ok("gh"),
        Forge::GitLab => Ok("glab"),
        Forge::Forgejo | Forge::Gitea | Forge::Azure | Forge::Bitbucket => {
            Err(crate::i18n::message(
                "backend-gitcraft-has-no-tool-for-c05ef1",
                &[("name", (forge.name()).to_string().into())],
            ))
        }
    }
}

/// Never let a CLI answer's URL override the repository captured for follow-up mutations.
pub(super) fn created_pr_number(hosting: &Hosting, url: &str) -> Result<String, String> {
    let expected = hosting.matches_pull_request(url);
    let number = url
        .rsplit('/')
        .next()
        .filter(|n| n.parse::<u64>().is_ok_and(|n| n > 0));
    if expected && let Some(number) = number {
        return Ok(number.to_owned());
    }
    Err(format!(
        "The forge returned a pull request outside the selected repository; reviewers were not requested. Check the created pull request: {url}"
    ))
}

/// Asks each of `reviewers` for a review of the pull request just opened at `url`, one at a
/// time with `ask`: one the forge refuses (the author, an account or team that is gone) doesn't
/// keep the others from being asked, and doesn't undo the opening. The URL, or what went wrong.
pub(crate) fn ask_reviewers(
    url: String,
    reviewers: &[String],
    ask: impl Fn(&str) -> Result<(), String>,
) -> Result<String, String> {
    let refused: Vec<(String, String)> = reviewers
        .iter()
        .filter_map(|r| ask(r).err().map(|e| (r.clone(), e)))
        .collect();
    match refused.as_slice() {
        [] => Ok(url),
        [(who, why)] => Err(crate::i18n::message(
            "backend-opened-url-but-couldn-t-ask-who-for-a-review-eded91",
            &[
                ("url", (url).to_string().into()),
                ("who", (who).to_string().into()),
                (
                    "why",
                    (why.lines().next().unwrap_or_default()).to_string().into(),
                ),
            ],
        )),
        [(_, why), ..] => Err(crate::i18n::message(
            "backend-opened-url-but-couldn-t-ask-for-a-review-3afd5b",
            &[
                ("url", (url).to_string().into()),
                (
                    "refused",
                    (refused
                        .iter()
                        .map(|(w, _)| w.as_str())
                        .collect::<Vec<_>>()
                        .join(", "))
                    .to_string()
                    .into(),
                ),
                (
                    "why",
                    (why.lines().next().unwrap_or_default()).to_string().into(),
                ),
            ],
        )),
    }
}

/// Account pictures by commit email, from a list of commits as GitHub's, Forgejo's and Gitea's
/// APIs give it: each commit is tied to the accounts its emails belong to.
pub(super) fn parse_commit_avatars(json: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(json) else {
        return out;
    };
    for item in &items {
        for (who, account) in [("author", "author"), ("committer", "committer")] {
            let email = item
                .pointer(&format!("/commit/{who}/email"))
                .and_then(|e| e.as_str());
            let avatar = item
                .pointer(&format!("/{account}/avatar_url"))
                .and_then(|a| a.as_str())
                .filter(|a| !a.is_empty());
            if let (Some(email), Some(avatar)) = (email, avatar) {
                out.entry(email.trim().to_lowercase())
                    .or_insert_with(|| avatar.to_owned());
            }
        }
    }
    out
}
