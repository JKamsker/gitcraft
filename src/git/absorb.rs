//! What absorbing staged fixes reads: the commits of the checked-out branch that are its own
//! (on no other branch nor remote but its upstream), who last touched some lines of a file,
//! and whether `git-absorb` is installed where git runs.

use std::collections::HashMap;
use std::path::Path;

use super::{LITERAL_PATHS, NO_SIGNATURE, UTF8, run};

/// A commit of the branch's own, which staged fixes can go into.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StackCommit {
    pub hash: String,
    pub summary: String,
    /// How far down the branch: 0 for HEAD.
    pub depth: usize,
    /// Its upstream has it: rewriting it needs a force push.
    pub pushed: bool,
}

/// Most commits looked at.
const STACK_MAX: usize = 100;

/// The commits of `branch` (checked out) from HEAD down its first parents that no other
/// branch and no remote but its upstream has, stopping before a merge or the first commit,
/// which can't be rewritten here.
pub fn absorb_stack(repo: &Path, branch: &str) -> Result<Vec<StackCommit>, String> {
    let upstream = run(
        Some(repo),
        &[
            "rev-parse",
            "--symbolic-full-name",
            "--verify",
            "--quiet",
            "@{upstream}",
        ],
    )
    .map(|u| u.trim().to_owned())
    .unwrap_or_default();
    // Patterns of `--branches` / `--remotes` go without `refs/heads/` / `refs/remotes/`, and
    // are globs: the names' own glob characters are escaped.
    let glob = |name: &str| {
        name.chars()
            .flat_map(|c| match c {
                '*' | '?' | '[' | '\\' => vec!['\\', c],
                _ => vec![c],
            })
            .collect::<String>()
    };
    let own = format!("--exclude={}", glob(branch));
    let theirs = format!(
        "--exclude={}",
        glob(upstream.strip_prefix("refs/remotes/").unwrap_or(&upstream))
    );
    let max = format!("--max-count={STACK_MAX}");
    // `origin/HEAD` is the remote's default branch under another name.
    let heads = "--exclude=*/HEAD";
    let mut args = vec![
        "log",
        NO_SIGNATURE,
        UTF8,
        "--first-parent",
        &max,
        "--format=%H%x00%P%x00%s",
        "HEAD",
        "--not",
        &own,
        "--branches",
    ];
    if upstream.starts_with("refs/remotes/") {
        args.extend([theirs.as_str(), heads]);
    }
    args.extend(["--remotes", "--"]);
    let out = run(Some(repo), &args)?;
    // Those the upstream doesn't have yet.
    let unpushed: std::collections::HashSet<String> = if upstream.is_empty() {
        Default::default()
    } else {
        run(
            Some(repo),
            &[
                "rev-list",
                "--first-parent",
                &max,
                "HEAD",
                "--not",
                "@{upstream}",
                "--",
            ],
        )?
        .lines()
        .map(str::to_owned)
        .collect()
    };
    let mut stack = Vec::new();
    for (depth, record) in out.lines().enumerate() {
        let mut fields = record.split('\0');
        let (Some(hash), Some(parents), summary) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if parents.split_whitespace().count() != 1 {
            break;
        }
        stack.push(StackCommit {
            hash: hash.to_owned(),
            summary: summary.unwrap_or_default().to_owned(),
            depth,
            pushed: !upstream.is_empty() && !unpushed.contains(hash),
        });
    }
    Ok(stack)
}

/// Who last touched each of `lines` (1-based) of `path` at HEAD.
pub fn blame_lines(repo: &Path, path: &str, lines: &[u32]) -> Result<HashMap<u32, String>, String> {
    let mut sorted: Vec<u32> = lines.iter().copied().filter(|&l| l > 0).collect();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.is_empty() {
        return Ok(HashMap::new());
    }
    // Runs of lines as few `-L` ranges.
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for l in sorted {
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == l => *end = l,
            _ => ranges.push((l, l)),
        }
    }
    let specs: Vec<String> = ranges.iter().map(|(a, b)| format!("-L{a},{b}")).collect();
    let mut args = vec![LITERAL_PATHS, "blame", "--porcelain"];
    args.extend(specs.iter().map(String::as_str));
    args.extend(["HEAD", "--", path]);
    let out = run(Some(repo), &args)?;
    Ok(parse_porcelain(&out))
}

/// `blame --porcelain`: each line of the file comes after a header `<hash> <old> <final> …`.
fn parse_porcelain(out: &str) -> HashMap<u32, String> {
    let mut found = HashMap::new();
    for line in out.lines() {
        let mut words = line.split(' ');
        let (Some(hash), Some(_), Some(at)) = (words.next(), words.next(), words.next()) else {
            continue;
        };
        if hash.len() >= 40
            && hash.bytes().all(|b| b.is_ascii_hexdigit())
            && let Ok(at) = at.parse::<u32>()
        {
            found.insert(at, hash.to_owned());
        }
    }
    found
}

/// Whether `git absorb` runs where git runs for `repo`.
pub fn has_git_absorb(repo: &Path) -> bool {
    run(Some(repo), &["absorb", "--version"]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_lines_and_their_commits() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let out = format!(
            "{a} 1 3 2\nauthor x\nfilename f\n\tone\n{a} 2 4\n\ttwo\n{b} 9 7 1\nauthor y\nboundary\nfilename f\n\tseven\n"
        );
        let found = parse_porcelain(&out);
        assert_eq!(found.len(), 3);
        assert_eq!(found.get(&3), Some(&a));
        assert_eq!(found.get(&4), Some(&a));
        assert_eq!(found.get(&7), Some(&b));
    }
}
