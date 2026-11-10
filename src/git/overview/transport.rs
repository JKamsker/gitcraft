//! Transport.

use super::*;

/// The questions asked of every repository: (key, git's arguments).
pub(super) fn probes() -> Vec<(&'static str, Vec<String>)> {
    let words = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
    vec![
        ("branch", words(&["symbolic-ref", "-q", "HEAD"])),
        (
            "head",
            words(&["rev-parse", "-q", "--verify", "--short", "HEAD"]),
        ),
        (
            "refs",
            vec![
                "for-each-ref".to_owned(),
                "--sort=-committerdate".to_owned(),
                format!(
                    "--format=%(refname){FS}%(upstream:short){FS}%(upstream:track){FS}%(upstream)"
                ),
                "refs/heads".to_owned(),
            ],
        ),
        ("remotes", words(&["remote"])),
        (
            "status",
            words(&[
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
            ]),
        ),
        ("stashes", words(&["stash", "list", UTF8, "--format=%gs"])),
        ("worktrees", words(&["worktree", "list", "--porcelain"])),
        ("top", words(&["rev-parse", "--show-toplevel"])),
    ]
}

/// The `status` asked of each other worktree.
pub(super) const WORKTREE_STATUS: [&str; 5] = [
    "--no-optional-locks",
    "status",
    "--porcelain=v1",
    "-z",
    "--untracked-files=all",
];

/// The sections, asked here one command at a time.
pub(super) fn sections_here(repo: &Path) -> Result<Vec<Section>, String> {
    let mut sections = Vec::new();
    let ask = |key: String, dir: &Path, args: &[&str]| -> Result<Section, String> {
        let (code, out) = run_coded(dir, args)?;
        Ok(Section { key, code, out })
    };
    for (key, args) in probes() {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        sections.push(ask(key.to_owned(), repo, &args)?);
    }
    let fetched = last_fetch_here(repo);
    sections.push(Section {
        key: "fetched".to_owned(),
        code: if fetched.is_some() { 0 } else { 1 },
        out: fetched
            .map(|t| t.to_string().into_bytes())
            .unwrap_or_default(),
    });
    let has_remote = !text(find(&sections, "remotes")).trim().is_empty();
    if has_remote {
        for branch in branches_to_count(&text(find(&sections, "refs"))) {
            let full = format!("refs/heads/{branch}");
            let args = ["rev-list", "--count", &full, "--not", "--remotes"];
            sections.push(ask(format!("count:{branch}"), repo, &args)?);
        }
    }
    let top = text(find(&sections, "top"))
        .trim_end_matches(['\n', '\r'])
        .to_owned();
    for path in other_worktrees(&text(find(&sections, "worktrees")), &top) {
        let key = format!("wt:{path}");
        sections.push(ask(key, Path::new(&path), &WORKTREE_STATUS)?);
    }
    Ok(sections)
}

/// When `repo` here last fetched: the time of its `FETCH_HEAD`, which every fetch writes.
pub(super) fn last_fetch_here(repo: &Path) -> Option<i64> {
    let (code, out) = run_coded(repo, &["rev-parse", "--git-common-dir"]).ok()?;
    if code != 0 {
        return None;
    }
    let dir = String::from_utf8_lossy(&out);
    let dir = repo.join(dir.trim_end_matches(['\n', '\r']));
    let modified = std::fs::metadata(dir.join("FETCH_HEAD"))
        .ok()?
        .modified()
        .ok()?;
    let secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    i64::try_from(secs).ok()
}

/// The sections, asked of `target`'s machine in one script.
pub(super) fn sections_there(
    target: &crate::remote::Target,
    limit: Duration,
) -> Result<Vec<Section>, String> {
    let req = crate::remote::Request {
        cwd: Some(target.path.clone()),
        script: script(),
        env: GIT_ENV
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect(),
        unset: GIT_ENV_REMOVED.iter().map(|n| (*n).to_owned()).collect(),
        stdin: None,
        long: false,
        timeout: Some(limit),
        apart: false,
        test: false,
    };
    let out = crate::remote::exec(&target.host, &req)?;
    if out.code == 1 && crate::remote::is_missing_folder(&out.stderr) {
        return Err(super::super::command::missing_folder(target));
    }
    if out.code != 0 {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if why.is_empty() {
            crate::i18n::message(
                "backend-couldn-t-look-at-on-6360b7",
                &[
                    ("path", (target.path).to_string().into()),
                    ("target", (target.host.label()).to_string().into()),
                ],
            )
        } else {
            why
        });
    }
    Ok(split_sections(&out.stdout))
}

/// The script that asks every section of the repository it runs in, each printed as
/// `\x1e<code> <bytes> <key>` on a line and then its output (see [`split_sections`]).
pub(super) fn script() -> String {
    use crate::remote::sh_quote;
    let quoted = |args: &[String]| {
        args.iter()
            .map(|a| sh_quote(a))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut s = String::from(concat!(
        "GIT_NO_LAZY_FETCH=1; export GIT_NO_LAZY_FETCH\n",
        "git() { command git --no-lazy-fetch \"$@\"; }\n",
        "d=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-look.XXXXXX\") || exit 1\n",
        "trap 'rm -rf \"$d\"' EXIT HUP INT TERM\n",
        "s() { k=$1; shift\n",
        "  if \"$@\" >\"$d/o\" 2>\"$d/e\" </dev/null; then c=0; else c=$?; mv \"$d/e\" \"$d/o\"; fi\n",
        "  printf '\\036%s %s %s\\n' \"$c\" \"$(wc -c <\"$d/o\" | tr -d ' ')\" \"$k\"; cat \"$d/o\"; }\n",
    ));
    for (key, args) in probes() {
        s.push_str(&format!("s {key} git {}\n", quoted(&args)));
    }
    s.push_str(concat!(
        "f=\"$(git rev-parse --git-common-dir 2>/dev/null)/FETCH_HEAD\"\n",
        "t() { [ -f \"$f\" ] && { stat -c %Y \"$f\" 2>/dev/null || stat -f %m \"$f\"; }; }\n",
        "s fetched t\n",
    ));
    s.push_str(&format!(
        concat!(
            "if [ -n \"$(git remote 2>/dev/null)\" ]; then\n",
            "  n=0; tab=$(printf '\\t')\n",
            "  git for-each-ref --sort=-committerdate '--format=%(refname)%09%(upstream)%09%(upstream:track)' refs/heads 2>/dev/null |\n",
            "  while IFS=\"$tab\" read -r r u t; do\n",
            "    case \"$u\" in refs/remotes/*) [ \"$t\" = '[gone]' ] || continue;; esac\n",
            "    [ \"$n\" -lt {max_b} ] || break; n=$((n + 1))\n",
            "    s \"count:${{r#refs/heads/}}\" git rev-list --count \"$r\" --not --remotes\n",
            "  done\n",
            "fi\n",
            "top=$(git rev-parse --show-toplevel 2>/dev/null)\n",
            "n=0\n",
            "git worktree list --porcelain 2>/dev/null |\n",
            "awk '/^worktree /{{p=substr($0,10)}} /^bare$/{{p=\"\"}} /^$/{{if(p!=\"\")print p; p=\"\"}} END{{if(p!=\"\")print p}}' |\n",
            "while IFS= read -r p; do\n",
            "  [ \"$p\" = \"$top\" ] && continue\n",
            "  [ \"$n\" -lt {max_w} ] || break; n=$((n + 1))\n",
            "  s \"wt:$p\" git -C \"$p\" {status}\n",
            "done\n",
            "rm -rf \"$d\"\n",
        ),
        max_b = MAX_COUNTED_BRANCHES,
        max_w = MAX_COUNTED_WORKTREES,
        status = WORKTREE_STATUS.join(" "),
    ));
    s
}

/// The sections of [`script`]'s output, in order; what doesn't read as one ends the list.
pub(super) fn split_sections(mut bytes: &[u8]) -> Vec<Section> {
    let mut sections = Vec::new();
    while let Some(start) = bytes.iter().position(|&b| b == 0x1e) {
        bytes = &bytes[start + 1..];
        let Some(nl) = bytes.iter().position(|&b| b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&bytes[..nl]).into_owned();
        bytes = &bytes[nl + 1..];
        let mut words = header.splitn(3, ' ');
        let (Some(code), Some(len), Some(key)) = (words.next(), words.next(), words.next()) else {
            break;
        };
        let (Ok(code), Ok(len)) = (code.parse::<i32>(), len.parse::<usize>()) else {
            break;
        };
        if len > bytes.len() {
            break;
        }
        sections.push(Section {
            key: key.to_owned(),
            code,
            out: bytes[..len].to_vec(),
        });
        bytes = &bytes[len..];
    }
    sections
}
