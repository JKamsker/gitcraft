//! Cutting a release from a commit: the version the Conventional Commits since the last tag call
//! for, notes grouped by type, the version moved in the manifests (and a changelog), an
//! annotated (or signed) tag, the push, and the release on the forge with its files. Each step
//! is done and recorded on its own, so that a release that failed halfway can go on from where
//! it stopped.

pub(crate) mod conventional;
pub(crate) mod manifest;
pub(crate) mod notes;
#[cfg(test)]
mod tests;
pub(crate) mod version;

use std::path::{Path, PathBuf};

use crate::forge::{Forge, Hosting};
use crate::git;
use conventional::Change;
use manifest::Kind;
use version::Version;

/// What a release starts from, read from the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Draft {
    /// The commit released.
    pub commit: String,
    /// The last tag before it, if any.
    pub last_tag: Option<String>,
    pub changes: Vec<Change>,
    /// The version suggested.
    pub suggested: Version,
    /// The branch checked out, when the commit is its tip: only then can a commit move the
    /// version in the manifests.
    pub branch: Option<String>,
    /// The manifests at the commit and their version, those that can be moved (clean here).
    pub manifests: Vec<(Kind, String)>,
    /// Manifests that can't be moved now, and why.
    pub manifest_note: Option<String>,
    /// `CHANGELOG.md` is there.
    pub changelog: bool,
    /// git signs tags by default (`tag.gpgSign`).
    pub sign: bool,
    /// The remote to push to.
    pub remote: Option<String>,
    /// Where on it the branch goes (`refs/heads/main`).
    pub remote_ref: Option<String>,
    /// The tags that exist, to refuse one again.
    pub tags: Vec<String>,
}

/// The file the changelog is in.
pub(crate) const CHANGELOG: &str = "CHANGELOG.md";

/// Reads what a release of `commit` starts from. `remote`: the one to push to.
pub(crate) fn draft(repo: &Path, commit: &str, remote: Option<String>) -> Result<Draft, String> {
    let commit = git::resolve(repo, &format!("{commit}^{{commit}}"))?;
    // The last tag reachable, and the version it says (tags that aren't versions are passed
    // over by git's own `--match` only loosely: checked here).
    let last_tag = git::run_ok_codes(
        Some(repo),
        &[
            "describe",
            "--tags",
            "--abbrev=0",
            "--match",
            "v[0-9]*",
            "--match",
            "[0-9]*",
            &commit,
        ],
        &[0, 128],
    )
    .ok()
    .map(|s| s.trim().to_owned())
    .filter(|s| !s.is_empty());
    let range = match &last_tag {
        Some(tag) => format!("refs/tags/{tag}..{commit}"),
        None => commit.clone(),
    };
    let changes = read_changes(repo, &range)?;
    let head = git::head(repo);
    let branch = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?
        .trim()
        .strip_prefix("refs/heads/")
        .map(str::to_owned)
        .filter(|_| head == commit);
    let mut manifests = Vec::new();
    for kind in Kind::ALL {
        let (file, _) = kind.files();
        if let Ok(bytes) = git::file_at(repo, Some(&commit), file)
            && let Some(v) = kind.version(&String::from_utf8_lossy(&bytes))
        {
            manifests.push((kind, v));
        }
    }
    let changelog = git::file_at(repo, Some(&commit), CHANGELOG).is_ok();
    let manifest_note = if manifests.is_empty() && !changelog {
        None
    } else if branch.is_none() {
        Some("Only the tip of the branch checked out can move the version in its files".to_owned())
    } else {
        let mut files: Vec<&str> = manifests
            .iter()
            .flat_map(|(k, _)| {
                let (a, b) = k.files();
                [a, b]
            })
            .collect();
        if changelog {
            files.push(CHANGELOG);
        }
        let mut args = vec!["status", "--porcelain", "--"];
        args.extend(&files);
        let dirty = git::run(Some(repo), &args)?;
        (!dirty.trim().is_empty())
            .then(|| "Commit or stash the changes to the files the version is in first".to_owned())
    };
    if manifest_note.is_some() {
        manifests.clear();
    }
    let sign = git::run_ok_codes(
        Some(repo),
        &["config", "--bool", "--get", "tag.gpgSign"],
        &[0, 1],
    )
    .is_ok_and(|v| v.trim() == "true");
    let remote_ref = branch.as_deref().map(|b| {
        let out = git::run(
            Some(repo),
            &[
                "for-each-ref",
                "--format=%(upstream:remotename)%00%(upstream:remoteref)",
                &format!("refs/heads/{b}"),
            ],
        )
        .unwrap_or_default();
        let mut parts = out.trim().split('\0');
        match (parts.next(), parts.next()) {
            (Some(r), Some(dst)) if Some(r) == remote.as_deref() && !dst.is_empty() => {
                dst.to_owned()
            }
            _ => format!("refs/heads/{b}"),
        }
    });
    let tags: Vec<String> = git::run(Some(repo), &["tag", "--list"])?
        .lines()
        .map(str::to_owned)
        .collect();
    let base = last_tag.as_deref().and_then(Version::parse).or_else(|| {
        manifests
            .first()
            .and_then(|(_, v)| Version::parse(&format!("v{v}")))
    });
    let suggested = match (&base, conventional::bump_of(&changes)) {
        // A manifest's version not tagged yet is the first release's.
        (Some(v), _) if last_tag.is_none() => v.clone(),
        (Some(v), Some(bump)) => v.next(bump),
        (Some(v), None) => v.clone(),
        (None, _) => Version {
            prefix: "v".into(),
            major: 0,
            minor: 1,
            patch: 0,
            pre: String::new(),
        },
    };
    Ok(Draft {
        commit,
        last_tag,
        changes,
        suggested,
        branch,
        manifests,
        manifest_note,
        changelog,
        sign,
        remote,
        remote_ref,
        tags,
    })
}

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

fn read_changes(repo: &Path, range: &str) -> Result<Vec<Change>, String> {
    let out = git::run(
        Some(repo),
        &[
            "log",
            "--no-merges",
            "--format=%H%x1f%s%x1f%b%x1e",
            range,
            "--",
        ],
    )?;
    Ok(out
        .split(RECORD)
        .filter_map(|r| {
            let mut f = r.trim_start_matches('\n').splitn(3, FIELD);
            let hash = f.next()?.trim();
            if hash.is_empty() {
                return None;
            }
            Some(Change::parse(
                hash,
                f.next().unwrap_or_default(),
                f.next().unwrap_or_default(),
            ))
        })
        .collect())
}

/// Where a pull request numbered `n` is on `hosting`.
pub(crate) fn pr_url(hosting: &Hosting, n: u64) -> String {
    let base = format!("{}/{}", hosting.web, hosting.path);
    match hosting.forge {
        Forge::GitHub => format!("{base}/pull/{n}"),
        Forge::GitLab => format!("{base}/-/merge_requests/{n}"),
        Forge::Forgejo | Forge::Gitea => format!("{base}/pulls/{n}"),
    }
}

/// What the release is to be, as chosen in its dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub commit: String,
    pub tag: String,
    pub notes: String,
    /// The branch the version commit goes on, and the manifests it moves to `version`.
    pub bump: Option<Bump>,
    pub sign: bool,
    /// The remote to push the tag (and the version commit) to.
    pub push: Option<String>,
    /// Make the release on the forge.
    pub publish: bool,
    pub prerelease: bool,
    /// Files attached to the forge's release.
    pub assets: Vec<PathBuf>,
}

/// The commit that moves the version in the files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bump {
    pub branch: String,
    /// The branch on the remote the version commit is pushed to (`refs/heads/main`): the
    /// branch's upstream there, else the branch of its name.
    pub remote_ref: String,
    /// The version written, without its prefix.
    pub version: String,
    pub manifests: Vec<(Kind, String)>,
    pub changelog: bool,
}

/// One step of a release, done on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Bump,
    Tag,
    Push,
    Publish,
    Upload,
}

impl Step {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Step::Bump => "Move the version",
            Step::Tag => "Tag",
            Step::Push => "Push",
            Step::Publish => "Make the release on the forge",
            Step::Upload => "Attach the files",
        }
    }
}

impl Plan {
    /// The steps, in order.
    pub(crate) fn steps(&self) -> Vec<Step> {
        let mut steps = Vec::new();
        if self.bump.is_some() {
            steps.push(Step::Bump);
        }
        steps.push(Step::Tag);
        if self.push.is_some() {
            steps.push(Step::Push);
        }
        if self.publish {
            steps.push(Step::Publish);
            if !self.assets.is_empty() {
                steps.push(Step::Upload);
            }
        }
        steps
    }

    /// What each step will do, in words and commands, for the dialog.
    pub(crate) fn preview(&self, forge: Option<Forge>) -> Vec<(Step, String)> {
        let short = crate::repo::short(&self.commit);
        self.steps()
            .into_iter()
            .map(|step| {
                let text = match step {
                    Step::Bump => {
                        let b = self.bump.as_ref();
                        let mut files: Vec<String> = b
                            .map(|b| {
                                b.manifests
                                    .iter()
                                    .map(|(k, from)| {
                                        format!("{} ({from} → {})", k.files().0, b.version)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if b.is_some_and(|b| b.changelog) {
                            files.push(format!("{CHANGELOG} (the notes on top)"));
                        }
                        format!(
                            "Commits {} on {}: “chore(release): {}”",
                            files.join(", "),
                            b.map(|b| b.branch.as_str()).unwrap_or_default(),
                            self.tag
                        )
                    }
                    Step::Tag => format!(
                        "Makes the {} tag {} at {}, its message the notes (git tag {} {})",
                        if self.sign { "signed" } else { "annotated" },
                        self.tag,
                        if self.bump.is_some() {
                            "that commit".to_owned()
                        } else {
                            short.to_owned()
                        },
                        if self.sign { "-s" } else { "-a" },
                        self.tag
                    ),
                    Step::Push => {
                        let remote = self.push.as_deref().unwrap_or_default();
                        match &self.bump {
                            Some(b) => format!(
                                "Pushes the version commit to {remote}'s {} and the tag to \
                                 {remote}, both or neither (git push --atomic)",
                                b.remote_ref
                                    .strip_prefix("refs/heads/")
                                    .unwrap_or(&b.remote_ref)
                            ),
                            None => format!("Pushes the tag to {remote}"),
                        }
                    }
                    Step::Publish => format!(
                        "Makes the {}release {} on {} with the notes",
                        if self.prerelease { "pre-" } else { "" },
                        self.tag,
                        forge.map_or("the forge", Forge::name)
                    ),
                    Step::Upload => format!(
                        "Attaches {} to it",
                        self.assets
                            .iter()
                            .map(|a| a
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                (step, text)
            })
            .collect()
    }

    /// The tag's message: the tag, then the notes.
    fn tag_message(&self) -> String {
        if self.notes.trim().is_empty() {
            self.tag.clone()
        } else {
            format!("{}\n\n{}", self.tag, self.notes.trim())
        }
    }
}

/// What the local steps left, for undoing them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Done {
    /// The version commit made.
    pub bump_commit: Option<String>,
    /// The tag object made (not the commit it points to).
    pub tag_object: Option<String>,
    /// The commit the tag points to.
    pub tag_commit: Option<String>,
    /// The URL of the release on the forge.
    pub url: Option<String>,
}

/// Runs `step` of `plan` in `repo`, with what the steps before left in `done`.
pub(crate) fn run_step(
    repo: &Path,
    plan: &Plan,
    step: Step,
    done: &mut Done,
    creds: Option<&git::Credentials>,
    hosting: Option<&Hosting>,
) -> Result<(), String> {
    if matches!(step, Step::Bump | Step::Tag | Step::Push) {
        preflight(repo, plan, hosting)?;
    }
    match step {
        Step::Bump => {
            let bump = plan.bump.as_ref().ok_or("nothing to move")?;
            done.bump_commit = Some(commit_bump(repo, plan, bump)?);
        }
        Step::Tag => {
            let target = done
                .bump_commit
                .clone()
                .unwrap_or_else(|| plan.commit.clone());
            let kind = if plan.sign { "-s" } else { "-a" };
            let message = plan.tag_message();
            // Unticked, not signed even where tag.gpgSign asks for every tag to be.
            let sign = if plan.sign {
                "tag.gpgSign=true"
            } else {
                "tag.gpgSign=false"
            };
            git::run(
                Some(repo),
                &[
                    "-c",
                    sign,
                    "tag",
                    kind,
                    "--cleanup=whitespace",
                    "-m",
                    &message,
                    "--",
                    &plan.tag,
                    &target,
                ],
            )?;
            let tag_ref = format!("refs/tags/{}", plan.tag);
            // The tag object itself: putting it back on redo keeps the notes and signature.
            done.tag_object = Some(
                git::run(Some(repo), &["rev-parse", "--verify", &tag_ref])?
                    .trim()
                    .to_owned(),
            );
            done.tag_commit = Some(git::resolve(repo, &tag_ref)?);
        }
        Step::Push => {
            let remote = plan.push.as_deref().ok_or("no remote to push to")?;
            let mut args = vec![
                "push".to_owned(),
                "--atomic".to_owned(),
                "--".to_owned(),
                remote.to_owned(),
            ];
            // The version commit itself (not what the branch has become since), to the branch
            // it goes to there.
            if let (Some(bump), Some(commit)) = (&plan.bump, &done.bump_commit) {
                args.push(format!("{commit}:{}", bump.remote_ref));
            }
            args.push(format!("refs/tags/{0}:refs/tags/{0}", plan.tag));
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            git::run_with(Some(repo), &args, creds)?;
        }
        Step::Publish => {
            let hosting = hosting.ok_or("The repository isn't on a forge gitcraft knows")?;
            check_publication_destination(repo, plan, hosting)?;
            let url = hosting.forge.provider().create_release(
                repo,
                hosting,
                &crate::forge::NewRelease {
                    tag: &plan.tag,
                    title: &plan.tag,
                    notes: &plan.notes,
                    prerelease: plan.prerelease,
                },
            )?;
            done.url = Some(url);
        }
        Step::Upload => {
            let hosting = hosting.ok_or("The repository isn't on a forge gitcraft knows")?;
            check_publication_destination(repo, plan, hosting)?;
            hosting
                .forge
                .provider()
                .upload_assets(repo, hosting, &plan.tag, &plan.assets)?;
        }
    }
    Ok(())
}

/// Validate a publishing plan before executing it, and again immediately before each
/// local mutation and push. Resuming preserves the completed steps even when this refuses.
pub(crate) fn preflight(repo: &Path, plan: &Plan, hosting: Option<&Hosting>) -> Result<(), String> {
    if !plan.publish {
        return Ok(());
    }
    let hosting = hosting.ok_or("The repository isn't on a forge gitcraft knows")?;
    check_publication_destination(repo, plan, hosting)
}

/// A push URL, URL rewrite, or a remote edited during the release must not receive a tag
/// intended for a different publishing repository. Also checked before forge mutations,
/// including resumed uploads; Git's configured transport routing itself is never rewritten.
fn check_publication_destination(
    repo: &Path,
    plan: &Plan,
    hosting: &Hosting,
) -> Result<(), String> {
    let remote = plan
        .push
        .as_deref()
        .ok_or("Publication needs the release's push destination")?;
    let urls = git::run(
        Some(repo),
        &["remote", "get-url", "--push", "--all", "--", remote],
    )?;
    let expected = format!("{}/{}", hosting.web.trim_end_matches('/'), hosting.path);
    let urls = urls
        .lines()
        .filter(|url| !url.trim().is_empty())
        .collect::<Vec<_>>();
    if urls.is_empty()
        || !urls
            .iter()
            .all(|url| same_publication_repository(url, hosting))
    {
        return Err(format!(
            "Release publication stopped: the push destination of {remote} does not match the selected forge repository {expected}. Local release changes remain undoable."
        ));
    }
    Ok(())
}

/// Compare the web authority separately from the repository path. DNS names are case
/// insensitive; GitLab's captured subgroup/project spelling must remain exact. GitHub,
/// Forgejo and Gitea identify owner/repository names without regard to ASCII case.
fn same_publication_repository(url: &str, hosting: &Hosting) -> bool {
    let Some((host, path)) = crate::forge::location(url) else {
        return false;
    };
    if !host.eq_ignore_ascii_case(&hosting.host) {
        return false;
    }
    // SSH identifies the Git repository independently of the forge's configured
    // web port or reverse-proxy prefix. HTTP transports must also match the web
    // authority and scheme; a different port can serve another repository.
    let web_transport = url.split_once("://").is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    });
    if web_transport {
        return hosting.matches_http_repository(url);
    }
    hosting.same_repository_path(&path)
}

/// Writes the new version (and the notes in the changelog) and commits those files only.
fn commit_bump(repo: &Path, plan: &Plan, bump: &Bump) -> Result<String, String> {
    let checked_out = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    if checked_out.trim() != format!("refs/heads/{}", bump.branch) || git::head(repo) != plan.commit
    {
        return Err(format!(
            "{} isn't checked out at {} any more: plan the release again",
            bump.branch,
            crate::repo::short(&plan.commit)
        ));
    }
    // Changes made to those files since the dialog opened are never written over.
    let touched = bump_files(repo, bump);
    let mut args = vec!["status", "--porcelain", "--"];
    args.extend(touched.iter().map(String::as_str));
    if !git::run(Some(repo), &args)?.trim().is_empty() {
        return Err(format!(
            "{} changed since the release was planned: commit or stash that first",
            touched.join(", ")
        ));
    }
    let written = write_bump(repo, plan, bump);
    let files = match written {
        Ok(files) => files,
        Err(e) => {
            restore(repo, &touched);
            return Err(e);
        }
    };
    let message = format!("chore(release): {}", plan.tag);
    let mut args = vec!["commit", "-q", "-m", &message, "--"];
    args.extend(files.iter().map(String::as_str));
    if let Err(e) = git::run(Some(repo), &args) {
        // A hook refused it: the files go back as they were.
        restore(repo, &files);
        return Err(e);
    }
    Ok(git::head(repo))
}

/// The files the version commit may change (those of them at HEAD).
fn bump_files(repo: &Path, bump: &Bump) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for (kind, _) in &bump.manifests {
        let (manifest, lock) = kind.files();
        files.push(manifest.to_owned());
        if git::file_at(repo, Some("HEAD"), lock).is_ok() {
            files.push(lock.to_owned());
        }
    }
    if bump.changelog {
        files.push(CHANGELOG.to_owned());
    }
    files
}

/// Puts `files` back as they are at HEAD.
fn restore(repo: &Path, files: &[String]) {
    let mut args = vec!["checkout", "HEAD", "--"];
    args.extend(files.iter().map(String::as_str));
    let _ = git::run(Some(repo), &args);
}

/// Writes the new version and notes; the files written.
fn write_bump(repo: &Path, plan: &Plan, bump: &Bump) -> Result<Vec<String>, String> {
    let mut files: Vec<String> = Vec::new();
    for (kind, old) in &bump.manifests {
        let (manifest, lock) = kind.files();
        let text = read_at_head(repo, manifest)?;
        let new = kind
            .bump(&text, &bump.version)
            .ok_or_else(|| format!("Couldn't find the version in {manifest}"))?;
        write(repo, manifest, &new)?;
        files.push(manifest.to_owned());
        let names: Vec<String> = kind.own_name(&text).into_iter().collect();
        if let Ok(text) = read_at_head(repo, lock)
            && let Some(new) = kind.bump_lock(&text, &names, old, &bump.version)
            && new != text
        {
            write(repo, lock, &new)?;
            files.push(lock.to_owned());
        }
    }
    if bump.changelog {
        let text = read_at_head(repo, CHANGELOG)?;
        write(
            repo,
            CHANGELOG,
            &with_entry(&text, &plan.tag, &plan.notes, &today()),
        )?;
        files.push(CHANGELOG.to_owned());
    }
    Ok(files)
}

fn read_at_head(repo: &Path, file: &str) -> Result<String, String> {
    git::file_at(repo, Some("HEAD"), file).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Writes `file` of the working tree, on whichever machine the repository is.
fn write(repo: &Path, file: &str, text: &str) -> Result<(), String> {
    crate::remote::write(&crate::remote::join(repo, file), text.as_bytes())
        .map_err(|e| format!("Couldn't write {file}: {e}"))
}

/// `CHANGELOG.md` with the release's notes added on top, under its first heading when it starts
/// with one (`# Changelog`).
pub(crate) fn with_entry(text: &str, tag: &str, notes: &str, date: &str) -> String {
    let entry = format!("## {tag} - {date}\n\n{}\n\n", notes.trim());
    let mut lines = text.split_inclusive('\n').peekable();
    let mut head = String::new();
    if lines.peek().is_some_and(|l| l.starts_with("# ")) {
        head.push_str(lines.next().unwrap_or_default());
        // The heading's own paragraph stays above.
        while let Some(l) = lines.peek() {
            if l.starts_with("## ") {
                break;
            }
            head.push_str(lines.next().unwrap_or_default());
        }
        if !head.ends_with("\n\n") {
            head.push('\n');
        }
    }
    let rest: String = lines.collect();
    format!("{head}{entry}{rest}")
}

/// Today's date, `YYYY-MM-DD` (UTC).
pub(crate) fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The calendar date of `days` since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Whether `tag` can be a new tag: something, not taken, nothing git would refuse in a name.
pub(crate) fn tag_problem(tag: &str, taken: &[String]) -> Option<String> {
    let tag = tag.trim();
    if tag.is_empty() {
        return Some("Name the version".to_owned());
    }
    if taken.iter().any(|t| t == tag) {
        return Some(format!("{tag} is a tag already"));
    }
    let bad = tag.starts_with(['-', '.', '/'])
        || tag.ends_with(['.', '/'])
        || tag.ends_with(".lock")
        || tag.contains("..")
        || tag.contains("@{")
        || tag
            .chars()
            .any(|c| c.is_control() || " ~^:?*[\\".contains(c));
    bad.then(|| format!("{tag} can't be a tag's name"))
}
