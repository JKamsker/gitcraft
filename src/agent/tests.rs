//! The setup recipe and the agent launch run for real: scripts made here run with `sh` in new
//! worktrees of temporary repositories, the way the terminal runs them.

use std::path::{Path, PathBuf};

use super::launch::Launch;
use super::recipe::{EnvVar, Recipe};
use super::tasks::{self, AgentTask};
use crate::git;

/// A repository with one commit in a folder whose name has a space and letters outside ASCII,
/// `.env.local` and `node_modules/` ignored and there, and a worktree made beside it.
struct Fixture {
    root: PathBuf,
    main: PathBuf,
    worktree: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Result<Self, String> {
        let root =
            std::env::temp_dir().join(format!("gitcraft-agent-{name}-{} ü x", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let main = root.join("main repo");
        std::fs::create_dir_all(main.join("node_modules/pkg")).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| git::run(Some(&main), args);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            git(args)?;
        }
        write(&main.join(".gitignore"), ".env.local\nnode_modules/\n")?;
        write(&main.join("README.md"), "hi\n")?;
        write(&main.join(".env.local"), "SECRET=1\n")?;
        write(&main.join("node_modules/pkg/index.js"), "x\n")?;
        git(&["add", "-A"])?;
        git(&["commit", "-qm", "one"])?;
        let worktree = root.join("wt ñ");
        let path = worktree.display().to_string();
        git(&["worktree", "add", "-q", "-b", "agent/x", &path, "HEAD"])?;
        Ok(Self {
            root,
            main,
            worktree,
        })
    }

    /// Runs the launch's script as the terminal would here, ending with `true` instead of a
    /// shell: (exit code, what it printed).
    #[cfg(unix)]
    fn run(&self, launch: &Launch) -> Result<(i32, String), String> {
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(launch.script("true"))
            .current_dir(&self.root)
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        Ok((out.status.code().unwrap_or(-1), text))
    }

    fn launch(&self, recipe: Recipe, agent: Option<&str>) -> Launch {
        Launch {
            label: "setup".to_owned(),
            worktree: self.worktree.clone(),
            recipe,
            agent: agent.map(str::to_owned),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(unix)]
#[test]
fn the_recipe_copies_links_sets_variables_and_runs_its_command() -> Result<(), String> {
    let fx = Fixture::new("recipe")?;
    let recipe = Recipe {
        copy: vec![".env.local".to_owned(), "missing.txt".to_owned()],
        link: vec!["node_modules".to_owned()],
        env: vec![
            EnvVar {
                name: "PORT".to_owned(),
                value: "$((3000 + GITCRAFT_WORKTREE_INDEX))".to_owned(),
            },
            EnvVar {
                name: "QUOTED".to_owned(),
                value: r#"a "b" \c"#.to_owned(),
            },
        ],
        command: "printf '%s\\n' \"port=$PORT quoted=$QUOTED\" > setup-ran.txt\nprintf '%s' 'GITCRAFT_END' >> setup-ran.txt".to_owned(),
    };
    let (code, out) = fx.run(&fx.launch(recipe, Some("echo agent-started")))?;
    assert_eq!(code, 0, "{out}");
    let wt = &fx.worktree;
    assert_eq!(
        std::fs::read_to_string(wt.join(".env.local")).map_err(|e| e.to_string())?,
        "SECRET=1\n"
    );
    let link = std::fs::symlink_metadata(wt.join("node_modules")).map_err(|e| e.to_string())?;
    assert!(link.file_type().is_symlink());
    assert!(wt.join("node_modules/pkg/index.js").is_file());
    let ran = std::fs::read_to_string(wt.join("setup-ran.txt")).map_err(|e| e.to_string())?;
    assert_eq!(ran, "port=3001 quoted=a \"b\" \\c\nGITCRAFT_END", "{out}");
    assert!(out.contains("skipped missing.txt"), "{out}");
    assert!(out.contains("setup done"), "{out}");
    assert!(out.contains("agent-started"), "{out}");
    // A second run keeps what is there.
    let (_, again) = fx.run(&fx.launch(
        Recipe {
            copy: vec![".env.local".to_owned()],
            ..Recipe::default()
        },
        None,
    ))?;
    assert!(again.contains("kept .env.local"), "{again}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_failing_step_says_so_and_the_agent_is_not_started() -> Result<(), String> {
    let fx = Fixture::new("fails")?;
    let recipe = Recipe {
        command: "echo first\nfalse\necho never".to_owned(),
        ..Recipe::default()
    };
    let (code, out) = fx.run(&fx.launch(recipe, Some("echo agent-started")))?;
    assert_eq!(code, 0, "the shell still starts: {out}");
    assert!(out.contains("first"), "{out}");
    assert!(
        !out.contains("\nnever"),
        "sh -e stops at the failure: {out}"
    );
    assert!(
        out.contains("the setup command failed (exit status 1)"),
        "{out}"
    );
    assert!(out.contains("setup failed"), "{out}");
    assert!(!out.contains("\nagent-started"), "{out}");
    assert!(
        out.contains("start it yourself with: echo agent-started"),
        "{out}"
    );

    // A value that can't be worked out fails the setup without ending the script.
    let bad = Recipe {
        env: vec![EnvVar {
            name: "PORT".to_owned(),
            value: "$((3000 +))".to_owned(),
        }],
        command: "echo never-ran > ran.txt".to_owned(),
        ..Recipe::default()
    };
    let (code, out) = fx.run(&fx.launch(bad, None))?;
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("could not work out PORT"), "{out}");
    assert!(!fx.worktree.join("ran.txt").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_worktree_that_is_gone_still_gets_a_shell() -> Result<(), String> {
    let fx = Fixture::new("gone")?;
    let mut launch = fx.launch(Recipe::default(), Some("echo agent-started"));
    launch.worktree = fx.root.join("nowhere");
    let (code, out) = fx.run(&launch)?;
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("is not there"), "{out}");
    assert!(!out.contains("agent-started"), "{out}");
    Ok(())
}

/// The way a script reaches WSL, SSH and Git's `sh` on Windows: in base64, decoded into a file
/// that is read with the terminal as its input, and removed.
#[cfg(unix)]
#[test]
fn the_script_handed_over_in_base64_runs_and_leaves_no_file() -> Result<(), String> {
    let fx = Fixture::new("b64")?;
    let launch = fx.launch(
        Recipe {
            copy: vec![".env.local".to_owned()],
            ..Recipe::default()
        },
        Some("echo \"agent in $(basename \"$PWD\")\" \"$gitcraft_f\""),
    );
    let shell = crate::terminal::ShellSpec {
        label: "sh".to_owned(),
        program: "true".to_owned(),
        args: Vec::new(),
    };
    // As for Windows: `sh -c <decode> sh <base64>`.
    let script = launch.script("true");
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&script);
    let decode = super::launch::decode_for_tests();
    let out = std::process::Command::new("/bin/sh")
        .args(["-c", decode, "sh", &b64])
        .current_dir(&fx.root)
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("agent in wt ñ"), "{text}");
    let file = text
        .lines()
        .find_map(|l| l.strip_prefix("agent in wt ñ "))
        .unwrap_or_default();
    assert!(!file.is_empty() && !Path::new(file).exists(), "{text}");
    assert!(fx.worktree.join(".env.local").is_file());
    // Here the script is given as it is, the shell it ends with quoted.
    let (spec, cwd) = launch.program(&shell, || Err("not on Windows".to_owned()))?;
    assert_eq!(spec.program, "/bin/sh");
    assert_eq!(cwd, fx.worktree);
    assert!(spec.args[1].ends_with("exec 'true'\n"), "{}", spec.args[1]);
    Ok(())
}

#[test]
fn a_repository_elsewhere_gets_its_script_in_words_every_shell_reads() -> Result<(), String> {
    let launch = Launch {
        label: "x".to_owned(),
        worktree: PathBuf::from("ssh://build/home/me/wt x"),
        recipe: Recipe::default(),
        agent: Some("claude 'it'\\''s'".to_owned()),
    };
    let shell = crate::terminal::ShellSpec {
        label: "sh".to_owned(),
        program: "sh".to_owned(),
        args: Vec::new(),
    };
    let (spec, _) = launch.program(&shell, || Err("unused".to_owned()))?;
    assert_eq!(spec.program, "ssh");
    let words = spec.args.last().cloned().unwrap_or_default();
    let inner = words
        .strip_prefix("exec sh -c '")
        .and_then(|rest| rest.split_once("' sh "))
        .map(|(decode, b64)| (decode.to_owned(), b64.to_owned()));
    let Some((decode, b64)) = inner else {
        return Err(words);
    };
    assert!(!decode.contains(['\'', '\\', '!', '"']), "{decode}");
    assert!(
        b64.chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c))
    );
    use base64::Engine;
    let script = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| e.to_string())?;
    let script = String::from_utf8(script).map_err(|e| e.to_string())?;
    assert!(script.contains("cd -- '/home/me/wt x'"), "{script}");
    assert!(script.ends_with("exec \"${SHELL:-sh}\" -l\n"), "{script}");

    // `\\wsl.localhost\…` is a WSL folder only on Windows.
    let wsl = Launch {
        worktree: PathBuf::from(r"\\wsl.localhost\Ubuntu\home\me\wt"),
        ..launch
    };
    if crate::remote::is_remote(&wsl.worktree) {
        let (spec, _) = wsl.program(&shell, || Err("unused".to_owned()))?;
        assert_eq!(spec.program, "wsl.exe");
        assert_eq!(spec.args[..5], ["-d", "Ubuntu", "-e", "sh", "-c"]);
    }
    Ok(())
}

#[test]
fn git_for_windows_sh_is_looked_for_beside_its_git() {
    let found = super::launch::sh_beside_git("C:/Program Files/Git/mingw64/libexec/git-core\n");
    assert_eq!(
        found
            .first()
            .map(|p| p.display().to_string().replace('\\', "/")),
        Some("C:/Program Files/Git/usr/bin/sh.exe".to_owned())
    );
}

#[test]
fn a_pending_launch_waits_for_its_own_worktree() {
    use super::launch::PendingLaunches;
    let path = PathBuf::from("ssh://h/home/me/pending-test-wt");
    PendingLaunches::queue(Launch {
        label: "a".to_owned(),
        worktree: path.clone(),
        recipe: Recipe::default(),
        agent: None,
    });
    assert!(PendingLaunches::take(Path::new("ssh://h/home/me/other")).is_none());
    assert!(PendingLaunches::take(Path::new("ssh://h/home/me/pending-test-wt/")).is_some());
    assert!(PendingLaunches::take(&path).is_none(), "taken once");
}

#[test]
fn recipe_and_tasks_are_kept_in_the_shared_git_folder() -> Result<(), String> {
    let fx = Fixture::new("files")?;
    assert_eq!(Recipe::load(&fx.main)?, Recipe::default());
    let recipe = Recipe {
        copy: vec![".env.local".to_owned()],
        command: "npm ci".to_owned(),
        ..Recipe::default()
    };
    recipe.save(&fx.main)?;
    // Read from the other worktree: one recipe for the repository.
    assert_eq!(Recipe::load(&fx.worktree)?, recipe);
    assert!(fx.main.join(".git/gitcraft/worktree-setup.json").is_file());

    let task = AgentTask {
        branch: "agent/x".to_owned(),
        path: fx.worktree.display().to_string(),
        agent: "claude".to_owned(),
        prompt: "fix ü".to_owned(),
        base: "abc".to_owned(),
        started: 1,
    };
    tasks::add(&fx.main, task.clone())?;
    let again = tasks::add(
        &fx.worktree,
        AgentTask {
            prompt: "again".to_owned(),
            ..task.clone()
        },
    )?;
    assert_eq!(again.len(), 1, "one task per worktree");
    assert_eq!(tasks::load(&fx.main)?[0].prompt, "again");
    assert!(tasks::remove(&fx.main, &task.path)?.is_empty());
    // A file that isn't JSON says which file.
    write(&fx.main.join(".git/gitcraft/agents.json"), "{oops")?;
    let err = tasks::load(&fx.main).err().unwrap_or_default();
    assert!(err.contains("agents.json"), "{err}");
    Ok(())
}

#[test]
fn a_folder_that_is_no_repository_says_so() {
    let dir = std::env::temp_dir().join(format!("gitcraft-agent-norepo-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let err = Recipe::load(&dir).err().unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err.contains("could not find the git folder"), "{err}");
}
