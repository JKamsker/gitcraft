use super::*;

pub(super) fn is_link(file: &Path) -> bool {
    crate::remote::is_link(file)
}

/// Writes `text`, the conflicts of `path` (in the repository at `repo`) solved, over it. On
/// another machine, a round trip: in the background.
pub fn save_solved(repo: &Path, path: &str, text: &str) -> Result<(), String> {
    let file = crate::remote::join(repo, path);
    if let Some(target) = crate::remote::parse(&file) {
        return save_there(&target, text).map_err(|e| {
            crate::i18n::message(
                "repo-core-3786",
                &[
                    ("path", path.to_string().into()),
                    ("e", e.to_string().into()),
                ],
            )
        });
    }
    // A link put there since it was opened is not written through.
    if is_link(&file) {
        return Err(crate::i18n::message(
            "repo-core-3787",
            &[("path", path.to_string().into())],
        ));
    }
    std::fs::write(&file, text).map_err(|e| {
        crate::i18n::message(
            "repo-core-3786",
            &[
                ("path", path.to_string().into()),
                ("e", format!("{}", e).into()),
            ],
        )
    })
}

/// What the save script ends with when the file is a link, or not a file, now.
const NOT_A_PLAIN_FILE: i32 = 4;

/// The script that saves a solved file on another machine, from its input: written to a new
/// file beside it (its mode copied), then renamed over it. A rename replaces whatever is at
/// the path, never follows it: a link put there after the check is replaced, not written
/// through.
pub(super) fn save_script(file: &str) -> String {
    let f = crate::remote::sh_path(file);
    let dir = crate::remote::sh_path(crate::remote::parent_there(file));
    format!(
        concat!(
            "f={f}\n",
            "if [ -L \"$f\" ] || [ ! -f \"$f\" ]; then exit {not_plain}; fi\n",
            "t=$(mktemp {dir}/.gitcraft-save.XXXXXX) || exit 1\n",
            "trap 'rm -f \"$t\"' EXIT\n",
            "cat > \"$t\" || exit 1\n",
            "m=$(stat -c %a -- \"$f\" 2>/dev/null || stat -f %Lp -- \"$f\" 2>/dev/null)\n",
            "chmod \"${{m:-644}}\" \"$t\" || exit 1\n",
            "mv -f -- \"$t\" \"$f\"\n",
        ),
        f = f,
        dir = dir,
        not_plain = NOT_A_PLAIN_FILE,
    )
}

/// [`save_solved`] on another machine (blocking: from a thread of its own).
pub(super) fn save_there(target: &crate::remote::Target, text: &str) -> Result<(), String> {
    let out = crate::remote::exec(
        &target.host,
        &crate::remote::Request {
            script: save_script(&target.path),
            stdin: Some(text.as_bytes().to_vec()),
            ..crate::remote::Request::default()
        },
    )?;
    match out.code {
        0 => Ok(()),
        NOT_A_PLAIN_FILE => Err(crate::i18n::text("repo-core-3796").to_owned()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()),
    }
}

/// Whether `path` (in the repository at `repo`) is no longer `opened`, as it was opened:
/// `None` when it couldn't be read (on another machine, a dropped connection says nothing
/// about the file). On another machine, a round trip: in the background.
pub fn changed_on_disk(repo: &Path, path: &str, opened: &str) -> Option<bool> {
    let bytes = crate::git::worktree_file(&crate::remote::join(repo, path)).ok()?;
    Some(String::from_utf8(bytes).ok().as_deref() != Some(opened))
}
