//! Commits: the graph's log, one commit's details, searches, the commits a rebase rewrites, and
//! how revisions relate.

use std::collections::HashSet;
use std::path::Path;

use super::ChangedFile;
use super::LogOrder;
use super::diff::commit_files;
use super::{END_OPTS, NO_SIGNATURE, UTF8, not_an_option, run};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
    pub date: String,
    pub summary: String,
    /// The rest of the message on one line (paragraphs joined by spaces), possibly empty.
    pub body: String,
    /// Decorations (`HEAD -> main`, `origin/main`, `tag: v1`), as git prints them.
    pub refs: Vec<String>,
    /// Its tree (what test results are kept by); empty for a row that isn't a commit.
    pub tree: String,
    /// A validated Gerrit trailer; kept before the body is flattened for the graph.
    pub change_id: Option<String>,
}

/// The commits of every branch (stashes aside), or of the revisions `from` when given, newest
/// first, in an order fit for drawing the graph (`order`): `limit` of them after the first
/// `skip`.
pub fn log(
    repo: &Path,
    from: Option<&[String]>,
    order: LogOrder,
    skip: usize,
    limit: usize,
) -> Result<Vec<Commit>, String> {
    log_scoped(
        repo,
        from,
        order,
        skip,
        limit,
        &super::history_scope::default_hidden_namespaces(),
    )
}

pub fn log_scoped(
    repo: &Path,
    from: Option<&[String]>,
    order: LogOrder,
    skip: usize,
    limit: usize,
    excluded: &[String],
) -> Result<Vec<Commit>, String> {
    // No revisions would make git log HEAD.
    if from.is_some_and(|revs| revs.is_empty()) {
        return Ok(Vec::new());
    }
    let limit = format!("--max-count={limit}");
    let args = log_args(from, order, skip, Some(&limit), excluded).as_args();
    let out = match super::backend::history_log(repo, &args, from.is_none(), || {
        run(
            Some(repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    }) {
        Ok(out) => out,
        // A freshly initialised repository has no commits yet.
        Err(e) if e.contains("does not have any commits") => String::new(),
        Err(e) => return Err(e),
    };
    let fields: Vec<&str> = out.split('\0').collect();
    Ok(fields
        .as_chunks::<LOG_FIELDS>()
        .0
        .iter()
        .filter_map(|record| parse_record(record))
        .collect())
}

/// The commits `hashes` themselves (none of their ancestors), in the order given, as [`log`]
/// reads them. Asked a hundred at a time, so the command line stays short (Windows allows
/// 32k characters).
pub fn commits_of(repo: &Path, hashes: &[String]) -> Result<Vec<Commit>, String> {
    let format = log_args(None, LogOrder::default(), 0, None, &[]).format;
    let mut commits = Vec::with_capacity(hashes.len());
    for chunk in hashes.chunks(100) {
        let mut args = vec![
            "log",
            NO_SIGNATURE,
            UTF8,
            "--no-walk=unsorted",
            "--date=format-local:%Y-%m-%d %H:%M",
            &format,
            END_OPTS,
        ];
        args.extend(chunk.iter().map(String::as_str));
        args.push("--");
        let out = run(Some(repo), &args)?;
        let fields: Vec<&str> = out.split('\0').collect();
        commits.extend(
            fields
                .as_chunks::<LOG_FIELDS>()
                .0
                .iter()
                .filter_map(|record| parse_record(record)),
        );
    }
    Ok(commits)
}

/// How many fields [`log_args`]' format has, each ended by a NUL. No commit can hold one (git
/// refuses it in a message), where the unit and record separators may well be in a message
/// someone pasted, and would cut the commit in two or leave it out of the graph.
const LOG_FIELDS: usize = 10;

/// The arguments of `log` / [`LogStream`]: the revisions `from` (else every branch) in `order`,
/// after the first `skip`, at most `limit` (`--max-count=n`) when given.
fn log_args<'a>(
    from: Option<&'a [String]>,
    order: LogOrder,
    skip: usize,
    limit: Option<&'a str>,
    excluded: &'a [String],
) -> LogArgs<'a> {
    LogArgs {
        order,
        format: "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%ad%x00%s%x00%D%x00%T%x00%b%x00"
            .to_owned(),
        skip: format!("--skip={skip}"),
        limit,
        from,
        excluded,
    }
}

struct LogArgs<'a> {
    order: LogOrder,
    format: String,
    skip: String,
    limit: Option<&'a str>,
    from: Option<&'a [String]>,
    excluded: &'a [String],
}

impl LogArgs<'_> {
    fn as_args(&self) -> Vec<String> {
        let mut args = vec!["log", NO_SIGNATURE, UTF8];
        args.extend(self.order.args());
        args.extend(["--date=format-local:%Y-%m-%d %H:%M", &self.skip]);
        args.extend(self.limit);
        args.push(&self.format);
        let mut args: Vec<String> = args.into_iter().map(str::to_owned).collect();
        args.extend(super::history_scope::revision_args(
            self.from,
            self.excluded,
        ));
        args
    }
}

/// One commit of `log`'s output: its [`LOG_FIELDS`] fields.
fn parse_record<S: AsRef<str>>(fields: &[S]) -> Option<Commit> {
    let [
        hash,
        parents,
        author,
        email,
        time,
        date,
        summary,
        decor,
        tree,
        body,
    ] = fields
    else {
        return None;
    };
    let [
        parents,
        author,
        email,
        time,
        date,
        summary,
        decor,
        tree,
        body,
    ] = [
        parents, author, email, time, date, summary, decor, tree, body,
    ]
    .map(AsRef::as_ref);
    // The line git ends each commit with is before the next one's hash.
    let hash = hash.as_ref().trim_start_matches('\n');
    if hash.is_empty() {
        return None;
    }
    Some(Commit {
        hash: hash.to_owned(),
        parents: parents.split_whitespace().map(str::to_owned).collect(),
        author: author.to_owned(),
        email: email.to_owned(),
        time: time.parse().unwrap_or(0),
        date: date.to_owned(),
        summary: summary.to_owned(),
        body: one_line(body),
        change_id: crate::gerrit::change_id(&format!("{summary}\n\n{body}"))
            .ok()
            .map(str::to_owned),
        refs: decor
            .split(", ")
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        tree: tree.to_owned(),
    })
}

/// Older history page after page, as [`log`] gives it. Each page uses the bounded process
/// service (or persistent remote shell pool). A tab
/// scope can cancel a stalled page without leaving a blocking pipe reader in a job worker.
/// This trades a repeated history walk for keeping a bounded worker available to other tabs.
pub struct LogStream {
    paged: Option<Paged>,
}
struct Paged {
    repo: std::path::PathBuf,
    from: Option<Vec<String>>,
    order: LogOrder,
    skip: usize,
    excluded: Vec<String>,
}
impl LogStream {
    /// The commits of `from` (else every branch) in `order`, after the first `skip`.
    #[cfg(test)]
    pub fn start(
        repo: &Path,
        from: Option<&[String]>,
        order: LogOrder,
        skip: usize,
    ) -> Result<Self, String> {
        Self::start_scoped(
            repo,
            from,
            order,
            skip,
            &super::history_scope::default_hidden_namespaces(),
        )
    }
    pub fn start_scoped(
        repo: &Path,
        from: Option<&[String]>,
        order: LogOrder,
        skip: usize,
        excluded: &[String],
    ) -> Result<Self, String> {
        Ok(Self {
            paged: if from.is_some_and(|revs| revs.is_empty()) {
                None
            } else {
                Some(Paged {
                    repo: repo.to_owned(),
                    from: from.map(<[String]>::to_vec),
                    order,
                    skip,
                    excluded: excluded.to_vec(),
                })
            },
        })
    }
    /// The next `n` commits; fewer at the end of history.
    pub fn next_page(&mut self, n: usize) -> Result<Vec<Commit>, String> {
        let Some(paged) = &mut self.paged else {
            return Ok(Vec::new());
        };
        let page = log_scoped(
            &paged.repo,
            paged.from.as_deref(),
            paged.order,
            paged.skip,
            n,
            &paged.excluded,
        )?;
        paged.skip += page.len();
        Ok(page)
    }
}

/// A commit body as one line: whitespace runs (line breaks included) become single spaces. Long
/// bodies are cut, the graph only shows their start.
fn one_line(body: &str) -> String {
    const MAX_CHARS: usize = 400;
    body.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_CHARS)
        .collect()
}

/// The best common ancestor of two commits.
pub fn merge_base(repo: &Path, a: &str, b: &str) -> Result<String, String> {
    run(Some(repo), &["merge-base", END_OPTS, a, b]).map(|s| s.trim().to_owned())
}

/// The full hash of a branch, tag or abbreviated hash.
pub fn resolve(repo: &Path, rev: &str) -> Result<String, String> {
    not_an_option(rev)?;
    run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
    )
    .map(|s| s.trim().to_owned())
    .map_err(|_| {
        crate::i18n::message(
            "backend-rev-is-not-a-commit-081a0f",
            &[("rev", (rev).to_string().into())],
        )
    })
}

/// How many commits `a` has that `b` hasn't, and the other way round.
pub fn count_apart(repo: &Path, a: &str, b: &str) -> Result<(u32, u32), String> {
    let out = run(
        Some(repo),
        &[
            "rev-list",
            "--left-right",
            "--count",
            END_OPTS,
            &format!("{a}...{b}"),
        ],
    )?;
    let mut n = out.split_whitespace().map(|x| x.parse().unwrap_or(0));
    Ok((n.next().unwrap_or(0), n.next().unwrap_or(0)))
}

/// Where a log starts, the last of its arguments: the revisions `from` when given, else every
/// branch (stashes aside).
pub(super) fn push_revisions<'a>(args: &mut Vec<&'a str>, from: Option<&'a [String]>) {
    match from {
        Some(revs) => {
            args.push(END_OPTS);
            args.extend(revs.iter().map(String::as_str));
            args.push("--");
        }
        // Nor the history of notes (`git notes`), nor what `git maintenance` fetches ahead
        // into `refs/prefetch/`: commits on no branch, not the repository's history. Nor
        // what gitcraft keeps of earlier versions in `refs/gitcraft/` (see `rewrites`).
        None => args.extend([
            "--exclude=refs/stash",
            "--exclude=refs/gitcraft/*",
            "--exclude=refs/notes/*",
            "--exclude=refs/prefetch/*",
            "--exclude=refs/t3/*",
            "--all",
        ]),
    }
}

/// A commit an interactive rebase would rewrite.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RebaseCommit {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub summary: String,
    /// The whole message (summary and body).
    pub message: String,
    pub merge: bool,
}

/// The commits `git rebase -i onto` would list (`onto..HEAD`), oldest first.
pub fn rebase_commits(repo: &Path, onto: &str) -> Result<Vec<RebaseCommit>, String> {
    rebase_commits_in(repo, &format!("{onto}..HEAD"))
}

/// The commits of `range` (`onto..HEAD`, or `HEAD` for all of them), oldest first.
pub(super) fn rebase_commits_in(repo: &Path, range: &str) -> Result<Vec<RebaseCommit>, String> {
    // Each field ended by a NUL, which no commit has: a record separator in a message would
    // cut it short, and rewording the commit would lose the rest.
    let format = "--format=%H%x00%P%x00%an%x00%ae%x00%s%x00%B%x00";
    let out = run(
        Some(repo),
        &[
            "log",
            NO_SIGNATURE,
            UTF8,
            "--reverse",
            format,
            END_OPTS,
            range,
        ],
    )?;
    Ok(parse_rebase_commits(&out))
}

fn parse_rebase_commits(out: &str) -> Vec<RebaseCommit> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .as_chunks::<6>()
        .0
        .iter()
        .filter_map(|[hash, parents, author, email, summary, message]| {
            let hash = hash.trim();
            if hash.is_empty() {
                return None;
            }
            Some(RebaseCommit {
                hash: hash.to_owned(),
                merge: parents.split_whitespace().count() > 1,
                author: (*author).to_owned(),
                email: (*email).to_owned(),
                summary: (*summary).to_owned(),
                message: message.trim_end().to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;

mod search;
pub(crate) use search::investigation_commits_of;
pub(crate) use search::investigation_page;
pub use search::search;
pub use search::search_code;

mod detail;
pub use detail::CommitDetail;
#[allow(unused_imports)] // Compatibility re-export.
pub use detail::MAX_TOOLTIP_MESSAGE_BYTES;
pub use detail::commit_detail;
#[allow(unused_imports)] // Compatibility re-export.
pub(super) use detail::commit_detail_by_git;
pub use detail::commit_message;
pub use detail::head_message;
