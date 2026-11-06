//! The worktree setup recipe: what every new worktree of a repository gets before work starts
//! in it. A fresh worktree has none of the files git ignores (`.env.local`, `node_modules`,
//! build caches), and its servers would fight the other worktrees' for ports. The recipe
//! copies or links such files from the main worktree, sets variables per worktree and runs a
//! setup command, in `sh` on the repository's machine, in a terminal where its output shows.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::sh;

/// The file of the repository's git folder the recipe is kept in.
pub(crate) const FILE: &str = "worktree-setup.json";

/// A variable set in a new worktree's terminal, for the setup command and the agent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EnvVar {
    pub name: String,
    /// Read by `sh` as between double quotes: `$((3000 + GITCRAFT_WORKTREE_INDEX))` expands.
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Recipe {
    /// Paths in the main worktree (relative to its top folder) copied into the new one.
    pub copy: Vec<String>,
    /// Paths in the main worktree the new one gets a symbolic link to.
    pub link: Vec<String>,
    pub env: Vec<EnvVar>,
    /// Run with `sh -e` in the new worktree once the files and variables are in.
    pub command: String,
}

impl Recipe {
    pub fn is_empty(&self) -> bool {
        self.copy.is_empty()
            && self.link.is_empty()
            && self.env.is_empty()
            && self.command.trim().is_empty()
    }

    /// What is wrong with it, in words for the user; empty when it can be saved.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for path in self.copy.iter().chain(&self.link) {
            if let Some(why) = path_problem(path) {
                out.push(format!("{path}: {why}"));
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        for var in &self.env {
            if !sh::valid_name(&var.name) {
                out.push(format!(
                    "{}: a variable's name is letters, digits and _, not starting with a digit",
                    var.name
                ));
            } else if seen.contains(&var.name.as_str()) {
                out.push(format!("{} is set twice", var.name));
            }
            if var.value.contains('\n') {
                out.push(format!("{}: a value is one line", var.name));
            }
            seen.push(&var.name);
        }
        out
    }

    /// The recipe as a part of a script that runs in the new worktree's top folder. It leaves
    /// `gitcraft_failed` at 1 when a step failed, and what it did on the terminal.
    pub fn script(&self) -> String {
        let mut s = String::from(PRELUDE);
        if self.is_empty() {
            return s;
        }
        s.push_str("gitcraft_say 'setting up this worktree (Worktree setup recipe)'\n");
        for (verb, paths) in [("copy", &self.copy), ("link", &self.link)] {
            for path in paths {
                s.push_str(&format!("gitcraft_bring {verb} {}\n", sh::quote(path)));
            }
        }
        for var in &self.env {
            s.push_str(&env_line(var));
        }
        if !self.command.trim().is_empty() {
            s.push_str("if [ \"$gitcraft_failed\" = 0 ]; then\n");
            s.push_str(&sh::assign_verbatim(
                "gitcraft_cmd",
                self.command.trim_end(),
            ));
            s.push_str(COMMAND);
            s.push_str("fi\n");
        }
        s.push_str(
            "if [ \"$gitcraft_failed\" = 0 ]; then gitcraft_say 'setup done'; \
             else gitcraft_say 'setup failed: fix the recipe (Worktree setup… in the palette) and run it again'; fi\n",
        );
        s
    }

    pub fn load(repo: &Path) -> Result<Self, String> {
        super::read_json(repo, FILE)
    }

    pub fn save(&self, repo: &Path) -> Result<(), String> {
        let problems = self.problems();
        if let Some(first) = problems.first() {
            return Err(format!("The recipe isn't saved: {first}"));
        }
        super::write_json(repo, FILE, self)
    }
}

/// What every script starts with, recipe or not: where the main worktree is, which number this
/// worktree is (the main one is 0), and the helpers the steps use.
const PRELUDE: &str = r#"gitcraft_say() { printf '\033[36mgitcraft:\033[0m %s\n' "$*"; }
gitcraft_failed=0
gitcraft_top=$(git rev-parse --show-toplevel 2>/dev/null) || gitcraft_top=$PWD
gitcraft_main=$(git worktree list --porcelain 2>/dev/null | sed -n '1s/^worktree //p')
GITCRAFT_WORKTREE_INDEX=$(git worktree list --porcelain 2>/dev/null | awk -v p="$gitcraft_top" 'substr($0, 1, 9) == "worktree " { if (substr($0, 10) == p) { print n + 0; exit } n++ }')
export GITCRAFT_MAIN_WORKTREE="$gitcraft_main"
export GITCRAFT_WORKTREE_INDEX="${GITCRAFT_WORKTREE_INDEX:-0}"
gitcraft_bring() {
  if [ -z "$gitcraft_main" ] || [ "$gitcraft_main" = "$gitcraft_top" ]; then
    gitcraft_say "skipped $2: this is the main worktree"; return 0
  fi
  if [ ! -e "$gitcraft_main/$2" ] && [ ! -L "$gitcraft_main/$2" ]; then
    gitcraft_say "skipped $2: it isn't in the main worktree"; return 0
  fi
  if [ -e "$2" ] || [ -L "$2" ]; then
    gitcraft_say "kept $2: it is here already"; return 0
  fi
  case $2 in */*) mkdir -p -- "${2%/*}" ;; esac
  if [ "$1" = link ]; then
    ln -s -- "$gitcraft_main/$2" "$2" && gitcraft_say "linked $2" && return 0
  else
    cp -R -- "$gitcraft_main/$2" "$2" && gitcraft_say "copied $2" && return 0
  fi
  gitcraft_say "could not $1 $2"; gitcraft_failed=1
}
"#;

/// Runs the setup command (in `gitcraft_cmd`) and says how it ended.
const COMMAND: &str = r#"gitcraft_say 'running the setup command:'
printf '%s\n' "$gitcraft_cmd" | sed 's/^/  $ /'
sh -e -c "$gitcraft_cmd"
gitcraft_code=$?
if [ "$gitcraft_code" -ne 0 ]; then
  gitcraft_say "the setup command failed (exit status $gitcraft_code)"; gitcraft_failed=1
fi
"#;

/// Sets `var`, worked out in a subshell first: a mistake in its value (`$((3000 +))`) would end
/// the whole script otherwise.
fn env_line(var: &EnvVar) -> String {
    let value = sh::double_quoted(&var.value);
    let name = &var.name;
    format!(
        "if gitcraft_value=$(printf '%s' {value}); then export {name}=\"$gitcraft_value\"; \
         gitcraft_say \"{name}=${name}\"; else gitcraft_say {}; gitcraft_failed=1; fi\n",
        sh::quote(&format!("could not work out {name}"))
    )
}

/// Why `path` can't be a path inside a worktree, if it can't.
fn path_problem(path: &str) -> Option<&'static str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Some("empty path");
    }
    if trimmed != path {
        return Some("starts or ends with a space");
    }
    if path.contains('\n') || path.contains('\0') {
        return Some("a path is one line");
    }
    let windows_drive = path.len() > 1 && path.as_bytes().get(1) == Some(&b':');
    if path.starts_with(['/', '\\', '~']) || windows_drive {
        return Some("give a path inside the worktree, relative to its top folder");
    }
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Some("a path can't go out of the worktree (..)");
    }
    if path
        .split(['/', '\\'])
        .all(|part| part.is_empty() || part == ".")
    {
        return Some("that is the whole worktree");
    }
    None
}

/// The non-empty lines of `text`, for the lists of paths.
pub(crate) fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// `NAME=value` lines into variables; the lines without `=` as variables without a value
/// (which [`Recipe::problems`] doesn't take issue with: an empty value is a value).
pub(crate) fn env_lines(text: &str) -> Vec<EnvVar> {
    lines(text)
        .into_iter()
        .map(|line| match line.split_once('=') {
            Some((name, value)) => EnvVar {
                name: name.trim().to_owned(),
                value: value.to_owned(),
            },
            None => EnvVar {
                name: line.trim().to_owned(),
                value: String::new(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_must_stay_inside_the_worktree() {
        assert_eq!(path_problem(".env.local"), None);
        assert_eq!(path_problem("apps/web/.env"), None);
        assert_eq!(path_problem("dir with space/ü.txt"), None);
        assert!(path_problem("/etc/passwd").is_some());
        assert!(path_problem("C:\\x").is_some());
        assert!(path_problem("~/x").is_some());
        assert!(path_problem("a/../../b").is_some());
        assert!(path_problem("./").is_some());
        assert!(path_problem("").is_some());
        assert!(path_problem(" x").is_some());
    }

    #[test]
    fn variables_need_shell_names_once_each() {
        let recipe = Recipe {
            env: env_lines("PORT=$((3000 + GITCRAFT_WORKTREE_INDEX))\nPORT=1\n9X=2\nEMPTY"),
            ..Recipe::default()
        };
        let problems = recipe.problems();
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("twice"));
        assert!(problems[1].starts_with("9X"));
        assert_eq!(recipe.env[3].value, "");
    }

    #[test]
    fn lines_drop_blank_ones_and_keep_spaces_inside() {
        assert_eq!(lines("a b\r\n\n  \nc\n"), vec!["a b", "c"]);
    }

    #[test]
    fn an_empty_recipe_only_sets_the_worktree_number() {
        let script = Recipe::default().script();
        assert!(script.contains("GITCRAFT_WORKTREE_INDEX"));
        assert!(!script.contains("setting up"));
    }

    #[test]
    fn a_recipe_with_problems_is_not_saved() {
        let recipe = Recipe {
            copy: vec!["../x".to_owned()],
            ..Recipe::default()
        };
        let saved = recipe.save(Path::new("/nonexistent"));
        assert!(saved.is_err_and(|e| e.contains("isn't saved")));
    }

    #[test]
    fn a_recipe_reads_with_missing_fields() -> Result<(), serde_json::Error> {
        let recipe: Recipe = serde_json::from_str(r#"{"command": "npm ci"}"#)?;
        assert_eq!(recipe.command, "npm ci");
        assert!(recipe.copy.is_empty());
        Ok(())
    }
}
