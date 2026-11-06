//! Working with coding agents (Claude Code, Codex, Aider…) in worktrees of their own: the setup
//! recipe a new worktree gets, the terminal an agent runs in, and the tasks started so.
//!
//! What is kept for a repository lives in its common git folder (`.git/gitcraft/`), shared by
//! all its worktrees and never committed. Everything here runs on the repository's machine,
//! WSL and SSH included: files through [`crate::remote`], git through [`crate::git`].

pub(crate) mod bakeoff;
mod files;
pub(crate) mod launch;
pub(crate) mod recipe;
pub(crate) mod sh;
pub(crate) mod tasks;
#[cfg(test)]
mod tests;

pub(crate) use files::{read_json, repo_file, write_json};
pub(crate) use launch::{Launch, PendingLaunches};
pub(crate) use recipe::Recipe;
pub(crate) use tasks::{AgentCli, AgentTask};

/// The folder `git worktree add …` (as gitcraft runs it, `args` from `worktree` on) makes: its
/// first word that is no option nor an option's value. `None` for any other command.
pub(crate) fn added_worktree(args: &[String]) -> Option<&str> {
    let mut rest = match args {
        [worktree, add, rest @ ..] if worktree == "worktree" && add == "add" => rest.iter(),
        _ => return None,
    };
    while let Some(word) = rest.next() {
        match word.as_str() {
            "--" => return rest.next().map(String::as_str),
            // The options of `worktree add` that take a value as the next word.
            "-b" | "-B" | "--reason" => {
                rest.next();
            }
            option if option.starts_with('-') => {}
            path => return Some(path),
        }
    }
    None
}

#[cfg(test)]
mod unit {
    use super::added_worktree;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn the_folder_of_a_new_worktree_is_found_among_its_options() {
        let cases: [(&[&str], Option<&str>); 7] = [
            (
                &["worktree", "add", "-b", "feat", "/w/a b", "HEAD"],
                Some("/w/a b"),
            ),
            (&["worktree", "add", "/w/x", "main"], Some("/w/x")),
            (&["worktree", "add", "--", "-odd", "main"], Some("-odd")),
            (
                &[
                    "worktree", "add", "--detach", "--lock", "--reason", "r", "ü/ñ",
                ],
                Some("ü/ñ"),
            ),
            (&["worktree", "add", "-B", "x"], None),
            (&["worktree", "remove", "/w/x"], None),
            (&["commit", "-m", "worktree add"], None),
        ];
        for (args, folder) in cases {
            assert_eq!(added_worktree(&words(args)), folder, "{args:?}");
        }
    }
}
