//! Read patch series and resolve available comparison sources.
use super::*;

/// The patches of the commits of `series` (the first [`MAX_PATCHES`] of them), by hash; and
/// whether there were more.
pub fn patches(repo: &Path, series: &Series) -> Result<(HashMap<String, Patch>, bool), String> {
    for rev in [&series.base, &series.tip] {
        not_an_option(rev)?;
    }
    let range = format!("{}..{}", series.base, series.tip);
    let limit = format!("--max-count={}", MAX_PATCHES + 1);
    let out = git::run(
        Some(repo),
        &[
            "-c",
            "core.quotePath=false",
            "log",
            "--no-show-signature",
            "--no-merges",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "-p",
            "--format=commit %H",
            &limit,
            &range,
            "--",
        ],
    )?;
    let mut found = parse_patches(&out);
    let more = found.len() > MAX_PATCHES;
    if more {
        // The oldest is left out: `log` lists newest first, so it is the last one parsed.
        let last = out
            .lines()
            .rev()
            .find_map(|l| l.strip_prefix("commit "))
            .map(str::to_owned);
        if let Some(last) = last {
            found.remove(&last);
        }
    }
    // patch-id reads what `log -p` printed; a commit changing nothing has none.
    if let Ok(ids) = git::run_input(Some(repo), &["patch-id", "--stable"], &out) {
        for line in ids.lines() {
            if let Some((id, commit)) = line.split_once(' ')
                && let Some(p) = found.get_mut(commit.trim())
            {
                p.id = Some(id.to_owned());
            }
        }
    }
    Ok((found, more))
}

/// `log -p --format='commit %H'`: the changed lines of each commit, by hash.
pub(crate) fn parse_patches(out: &str) -> HashMap<String, Patch> {
    let mut found: HashMap<String, Patch> = HashMap::new();
    let mut current: Option<String> = None;
    let mut file = String::new();
    let mut in_hunk = false;
    for line in out.lines() {
        if let Some(hash) = line.strip_prefix("commit ") {
            current = Some(hash.trim().to_owned());
            found.entry(hash.trim().to_owned()).or_default();
            in_hunk = false;
            continue;
        }
        let Some(hash) = &current else {
            continue;
        };
        if line.starts_with("diff --git ") {
            in_hunk = false;
            file.clear();
            continue;
        }
        if !in_hunk {
            if let Some(path) = line.strip_prefix("+++ b/") {
                file = path.trim_end_matches('\t').to_owned();
            } else if let Some(path) = line.strip_prefix("--- a/")
                && file.is_empty()
            {
                file = path.trim_end_matches('\t').to_owned();
            }
        }
        if line.starts_with("@@") {
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        let (sign, text) = match line.split_at_checked(1) {
            Some(("+", text)) => ('+', text),
            Some(("-", text)) => ('-', text),
            _ => continue,
        };
        let text = text.trim();
        // Blank lines and lone braces match everywhere: they say nothing of where a change went.
        if !text.chars().any(char::is_alphanumeric) {
            continue;
        }
        if let Some(p) = found.get_mut(hash)
            && p.lines.len() < MAX_LINES
        {
            p.lines.push(format!("{sign}{file}\u{1f}{text}"));
        }
    }
    found
}

/// The two versions `old` and `new` compared and classed; `names` the tips as the provenance
/// names them (`before`, `refs/heads/x`…), `scope` what is compared.
pub fn read(
    repo: &Path,
    old: &Series,
    new: &Series,
    names: (&str, &str),
    scope: &str,
) -> Result<Ribbons, String> {
    let diff = git::range_diff(repo, old, new)?;
    let (mut read, more_old) = patches(repo, old)?;
    let (new_patches, more_new) = patches(repo, new)?;
    read.extend(new_patches);
    let mut ribbons = classify(&diff, &read);
    ribbons.series = Some((old.clone(), new.clone()));
    let mut provenance = Provenance::new("git range-diff, patch-id", scope)
        .tip(names.0, &old.tip)
        .tip(names.1, &new.tip);
    if more_old || more_new {
        provenance = provenance.partial(format!(
            "more than {MAX_PATCHES} commits: those past them are paired by range-diff only, \
             not checked for the same patch-id, squashes or splits"
        ));
    }
    ribbons.provenance = provenance;
    Ok(ribbons)
}

/// How a pair of commits joined by a ribbon differ, as range-diff prints it: range-diff's own
/// body for a pair it made, else [`alone_patch`].
#[cfg(test)]
pub fn ribbon_patch(repo: &Path, ribbons: &Ribbons, at: usize) -> Result<Vec<String>, String> {
    let r = ribbons.ribbons.get(at).ok_or("no such ribbon")?;
    if let Some(pair) = r.pair.and_then(|p| ribbons.diff.pairs.get(p)) {
        return Ok(pair.body.clone());
    }
    let (old, new) = (
        &ribbons.old[r.old].commit.hash,
        &ribbons.new[r.new].commit.hash,
    );
    alone_patch(repo, old, new)
}

/// How commit `new` differs from `old`, for a ribbon range-diff didn't make: the two compared
/// alone, or when range-diff doesn't pair even those, the old patch (`-`) and then the new one
/// (`+`).
pub fn alone_patch(repo: &Path, old: &str, new: &str) -> Result<Vec<String>, String> {
    let alone = |hash: &str| Series::new(&format!("{hash}^"), hash);
    if let Ok(diff) = git::range_diff(repo, &alone(old), &alone(new))
        && let Some(pair) = diff
            .pairs
            .iter()
            .find(|p| p.old.is_some() && p.new.is_some())
    {
        return Ok(pair.body.clone());
    }
    let mut body = git::lone_patch(repo, old, '-')?;
    body.extend(git::lone_patch(repo, new, '+')?);
    Ok(body)
}

/// What can be compared: a rewrite gitcraft kept, a branch and its upstream, a pull request
/// as last seen and now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Rewrite(Rewrite),
    /// A local branch (short name) and its upstream (`origin/x`), diverged.
    Upstream {
        branch: String,
        upstream: String,
    },
    /// A pull request whose head moved since the version of it seen: its number, title, head
    /// now, and its base branch.
    Pr {
        number: u64,
        title: String,
        head: String,
        base: String,
    },
}

impl Source {
    pub fn label(&self) -> String {
        match self {
            Source::Rewrite(r) => format!(
                "{} before and after its rewrite ({})",
                r.branch,
                git::relative_time(r.time, git::now())
            ),
            Source::Upstream { branch, upstream } => format!("{upstream} and {branch}"),
            Source::Pr { number, title, .. } => format!("#{number} since last seen: {title}"),
        }
    }

    /// Two branches neither of which is the older: a commit only on one side is "only on the
    /// left / right", not dropped or added.
    pub fn sides(&self) -> bool {
        matches!(self, Source::Upstream { .. })
    }
}

/// A local branch as [`sources`] takes it: name, upstream, (ahead, behind).
pub type Tracking = (String, String, (u32, u32));

/// A pull request as [`sources`] takes it: number, title, head, base branch, and the head of
/// the version last seen (if one was).
pub type PrSeen = (u64, String, String, String, Option<String>);

/// Everything there is to compare: the rewrites kept (newest first), the branches diverged from
/// their upstream (the checked-out one first), the pull requests changed since last seen.
/// Empty when there is nothing.
pub fn sources(
    rewrites: &[Rewrite],
    branches: &[Tracking],
    current: Option<&str>,
    prs: &[PrSeen],
) -> Vec<Source> {
    let mut list: Vec<Source> = rewrites.iter().cloned().map(Source::Rewrite).collect();
    let mut diverged: Vec<&Tracking> = branches
        .iter()
        .filter(|(_, up, (ahead, behind))| !up.is_empty() && *ahead > 0 && *behind > 0)
        .collect();
    diverged.sort_by_key(|(name, _, _)| Some(name.as_str()) != current);
    list.extend(diverged.into_iter().map(|(name, up, _)| Source::Upstream {
        branch: name.clone(),
        upstream: up.clone(),
    }));
    list.extend(
        prs.iter()
            .filter(|(_, _, head, _, seen)| seen.as_ref().is_some_and(|s| s != head))
            .map(|(number, title, head, base, _)| Source::Pr {
                number: *number,
                title: title.clone(),
                head: head.clone(),
                base: base.clone(),
            }),
    );
    list
}

/// The version of pull request `number` seen (see [`git::reviewed`]) and the one now (`head`,
/// from where it forks off `base_refs`, the first that is here): nothing is fetched, so a head
/// that isn't here is an error.
pub fn pr_series(
    repo: &Path,
    repo_key: &str,
    number: u64,
    head: &str,
    base_refs: &[String],
) -> Result<(Series, Series), String> {
    let old = git::reviewed(repo, repo_key, number)?
        .ok_or_else(|| format!("no version of #{number} was seen in gitcraft"))?;
    not_an_option(head)?;
    if git::run(
        Some(repo),
        &["cat-file", "-e", &format!("{head}^{{commit}}")],
    )
    .is_err()
    {
        return Err(format!(
            "the head of #{number} ({}) isn't in this repository yet: open its page once, \
             which fetches it",
            crate::repo::short(head)
        ));
    }
    let base = base_refs
        .iter()
        .chain(std::iter::once(&old.base))
        .find_map(|b| git::merge_base(repo, b, head).ok())
        .ok_or_else(|| format!("can't tell where the commits of #{number} start"))?;
    Ok((old, Series::new(&base, head)))
}

/// `rev` as git takes it among other arguments: never as an option.
pub(crate) fn not_an_option(rev: &str) -> Result<&str, String> {
    if rev.starts_with('-') || rev.is_empty() {
        Err(format!("{rev:?} isn't a revision"))
    } else {
        Ok(rev)
    }
}
