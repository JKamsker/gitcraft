//! Keep both versions of a file that can't be merged (an image, an archive…): each side's
//! version written beside it as a file of its own, named after the branch it comes from
//! (`logo.main.png`, `logo.feature-x.png`), never over a file that is already there.

use std::path::Path;

use crate::git::{self, InProgress, Stage};

/// One side's version and where it is to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// Where it comes from, as the user thinks of it: "Ours, main", "Your commit, feature/x".
    pub origin: String,
    pub oid: String,
    /// Executable (`100755`).
    pub executable: bool,
    pub size: u64,
    /// The path it is to be written at, relative to the repository.
    pub target: String,
    /// Names passed over because a file (or a tracked one) is there already.
    pub taken: Vec<String>,
    /// A `.gitignore` names the target: Stage both adds it anyway.
    pub ignored: bool,
}

/// What Keep both would write for `path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub path: String,
    pub ours: Version,
    pub theirs: Version,
}

/// The characters that can't be in a file name somewhere (Windows is the strictest), and
/// whitespace, are written `-` in a branch's name; so are leading and trailing dots.
pub(super) fn tag(branch: &str) -> String {
    let mapped: String = branch
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_whitespace() || c.is_control() => '-',
            c => c,
        })
        .collect();
    let trimmed = mapped.trim_matches(['.', '-']);
    let mut out = String::new();
    // `a//b` → `a-b`, not `a--b`.
    for c in trimmed.chars() {
        if !(c == '-' && out.ends_with('-')) {
            out.push(c);
        }
    }
    out
}

/// `path` with `tag` before its extension: `img/logo.png` → `img/logo.main.png`; a file
/// without one (`Makefile`, `.env`) gets it at the end.
pub(super) fn tagged(path: &str, tag: &str) -> String {
    let (dir, name) = match path.rfind('/') {
        Some(i) => path.split_at(i + 1),
        None => ("", path),
    };
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{dir}{}.{tag}{}", &name[..i], &name[i..]),
        _ => format!("{dir}{name}.{tag}"),
    }
}

/// The tags for the two sides: their branches' names, or what git calls the sides. The two
/// are never the same.
pub(super) fn tags(branches: (Option<&str>, Option<&str>), rebase: bool) -> (String, String) {
    let fallback = if rebase {
        ("upstream", "your-commit")
    } else {
        ("ours", "theirs")
    };
    let pick = |branch: Option<&str>, other: &str| {
        let t = branch.map(tag).unwrap_or_default();
        if t.is_empty() { other.to_owned() } else { t }
    };
    let (ours, theirs) = (pick(branches.0, fallback.0), pick(branches.1, fallback.1));
    if ours.to_lowercase() == theirs.to_lowercase() {
        (
            format!("{ours}-{}", fallback.0),
            format!("{theirs}-{}", fallback.1),
        )
    } else {
        (ours, theirs)
    }
}

/// The first of `path` tagged `tag`, `tag-2`, `tag-3`… for which `taken` says no (and that
/// isn't `also_taken`), with the ones passed over.
pub(super) fn free_name(
    path: &str,
    tag: &str,
    also_taken: Option<&str>,
    mut taken: impl FnMut(&str) -> Result<bool, String>,
) -> Result<(String, Vec<String>), String> {
    let mut passed = Vec::new();
    for n in 1..=100 {
        let name = if n == 1 {
            tagged(path, tag)
        } else {
            tagged(path, &format!("{tag}-{n}"))
        };
        let clash = also_taken.is_some_and(|other| other.to_lowercase() == name.to_lowercase());
        if !clash && !taken(&name)? {
            return Ok((name, passed));
        }
        passed.push(name);
    }
    Err(crate::i18n::message(
        "repo-core-0569",
        &[
            ("path", format!("{}", path).into()),
            ("tag", format!("{}", tagged(path, tag)).into()),
        ],
    ))
}

/// Whether there is something at `rel` in the repository at `repo`: a file, a folder or a link
/// (even a broken one) on disk, or a file in the index.
fn taken(repo: &Path, rel: &str) -> Result<bool, String> {
    let tracked = git::run(
        Some(repo),
        &["--literal-pathspecs", "ls-files", "-z", "--", rel],
    )?;
    if !tracked.is_empty() {
        return Ok(true);
    }
    exists(&crate::remote::join(repo, rel))
}

/// Whether anything (even a broken link) is at `file`.
fn exists(file: &Path) -> Result<bool, String> {
    let Some(target) = crate::remote::parse(file) else {
        return match std::fs::symlink_metadata(file) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(crate::i18n::message(
                "repo-core-0571",
                &[
                    ("file", format!("{}", file.display()).into()),
                    ("e", format!("{}", e).into()),
                ],
            )),
        };
    };
    let f = crate::remote::sh_path(&target.path);
    let out = crate::remote::exec(
        &target.host,
        &crate::remote::Request {
            script: format!("if [ -e {f} ] || [ -L {f} ]; then exit 0; else exit 1; fi"),
            ..crate::remote::Request::default()
        },
    )?;
    match out.code {
        0 => Ok(true),
        1 => Ok(false),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()),
    }
}

/// What the side `stage` of `path` is, as a file Keep both can write: refused for a link or a
/// submodule, or a side that deleted the file.
fn writable<'a>(stage: Option<&'a Stage>, path: &str, who: &str) -> Result<&'a Stage, String> {
    let stage = stage.ok_or_else(|| {
        crate::i18n::message(
            "repo-core-0573",
            &[
                ("who", format!("{}", who).into()),
                ("path", format!("{}", path).into()),
            ],
        )
    })?;
    match stage.mode.as_str() {
        "120000" => Err(crate::i18n::message(
            "repo-core-0574",
            &[
                ("who", format!("{}", who).into()),
                ("path", format!("{}", path).into()),
            ],
        )),
        "160000" => Err(crate::i18n::message(
            "repo-core-0575",
            &[
                ("who", format!("{}", who).into()),
                ("path", format!("{}", path).into()),
            ],
        )),
        _ => Ok(stage),
    }
}

/// Works out what Keep both would write for `path` in `repo`, left in conflict by `state`
/// (blocking: in the background).
pub fn plan(repo: &Path, path: &str, state: Option<InProgress>) -> Result<Plan, String> {
    let stages = git::conflict_stages(repo, path)?;
    if stages.ours.is_none() && stages.theirs.is_none() {
        return Err(crate::i18n::message(
            "repo-core-0576",
            &[("path", format!("{}", path).into())],
        ));
    }
    let rebase = state == Some(InProgress::Rebase);
    let (ours_who, theirs_who) = if rebase {
        (
            crate::i18n::text("repo-core-0577"),
            crate::i18n::text("repo-core-0578"),
        )
    } else {
        (
            crate::i18n::text("repo-core-0579"),
            crate::i18n::text("repo-core-0580"),
        )
    };
    let ours = writable(stages.ours.as_ref(), path, ours_who)?;
    let theirs = writable(stages.theirs.as_ref(), path, theirs_who)?;
    let (ours_branch, theirs_branch) = git::side_branches(repo, state);
    let (ours_tag, theirs_tag) = tags((ours_branch.as_deref(), theirs_branch.as_deref()), rebase);
    let is_taken = |rel: &str| taken(repo, rel);
    let (ours_target, ours_taken) = free_name(path, &ours_tag, None, is_taken)?;
    let (theirs_target, theirs_taken) = free_name(path, &theirs_tag, Some(&ours_target), is_taken)?;
    let version =
        |stage: &Stage, who: &str, branch: Option<String>, target: String, taken: Vec<String>| {
            Ok::<_, String>(Version {
                origin: match branch {
                    Some(b) => format!("{who} ({b})"),
                    None => who.to_owned(),
                },
                oid: stage.oid.clone(),
                executable: stage.mode == "100755",
                size: git::checked_out_size(repo, &stage.oid).map_err(|e| {
                    crate::i18n::message(
                        "repo-core-0582",
                        &[
                            ("who", format!("{}", who).into()),
                            ("e", format!("{}", e).into()),
                        ],
                    )
                })?,
                ignored: git::is_ignored(repo, &target)?,
                target,
                taken,
            })
        };
    Ok(Plan {
        path: path.to_owned(),
        ours: version(ours, ours_who, ours_branch, ours_target, ours_taken)?,
        theirs: version(
            theirs,
            theirs_who,
            theirs_branch,
            theirs_target,
            theirs_taken,
        )?,
    })
}

/// Writes both versions of `plan` in `repo` (blocking: in the background). Neither is written
/// over anything: when a file has appeared at one of the names since the preview, nothing is
/// left written.
pub fn write(repo: &Path, plan: &Plan) -> Result<(), String> {
    let mut written: Option<&str> = None;
    for version in [&plan.ours, &plan.theirs] {
        let done = git::checked_out_blob(repo, &version.oid, &plan.path)
            .map_err(|e| {
                crate::i18n::message(
                    "repo-core-0583",
                    &[
                        ("origin", format!("{}", version.origin).into()),
                        ("e", format!("{}", e).into()),
                    ],
                )
            })
            .and_then(|bytes| {
                create_new(
                    &crate::remote::join(repo, &version.target),
                    &bytes,
                    version.executable,
                )
                .map_err(|e| {
                    crate::i18n::message(
                        "repo-core-0584",
                        &[
                            ("target", format!("{}", version.target).into()),
                            ("e", format!("{}", e).into()),
                        ],
                    )
                })
            });
        if let Err(e) = done {
            if let Some(first) = written {
                let _ = remove(&crate::remote::join(repo, first));
            }
            return Err(e);
        }
        written = Some(&version.target);
    }
    Ok(())
}

/// Why a version was not written when something appeared at its name since the preview.
const APPEARED: &str = "repo-core-0585";

/// What the create script ends with when something is at the path already.
const EXISTS: i32 = 17;

/// Writes `bytes` to a new file at `file`, failing rather than replacing (or writing through)
/// anything there.
fn create_new(file: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    let Some(target) = crate::remote::parse(file) else {
        return create_new_here(file, bytes, executable);
    };
    let f = crate::remote::sh_path(&target.path);
    // `set -C`: the shell opens the file only if nothing is there (O_EXCL).
    let script = format!(
        "if [ -e {f} ] || [ -L {f} ]; then exit {EXISTS}; fi\nset -C\ncat > {f} || {{ rm -f -- {f}; exit 1; }}\n{}",
        if executable {
            format!("chmod +x {f} || {{ rm -f -- {f}; exit 1; }}\n")
        } else {
            String::new()
        }
    );
    let out = crate::remote::exec(
        &target.host,
        &crate::remote::Request {
            script,
            stdin: Some(bytes.to_vec()),
            ..crate::remote::Request::default()
        },
    )?;
    match out.code {
        0 => Ok(()),
        EXISTS => Err(crate::i18n::text(APPEARED).to_owned()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()),
    }
}

fn create_new_here(file: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    let mut opened = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => crate::i18n::text(APPEARED).to_owned(),
            _ => e.to_string(),
        })?;
    let filled = fill(&mut opened, bytes, executable);
    if filled.is_err() {
        // Not left half written.
        drop(opened);
        let _ = std::fs::remove_file(file);
    }
    filled
}

/// Writes `bytes` to the file just created, made executable when asked.
fn fill(opened: &mut std::fs::File, bytes: &[u8], executable: bool) -> Result<(), String> {
    use std::io::Write;
    opened.write_all(bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        let mode = opened
            .metadata()
            .map_err(|e| e.to_string())?
            .permissions()
            .mode();
        opened
            .set_permissions(std::fs::Permissions::from_mode(mode | 0o111))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}

/// Removes the file Keep both wrote at `file` (when the other could not be written).
fn remove(file: &Path) -> Result<(), String> {
    let Some(target) = crate::remote::parse(file) else {
        return std::fs::remove_file(file).map_err(|e| e.to_string());
    };
    let out = crate::remote::exec(
        &target.host,
        &crate::remote::Request {
            script: format!("rm -f -- {}", crate::remote::sh_path(&target.path)),
            ..crate::remote::Request::default()
        },
    )?;
    if out.code == 0 {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// Whether `plan.path` is still in the conflict `plan` was made for, both sides as they were
/// (blocking: in the background).
pub fn still_in_conflict(repo: &Path, plan: &Plan) -> Result<(), String> {
    let stages = git::conflict_stages(repo, &plan.path)?;
    let same =
        |stage: Option<&Stage>, version: &Version| stage.is_some_and(|s| s.oid == version.oid);
    if same(stages.ours.as_ref(), &plan.ours) && same(stages.theirs.as_ref(), &plan.theirs) {
        Ok(())
    } else {
        Err(crate::i18n::message(
            "repo-core-0589",
            &[
                ("path", format!("{}", plan.path).into()),
                ("target", format!("{}", plan.ours.target).into()),
                ("target2", format!("{}", plan.theirs.target).into()),
            ],
        ))
    }
}

/// The steps that stage both files (also when a `.gitignore` names them), then take
/// `plan.path` out, as it was in conflict: added first, so a refused add leaves the conflict as
/// it was rather than resolved as a deletion.
pub fn stage_steps(plan: &Plan) -> Vec<Vec<String>> {
    let force = plan.ours.ignored || plan.theirs.ignored;
    let mut add = vec!["--literal-pathspecs", "add"];
    if force {
        add.push("-f");
    }
    add.extend(["--", &plan.ours.target, &plan.theirs.target]);
    let steps: [&[&str]; 2] = [
        &add,
        &["--literal-pathspecs", "rm", "-q", "-f", "--", &plan.path],
    ];
    crate::repo::ops(&steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names_become_file_name_tags() {
        assert_eq!(tag("feature/x"), "feature-x");
        assert_eq!(tag("main"), "main");
        assert_eq!(tag("fix: a*b?"), "fix-a-b");
        assert_eq!(tag("users//jö/ünï"), "users-jö-ünï");
        assert_eq!(tag("..hidden."), "hidden");
        assert_eq!(tag("///"), "");
        assert_eq!(tag(""), "");
    }

    #[test]
    fn the_tag_goes_before_the_extension() {
        assert_eq!(tagged("logo.png", "main"), "logo.main.png");
        assert_eq!(tagged("a b/lö go.tar.gz", "x"), "a b/lö go.tar.x.gz");
        assert_eq!(tagged("Makefile", "main"), "Makefile.main");
        assert_eq!(tagged(".env", "main"), ".env.main");
        assert_eq!(tagged("dir.d/file", "t"), "dir.d/file.t");
        assert_eq!(tagged("dir/.hidden.png", "t"), "dir/.hidden.t.png");
    }

    #[test]
    fn the_two_tags_differ() {
        assert_eq!(
            tags((Some("main"), Some("feature/x")), false),
            ("main".into(), "feature-x".into())
        );
        assert_eq!(tags((None, None), false), ("ours".into(), "theirs".into()));
        assert_eq!(
            tags((None, Some("t")), true),
            ("upstream".into(), "t".into())
        );
        assert_eq!(
            tags((Some("m"), None), true),
            ("m".into(), "your-commit".into())
        );
        // The same branch on both sides (a stash popped on its own branch), or names that
        // only differ in case (one file on Windows and macOS).
        assert_eq!(
            tags((Some("main"), Some("Main")), false),
            ("main-ours".into(), "Main-theirs".into())
        );
        assert_eq!(
            tags((Some("jö"), Some("JÖ")), false),
            ("jö-ours".into(), "JÖ-theirs".into())
        );
        assert_eq!(
            tags((Some("///"), Some("..")), false),
            ("ours".into(), "theirs".into())
        );
    }

    #[test]
    fn taken_names_are_passed_over() -> Result<(), String> {
        let on_disk = ["logo.main.png", "logo.main-2.png"];
        let taken = |n: &str| Ok(on_disk.contains(&n));
        let (name, passed) = free_name("logo.png", "main", None, taken)?;
        assert_eq!(name, "logo.main-3.png");
        assert_eq!(passed, on_disk.map(str::to_owned).to_vec());
        // Also when only the case of a letter outside ASCII differs.
        let (name, _) = free_name("a.png", "jö", Some("a.JÖ.png"), |_| Ok(false))?;
        assert_eq!(name, "a.jö-2.png");
        // Not the name the other version takes, also when only the case differs.
        let (name, _) = free_name("logo.png", "x", Some("LOGO.X.png"), |_| Ok(false))?;
        assert_eq!(name, "logo.x-2.png");
        // Errors are told, not taken for a free name.
        assert!(free_name("f", "t", None, |_| Err("no answer".to_owned())).is_err());
        let full = free_name("f", "t", None, |_| Ok(true));
        assert!(full.is_err_and(|e| e.contains("no free name")));
        Ok(())
    }
}
