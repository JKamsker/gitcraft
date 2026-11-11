//! What git is asked for a prompt, through the usual git layer (so on the repository's own
//! machine): the staged diff, what a branch adds to its base, and the recent summaries whose
//! style a message follows. Blocking: from a background thread.

use std::path::Path;

use super::prompt::{self, Prompt, ReviewInput};
use super::settings::AiSettings;
use crate::git;

/// Options of every diff read here: names as they are (not `\303\274`), no colours, no
/// external diff program, the usual `a/` / `b/` prefixes whatever the user's config says, and
/// renames found.
const DIFF: [&str; 7] = [
    "--no-color",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "-M",
    "--submodule=short",
    "--no-relative",
];

/// How many recent summaries are read for the repository's conventions.
const SUBJECTS: &str = "30";
/// How many of a branch's commits a review lists.
const COMMITS: &str = "200";

/// The staged changes, as `git diff --cached` prints them; with `since`, from that commit (or
/// tree) to what is staged.
pub fn staged_diff(repo: &Path, since: Option<&str>) -> Result<String, String> {
    let mut args = vec!["-c", "core.quotePath=false", "diff", "--cached"];
    args.extend(DIFF);
    args.extend(since);
    args.push("--");
    git::run(Some(repo), &args)
}

/// What the last commit is amended from: its parent, or the empty tree for a first commit.
fn amend_base(repo: &Path) -> Result<String, String> {
    let parent = git::run(Some(repo), &["rev-parse", "--verify", "-q", "HEAD^"]);
    match parent {
        Ok(parent) if !parent.trim().is_empty() => Ok(parent.trim().to_owned()),
        // The hash of the empty tree in this repository's object format.
        _ => git::run(Some(repo), &["hash-object", "-t", "tree", "--stdin"])
            .map(|tree| tree.trim().to_owned()),
    }
}

/// The summaries of the latest commits (merges left out), newest first; none in a repository
/// with no commit yet.
/// `skip`: how many of the latest to leave out (the commit being amended).
pub fn recent_subjects(repo: &Path, skip: usize) -> Vec<String> {
    let skip = format!("--skip={skip}");
    git::run(
        Some(repo),
        &[
            "log",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--no-merges",
            "-n",
            SUBJECTS,
            &skip,
            "--format=%s",
        ],
    )
    .map(|out| out.lines().map(str::to_owned).collect())
    .unwrap_or_default()
}

/// A revision, refused when it would read as an option.
fn revision(rev: &str) -> Result<&str, String> {
    if rev.is_empty() || rev.starts_with('-') {
        Err(format!("{rev:?} is not a branch"))
    } else {
        Ok(rev)
    }
}

/// What `branch` adds to `base` since the two diverged: its commits (a line each) and its diff.
pub fn branch_changes(repo: &Path, branch: &str, base: &str) -> Result<(String, String), String> {
    let (branch, base) = (revision(branch)?, revision(base)?);
    let range = format!("{base}..{branch}");
    let commits = git::run(
        Some(repo),
        &[
            "log",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--no-merges",
            "-n",
            COMMITS,
            "--format=%h %s",
            &range,
            "--",
        ],
    )?;
    let three_dots = format!("{base}...{branch}");
    let mut args = vec!["-c", "core.quotePath=false", "diff"];
    args.extend(DIFF);
    args.extend([three_dots.as_str(), "--"]);
    let diff = git::run(Some(repo), &args)?;
    Ok((commits, diff))
}

/// What the prompt says when the message is for an amended commit.
pub const AMENDING: &str = "This message replaces the last commit's: the changes below are \
                            that commit's, with what is staged now.\n";

/// The prompt for a message of what is staged in `repo`; when `amend`, of the last commit
/// with what is staged.
pub fn commit_prompt(repo: &Path, settings: &AiSettings, amend: bool) -> Result<Prompt, String> {
    let base = if amend { Some(amend_base(repo)?) } else { None };
    let staged = staged_diff(repo, base.as_deref())?;
    if staged.trim().is_empty() {
        return Err(if amend {
            "The amended commit would change nothing: there is nothing to describe.".into()
        } else {
            "Nothing is staged: stage the changes the message should describe.".into()
        });
    }
    let subjects = recent_subjects(repo, usize::from(amend));
    let mut prompt = prompt::commit_prompt(settings, &staged, &subjects);
    if amend {
        prompt.text = format!("{AMENDING}\n{}", prompt.text);
    }
    Ok(prompt)
}

/// The prompt for a review of what `branch` adds to `base`.
pub fn review_prompt(
    repo: &Path,
    settings: &AiSettings,
    branch: &str,
    base: &str,
) -> Result<Prompt, String> {
    let (commits, diff) = branch_changes(repo, branch, base)?;
    if diff.trim().is_empty() {
        return Err(format!(
            "{branch} changes nothing that {base} doesn't have."
        ));
    }
    Ok(prompt::review_prompt(
        settings,
        &ReviewInput {
            branch,
            base,
            commits: &commits,
            diff: &diff,
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A repository with one commit, in a folder whose name has spaces and non-ASCII letters.
    fn repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gitcraft ai ü {name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            // What a user's config may set, which the reads here must not follow.
            &["config", "diff.noprefix", "true"],
            &["config", "color.ui", "always"],
            &["commit", "-q", "--allow-empty", "-m", "feat: start"],
        ] {
            assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
        }
        dir
    }

    fn git_ok(dir: &Path, args: &[&str]) {
        assert!(git::run(Some(dir), args).is_ok(), "{args:?}");
    }

    #[test]
    fn nothing_staged_is_said_so() {
        let dir = repo("nothing");
        let _ = std::fs::write(dir.join("unstaged.txt"), "x\n");
        let found = commit_prompt(&dir, &AiSettings::default(), false);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            found,
            Err("Nothing is staged: stage the changes the message should describe.".to_owned())
        );
    }

    #[test]
    fn the_staged_diff_and_conventions_make_the_prompt() -> Result<(), String> {
        let dir = repo("staged");
        git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "fix(ui): a"]);
        git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "docs: b"]);
        let name = "dossier é/fichier à espaces.txt";
        let _ = std::fs::create_dir_all(dir.join("dossier é"));
        let _ = std::fs::write(dir.join(name), "héllo wörld 😀\n");
        let _ = std::fs::write(dir.join("not staged.txt"), "no\n");
        git_ok(&dir, &["add", "--", name]);
        let found = commit_prompt(&dir, &AiSettings::default(), false);
        let _ = std::fs::remove_dir_all(&dir);
        let p = found?;
        assert!(p.text.contains(&format!("+++ b/{name}")), "{}", p.text);
        assert!(p.text.contains("+héllo wörld 😀"));
        assert!(!p.text.contains("not staged"));
        assert!(!p.text.contains('\u{1b}'), "no colours");
        assert!(p.text.contains("Conventional Commits"));
        assert_eq!((p.files, p.left_out), (1, 0));
        Ok(())
    }

    #[test]
    fn a_huge_staged_change_is_cut_to_the_setting() -> Result<(), String> {
        let dir = repo("huge");
        let big: String = (0..200_000).map(|i| format!("line {i}\n")).collect();
        let _ = std::fs::write(dir.join("big.txt"), big);
        let _ = std::fs::write(dir.join("small.txt"), "small\n");
        git_ok(&dir, &["add", "."]);
        let settings = AiSettings {
            max_diff_kb: 16,
            ..AiSettings::default()
        };
        let found = commit_prompt(&dir, &settings, false);
        let _ = std::fs::remove_dir_all(&dir);
        let p = found?;
        assert!(p.text.len() < 20 * 1024, "{}", p.text.len());
        assert!(p.text.contains("+small"));
        assert!(p.text.contains("- big.txt (+200000 −0)"));
        Ok(())
    }

    #[test]
    fn a_review_has_what_the_branch_adds_since_it_diverged() -> Result<(), String> {
        let dir = repo("review");
        git_ok(&dir, &["checkout", "-q", "-b", "feature/ünï"]);
        let _ = std::fs::write(dir.join("feature.txt"), "new\n");
        git_ok(&dir, &["add", "."]);
        git_ok(&dir, &["commit", "-q", "-m", "feat: the feature"]);
        git_ok(&dir, &["checkout", "-q", "main"]);
        // On main since: not part of what the branch adds.
        let _ = std::fs::write(dir.join("main-only.txt"), "m\n");
        git_ok(&dir, &["add", "."]);
        git_ok(&dir, &["commit", "-q", "-m", "chore: main moved"]);
        let found = review_prompt(&dir, &AiSettings::default(), "feature/ünï", "main");
        let nothing = review_prompt(&dir, &AiSettings::default(), "main", "feature/ünï");
        let refused = branch_changes(&dir, "--output=/tmp/x", "main");
        let _ = std::fs::remove_dir_all(&dir);
        let p = found?;
        assert!(p.text.contains("branch feature/ünï adds to main"));
        assert!(p.text.contains("feat: the feature"));
        assert!(p.text.contains("+++ b/feature.txt"));
        assert!(!p.text.contains("main-only"));
        // The other way round, what main adds is its own change.
        assert!(nothing.is_ok_and(|p| p.text.contains("main-only")));
        assert!(refused.is_err());
        Ok(())
    }

    #[test]
    fn a_branch_with_nothing_new_is_said_so() {
        let dir = repo("review-empty");
        git_ok(&dir, &["branch", "same"]);
        let found = review_prompt(&dir, &AiSettings::default(), "same", "main");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            found,
            Err("same changes nothing that main doesn't have.".to_owned())
        );
    }

    #[test]
    fn an_amend_describes_the_last_commit_with_what_is_staged() -> Result<(), String> {
        let dir = repo("amend");
        let _ = std::fs::write(dir.join("first.txt"), "first\n");
        git_ok(&dir, &["add", "."]);
        git_ok(&dir, &["commit", "-q", "-m", "fix: the commit amended"]);
        // A reword: nothing staged.
        let reword = commit_prompt(&dir, &AiSettings::default(), true);
        let _ = std::fs::write(dir.join("more.txt"), "more\n");
        git_ok(&dir, &["add", "."]);
        let more = commit_prompt(&dir, &AiSettings::default(), true);
        let plain = commit_prompt(&dir, &AiSettings::default(), false);
        let _ = std::fs::remove_dir_all(&dir);
        let reword = reword?;
        assert!(reword.text.starts_with(AMENDING));
        assert!(reword.text.contains("+++ b/first.txt"));
        // The commit amended is no example of the repository's style.
        assert!(!reword.text.contains("- fix: the commit amended"));
        let more = more?;
        assert!(more.text.contains("+++ b/first.txt") && more.text.contains("+++ b/more.txt"));
        let plain = plain?;
        assert!(!plain.text.contains("first.txt") && !plain.text.contains(AMENDING));
        Ok(())
    }

    #[test]
    fn amending_the_first_commit_describes_all_of_it() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-ai-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        git_ok(&dir, &["init", "-q"]);
        git_ok(&dir, &["config", "user.name", "t"]);
        git_ok(&dir, &["config", "user.email", "t@t"]);
        let _ = std::fs::write(dir.join("root.txt"), "root\n");
        git_ok(&dir, &["add", "."]);
        git_ok(&dir, &["commit", "-q", "-m", "root"]);
        let found = commit_prompt(&dir, &AiSettings::default(), true);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(found?.text.contains("+++ b/root.txt"));
        Ok(())
    }

    #[test]
    fn a_repository_without_commits_has_no_summaries() {
        let dir = std::env::temp_dir().join(format!("gitcraft-ai-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        git_ok(&dir, &["init", "-q"]);
        let subjects = recent_subjects(&dir, 0);
        let _ = std::fs::write(dir.join("first.txt"), "1\n");
        git_ok(&dir, &["add", "."]);
        let p = commit_prompt(&dir, &AiSettings::default(), false);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(subjects.is_empty());
        assert!(p.is_ok_and(|p| p.text.contains("+++ b/first.txt")));
    }
}
