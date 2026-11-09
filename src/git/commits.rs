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
    let out = match run(
        Some(repo),
        &log_args(from, order, skip, Some(&limit), excluded)
            .as_args()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    ) {
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

#[derive(Clone, Debug)]
pub struct CommitDetail {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub author_date: String,
    pub committer: String,
    pub commit_date: String,
    pub message: String,
    pub files: Vec<ChangedFile>,
    /// Whether the checked-out branch (HEAD) contains it, so its message can be edited.
    pub in_head: bool,
    /// Whether HEAD has merges after it, which rewording it by a rebase would flatten.
    pub merges_after: bool,
}

/// What a read on a thread of its own gave, or that it stopped (it panicked).
fn joined<T>(handle: std::thread::ScopedJoinHandle<'_, Result<T, String>>) -> Result<T, String> {
    handle
        .join()
        .unwrap_or_else(|_| Err(crate::i18n::text("interaction-teleport-error-stopped").into()))
}

pub fn commit_detail(repo: &Path, hash: &str) -> Result<CommitDetail, String> {
    // NULs between the fields: no commit has one, unlike the unit separator.
    let format = "--format=%H%x00%P%x00%an%x00%ae%x00%ad%x00%cn%x00%cd%x00%B";
    let header_args = [
        "show",
        NO_SIGNATURE,
        UTF8,
        "-s",
        "--date=format-local:%Y-%m-%d %H:%M:%S",
        format,
        END_OPTS,
        hash,
    ];
    let range = format!("{hash}..HEAD");
    // The reads don't depend on each other: all at once, each of them a process start (the
    // larger part of the wait on Windows) rather than five one after the other.
    let (out, files, in_head, merges) = std::thread::scope(|s| {
        let files = {
            let inherited = crate::tasks::Inherited::capture();
            s.spawn(move || inherited.run(|| commit_files(repo, hash)))
        };
        let in_head = {
            let inherited = crate::tasks::Inherited::capture();
            s.spawn(move || {
                inherited.run(|| run(Some(repo), &["merge-base", "--is-ancestor", hash, "HEAD"]))
            })
        };
        let merges = {
            let inherited = crate::tasks::Inherited::capture();
            let range = &range;
            s.spawn(move || {
                inherited.run(|| run(Some(repo), &["rev-list", "--merges", "--count", range]))
            })
        };
        let out = run(Some(repo), &header_args);
        (out, joined(files), joined(in_head), joined(merges))
    });
    let out = out?;
    let f: Vec<&str> = out.splitn(8, '\0').collect();
    let [h, parents, author, email, adate, committer, cdate, message] = f[..] else {
        return Err(crate::i18n::text("backend-unexpected-output-from-git-show-c43bf4").to_owned());
    };
    let files = files?;
    let in_head = in_head.is_ok();
    let merges_after = in_head && merges.is_ok_and(|n| n.trim() != "0");
    Ok(CommitDetail {
        hash: h.to_owned(),
        parents: parents.split_whitespace().map(str::to_owned).collect(),
        author: author.to_owned(),
        email: email.to_owned(),
        author_date: adate.to_owned(),
        committer: committer.to_owned(),
        commit_date: cdate.to_owned(),
        message: message.trim_end().to_owned(),
        files,
        in_head,
        merges_after,
    })
}

/// Maximum object and decoded message size retained for a graph tooltip.
pub const MAX_TOOLTIP_MESSAGE_BYTES: usize = 256 * 1024;

/// Read one original message without expanding every graph row's compact preview.
/// Check object size before reading it, and cap the decoded text retained by the UI.
pub fn commit_message(repo: &Path, hash: &str) -> Result<String, String> {
    if !matches!(hash.len(), 40 | 64) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(
            crate::i18n::text("backend-expected-a-full-commit-object-id-edce46").to_owned(),
        );
    }
    let commit = format!("{hash}^{{commit}}");
    let size = run(Some(repo), &["cat-file", "-s", &commit])?
        .trim()
        .parse::<usize>()
        .map_err(|_| crate::i18n::text("backend-invalid-commit-object-size-0ac3de").to_owned())?;
    let too_large = || {
        crate::i18n::text(
            "backend-message-exceeds-the-256-kib-tooltip-limit-select-the-commit-to-re-ebcc66",
        )
        .to_owned()
    };
    if size > MAX_TOOLTIP_MESSAGE_BYTES {
        return Err(too_large());
    }
    let message = run(
        Some(repo),
        &[
            "show",
            NO_SIGNATURE,
            UTF8,
            "-s",
            "--format=%B",
            END_OPTS,
            &commit,
        ],
    )?;
    if message.len() > MAX_TOOLTIP_MESSAGE_BYTES {
        return Err(too_large());
    }
    Ok(message.trim_end().to_owned())
}

/// The full message of HEAD (to amend it).
pub fn head_message(repo: &Path) -> Result<String, String> {
    run(
        Some(repo),
        &["log", NO_SIGNATURE, UTF8, "-1", "--format=%B"],
    )
    .map(|s| s.trim_end().to_owned())
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

/// Commits whose message or author matches `query` (case-insensitive), or whose hash starts
/// with it, newest first; among those of the revisions `from` when given, read in `order`, as
/// `log`.
pub fn search(
    repo: &Path,
    from: Option<&[String]>,
    order: LogOrder,
    query: &str,
) -> Result<Vec<String>, String> {
    if from.is_some_and(|revs| revs.is_empty()) {
        return Ok(Vec::new());
    }
    let mut hashes: Vec<String> = Vec::new();
    // A short query matches most of history: a linear check per hash would be quadratic.
    let mut seen: HashSet<String> = HashSet::new();
    let mut base = vec!["log", NO_SIGNATURE, UTF8];
    base.extend(order.args());
    base.extend(["-i", "--format=%H"]);
    for filter in [format!("--grep={query}"), format!("--author={query}")] {
        let mut args = base.clone();
        args.push("--fixed-strings");
        args.push(&filter);
        push_revisions(&mut args, from);
        if let Ok(out) = run(Some(repo), &args) {
            for h in out.lines() {
                if seen.insert(h.to_owned()) {
                    hashes.push(h.to_owned());
                }
            }
        }
    }
    let q = query.trim();
    if q.len() >= 4
        && q.chars().all(|c| c.is_ascii_hexdigit())
        && let Ok(h) = run(
            Some(repo),
            &["rev-parse", "--verify", "-q", &format!("{q}^{{commit}}")],
        )
    {
        // First, and once: a tag `2024` may be on a commit whose message has `2024` in it.
        let h = h.trim();
        hashes.retain(|x| x != h);
        hashes.insert(0, h.to_owned());
    }
    Ok(hashes)
}

/// Commits that added or removed `query` in a file's contents (`git log -S`), newest first;
/// among those of the revisions `from` when given, read in `order`, as `log`.
pub fn search_code(
    repo: &Path,
    from: Option<&[String]>,
    order: LogOrder,
    query: &str,
) -> Result<Vec<String>, String> {
    if from.is_some_and(|revs| revs.is_empty()) {
        return Ok(Vec::new());
    }
    let pickaxe = format!("-S{query}");
    let mut args = vec!["log", NO_SIGNATURE];
    args.extend(order.args());
    args.extend(["--format=%H", pickaxe.as_str()]);
    push_revisions(&mut args, from);
    let out = run(Some(repo), &args)?;
    Ok(out.lines().map(str::to_owned).collect())
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

/// The relation view's explicit older-context read; reuse the ordinary log format/parser.
pub(crate) fn investigation_page(
    reader: &super::investigations::Reader<'_>,
    tips: &[String],
    order: LogOrder,
    skip: usize,
    limit: usize,
) -> Result<Vec<Commit>, String> {
    let max = format!("--max-count={limit}");
    let args = log_args(Some(tips), order, skip, Some(&max), &[]);
    let output = reader.run(
        &args
            .as_args()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        Vec::new(),
        &[0],
    )?;
    parse_investigation_commits(&output)
}

pub(crate) fn investigation_commits_of(
    reader: &super::investigations::Reader<'_>,
    hashes: &[String],
) -> Result<Vec<Commit>, String> {
    if hashes.len() > super::LOG_PAGE || hashes.iter().any(|s| !super::investigations::oid(s)) {
        return Err(crate::i18n::text("backend-invalid-pinned-commit-request-ed2691").into());
    }
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    let format = log_args(None, LogOrder::default(), 0, None, &[]).format;
    let mut result = Vec::new();
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
        result.extend(parse_investigation_commits(&reader.run(
            &args,
            vec![],
            &[0],
        )?)?);
    }
    Ok(result)
}

fn parse_investigation_commits(output: &[u8]) -> Result<Vec<Commit>, String> {
    let text = std::str::from_utf8(output).map_err(|_| {
        crate::i18n::text("backend-history-page-returned-non-utf-8-metadata-333f22")
    })?;
    let fields: Vec<_> = text.split('\0').collect();
    let (records, tail) = fields.as_chunks::<LOG_FIELDS>();
    if !tail.iter().all(|s| s.trim().is_empty()) {
        return Err(
            crate::i18n::text("backend-history-page-returned-truncated-metadata-cc11d9").into(),
        );
    }
    records
        .iter()
        .map(|record| {
            parse_record(record)
                .filter(|c| {
                    super::investigations::oid(&c.hash)
                        && c.parents.iter().all(|p| super::investigations::oid(p))
                })
                .ok_or_else(|| {
                    crate::i18n::text(
                        "backend-history-page-returned-invalid-commit-identity-8d0985",
                    )
                    .into()
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every branch's history leaves out the commits of notes and of `git maintenance`'s
    /// prefetching, which are on no branch.
    #[test]
    fn history_of_branches_has_no_notes_or_prefetched_commits() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| run(Some(&dir), args);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["commit", "-q", "--allow-empty", "-m", "one"],
            &["notes", "add", "-m", "a note", "HEAD"],
            &["tag", "v1"],
        ] {
            git(args)?;
        }
        let ahead = git(&[
            "commit-tree",
            "-p",
            "HEAD",
            "-m",
            "fetched ahead",
            "HEAD^{tree}",
        ])?;
        git(&[
            "update-ref",
            "refs/prefetch/remotes/origin/main",
            ahead.trim(),
        ])?;
        let whole = log(&dir, None, Default::default(), 0, 10);
        let found = search(&dir, None, Default::default(), "a");
        let _ = std::fs::remove_dir_all(&dir);
        let summaries: Vec<String> = whole?.into_iter().map(|c| c.summary).collect();
        assert_eq!(summaries, ["one"]);
        assert!(found?.is_empty());
        Ok(())
    }

    /// A message with the unit and record separators in it (pasted from somewhere) is one
    /// commit like any other: in the graph, page after page, its details and a rebase.
    #[test]
    fn separators_in_a_message_are_only_text() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-separators-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| run(Some(&dir), args);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "odd \x1f one\n\nbody \x1f a \x1e b \x1e",
            ],
            &["commit", "-q", "--allow-empty", "-m", "top"],
        ] {
            git(args)?;
        }
        let odd = git(&["rev-parse", "HEAD~1"])?;
        let odd = odd.trim();
        let whole = log(&dir, None, Default::default(), 0, 10);
        let paged =
            LogStream::start(&dir, None, Default::default(), 0).and_then(|mut s| s.next_page(10));
        let detail = commit_detail(&dir, odd);
        let rebase = rebase_commits(&dir, "HEAD~2");
        let _ = std::fs::remove_dir_all(&dir);
        for commits in [whole?, paged?] {
            let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
            assert_eq!(summaries, ["top", "odd \x1f one", "base"]);
            assert_eq!(commits[1].body, "body \x1f a \x1e b \x1e");
            assert_eq!(commits[0].parents, [odd]);
        }
        let detail = detail?;
        assert_eq!((detail.author.as_str(), detail.parents.len()), ("t", 1));
        assert_eq!(detail.message, "odd \x1f one\n\nbody \x1f a \x1e b \x1e");
        let rebase = rebase?;
        assert_eq!(rebase.len(), 2);
        assert_eq!(rebase[0].message, detail.message);
        assert_eq!(rebase[1].summary, "top");
        Ok(())
    }

    #[test]
    fn bodies_fit_on_one_line() {
        assert_eq!(
            one_line(
                "First line
second.

- a
- b
"
            ),
            "First line second. - a - b"
        );
        assert_eq!(one_line(""), "");
        assert_eq!(one_line(&"x ".repeat(500)).chars().count(), 400);
    }
}
