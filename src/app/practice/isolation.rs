//! Run hostile inherited environment cases in a fresh test process. This avoids mutating the
//! environment/profile of other tests, and exercises the actual App and operation runner.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::app::{App, Tab};
use crate::git;
use crate::practice::Scenario;
use crate::repo::{Action, Refresh, Undo};

type H<'a> = egui_kittest::Harness<'a, App>;

fn wait(harness: &mut H<'_>) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        harness.step();
        let app = harness.state();
        if app.practicing.is_none()
            && matches!(app.tabs.get(app.active), Some(Tab::Repo(tab)) if tab.settled())
        {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(format!("Practice app did not settle: {:?}", app.error));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn apply(harness: &mut H<'_>, action: Action) -> Result<(), String> {
    let ctx = harness.ctx.clone();
    let active = harness.state().active;
    let Some(Tab::Repo(tab)) = harness.state_mut().tabs.get_mut(active) else {
        return Err("No practice tab".to_owned());
    };
    tab.apply(action, &ctx);
    wait(harness)
}

fn operation(args: &[&str]) -> Action {
    Action::UndoableOps {
        label: "Practice regression operation".to_owned(),
        ops: vec![args.iter().map(|s| (*s).to_owned()).collect()],
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

fn exercise_app(root: &Path) -> Result<(), String> {
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 900.0])
        .build_eframe(|cc| {
            let mut app = App::new(cc, None);
            app.settings.check_updates = false;
            app.settings.set(&cc.egui_ctx);
            app
        });
    // This is the real active-profile override; normal tabs must keep it, practice must not.
    // A selected SSH/account profile with a configuration error must never obstruct or
    // redirect a practice command, including captured worker contexts and binary reads.
    let hostile_context = crate::connections::Context {
        git_config: Some(root.join("hostile.gitconfig")),
        choices: crate::connections::Choices {
            ssh_key: root.join("missing-selected-key").to_string_lossy().into_owned(),
            github: crate::connections::Account {
                host: "fixture.invalid".into(),
                user: "selected-hostile-account".into(),
                config_dir: root.join("missing-account-config").to_string_lossy().into_owned(),
            },
            ..Default::default()
        },
        error: Some("Fixture profile must not govern Practice".into()),
        ..Default::default()
    };
    crate::connections::set(hostile_context.clone());
    assert!(git::run(Some(&root.join("caller")), &["rev-parse", "HEAD"])
        .is_err_and(|error| error.contains("Fixture profile")));
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .start_practice(Scenario::Published, &ctx);
    wait(&mut harness)?;
    let path = match harness.state().tabs.get(harness.state().active) {
        Some(Tab::Repo(tab)) => tab.path.clone(),
        _ => return Err("Practice did not open".to_owned()),
    };
    let session = crate::practice::load(&path).ok_or("No practice scenario")?;
    let before = git::head(&path);
    assert_ne!(
        before,
        git::practice_run(&root.join("caller"), &["rev-parse", "HEAD"])?.trim()
    );
    assert!(git::run(Some(&path), &["config", "--get", "practice.profile"]).is_err());
    assert!(git::run(Some(&path), &["config", "--get", "practice.injected"]).is_err());
    assert_eq!(git::identity(&path).email, "practice@example.invalid");
    // New shared activity transport must use the same practice-aware command builder.
    let activity = git::activity::read(
        &path,
        100,
        &|| true,
        &std::sync::atomic::AtomicUsize::new(0),
    )?;
    assert!(!activity.commits.is_empty());
    assert!(
        activity
            .commits
            .iter()
            .all(|c| c.email == "practice@example.invalid" && c.subject != "Caller history")
    );
    use egui_kittest::kittest::Queryable as _;
    let active = harness.state().active;
    let action = match harness.state().tabs.get(active) {
        Some(Tab::Repo(tab)) => tab
            .palette_items()
            .into_iter()
            .find(|(label, _, _)| label == "Activity calendar")
            .map(|(_, _, action)| action),
        _ => None,
    }
    .ok_or("No activity command")?;
    apply(&mut harness, action)?;
    let until = Instant::now() + Duration::from_secs(5);
    let label = format!("{} unique commits", activity.commits.len());
    loop {
        harness.step();
        if harness.query_by_label_contains(&label).is_some() {
            break;
        }
        if Instant::now() >= until {
            return Err("Practice activity did not show its own history".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    apply(&mut harness, Action::CloseActivity)?;
    // Streaming/piped history is a separate spawn route from ordinary reads/mutations.
    let mut stream = git::LogStream::start(&path, None, 0)?;
    assert!(
        stream
            .next_page(10)?
            .iter()
            .any(|commit| commit.hash == before)
    );
    drop(stream);
    assert_eq!(
        git::run_bytes(Some(&path), &["show", "HEAD:greeting.txt"])?,
        b"Oops, this greeting was a mistake!\n"
    );
    apply(
        &mut harness,
        operation(&["commit", "--allow-empty", "-m", "A practice-only commit"]),
    )?;
    assert_ne!(git::head(&path), before);
    assert_eq!(
        git::run(Some(&path), &["log", "-1", "--format=%an <%ae>"])?.trim(),
        "Gitcraft Practice <practice@example.invalid>"
    );
    apply(&mut harness, operation(&["revert", "--no-edit", &before]))?;
    apply(&mut harness, Action::Push)?;
    assert!(
        session.check(&path)?,
        "The App's plain push must reach the local practice origin"
    );

    // Every binary/bounded bundle path must use the same practice context as the App.
    // With inherited caller GIT_DIR and an active hostile profile, a raw command builder
    // would expose caller refs here even though the normal HEAD read stays in practice.
    let bundle_refs = git::bundle_refs(&path)?;
    let bundle_head = git::head(&path);
    assert!(
        bundle_refs
            .iter()
            .all(|reference| reference.oid == bundle_head),
        "Bounded bundle ref reads must remain in the practice repository"
    );
    let selected = bundle_refs
        .iter()
        .find(|reference| reference.name == "HEAD")
        .ok_or("No practice bundle HEAD")?;
    let bytes = git::export_bundle(&path, std::slice::from_ref(selected), None)?;
    let bundle_file = root.join("isolated-practice.bundle");
    git::save_bundle_file(&bundle_file, &bytes)?;
    let snapshot = git::read_bundle_file(&bundle_file)?;
    assert_eq!(snapshot, bytes);
    let refs_before_preview = git::run(Some(&path), &["show-ref"])?;
    let preview = git::inspect_bundle(&path, snapshot)?;
    assert!(
        preview.verification_error.is_none(),
        "{:?}",
        preview.verification_error
    );
    assert_eq!(git::run(Some(&path), &["show-ref"])?, refs_before_preview);
    git::import_bundle(
        &path,
        &preview,
        &[git::BundleImport {
            source: "HEAD".to_owned(),
            destination: "refs/heads/practice-bundle-copy".to_owned(),
        }],
    )?;
    assert_eq!(
        git::resolve(&path, "refs/heads/practice-bundle-copy")?,
        bundle_head
    );
    assert!(
        !root.join("escaped").exists(),
        "Bundle paths must not run caller/profile helpers"
    );

    // Opening a linked practice worktree must not return to inherited caller routing.
    let linked = path
        .parent()
        .ok_or("No sandbox root")?
        .join("linked practice ü");
    git::run(
        Some(&path),
        &[
            "worktree",
            "add",
            "-b",
            "practice-side",
            &linked.to_string_lossy(),
        ],
    )?;
    assert!(crate::practice::is_repository(&linked));
    assert_eq!(git::head(&linked), git::head(&path));
    git::run(
        Some(&linked),
        &["commit", "--allow-empty", "-m", "Linked practice only"],
    )?;
    assert_ne!(git::head(&linked), git::head(&path));
    // A subfolder/symlink alias also routes to the practice repo before Git can see GIT_DIR.
    let sub = path.join("subfolder");
    std::fs::create_dir(&sub).map_err(|e| e.to_string())?;
    assert_eq!(git::head(&sub), git::head(&path));
    let alias = root.join("practice-alias");
    std::os::unix::fs::symlink(&path, &alias).map_err(|e| e.to_string())?;
    assert_eq!(git::head(&alias), git::head(&path));

    // Spawn the same terminal implementation offered by a practice tab, with the same
    // inherited routing/configuration. Its ordinary git commit/push must remain local too.
    let receipt = root.join("shell-receipt");
    let script = format!(
        "set -eu; git rev-parse --absolute-git-dir > {receipt}; git commit --allow-empty -m 'A practice shell commit'; git push origin main; printf '\\nfinished\\n' >> {receipt}",
        receipt = crate::remote::sh_quote(&receipt.to_string_lossy())
    );
    let shell = crate::terminal::ShellSpec {
        label: "Isolation test shell".to_owned(),
        program: "/bin/sh".to_owned(),
        args: vec!["-c".to_owned(), script],
    };
    apply(
        &mut harness,
        Action::NewTerminal {
            shell: Some(shell),
            cwd: None,
        },
    )?;
    let until = Instant::now() + Duration::from_secs(10);
    let receipt = loop {
        harness.step();
        if let Ok(text) = std::fs::read_to_string(&receipt)
            && text.ends_with("finished\n")
        {
            break text;
        }
        if Instant::now() >= until {
            return Err("The practice tab's shell did not finish its local commit/push".to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(receipt.contains(&path.join(".git").to_string_lossy().into_owned()));
    let origin = path.parent().ok_or("No sandbox root")?.join("origin.git");
    assert_eq!(
        git::practice_run(&origin, &["rev-parse", "refs/heads/main"])?.trim(),
        git::head(&path)
    );
    assert!(
        !root.join("escaped").exists(),
        "No inherited hook, signing tool, fsmonitor or SSH helper ran"
    );

    // The same active profile still applies to a non-practice repository. This fix must
    // not globally sanitize the user's environment or change normal tab behavior.
    crate::connections::set(crate::connections::Context {
        error: None,
        ..hostile_context
    });
    assert_eq!(
        git::run(
            Some(&root.join("caller")),
            &["config", "--get", "practice.profile"]
        )?
        .trim(),
        "normal-tabs-keep-this"
    );
    drop(harness);
    crate::connections::set(Default::default());
    std::fs::remove_dir_all(path.parent().ok_or("No sandbox root")?).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn normal_practice_tab_operations_and_shells_are_isolated() -> Result<(), String> {
    const CHILD: &str = "GITCRAFT_PRACTICE_ISOLATION_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        return exercise_app(&PathBuf::from(root));
    }
    let holder = crate::practice::create(Scenario::Detached)?;
    let root = holder.parent().ok_or("No fixture root")?.to_owned();
    let caller = root.join("caller");
    std::fs::create_dir(&caller).map_err(|e| e.to_string())?;
    for args in [
        &["init", "--template=", "-b", "main"][..],
        &["config", "user.name", "Caller"],
        &["config", "user.email", "caller@example.invalid"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git::practice_run(&caller, args)?;
    }
    std::fs::write(caller.join("precious.txt"), "Untouched caller work\n")
        .map_err(|e| e.to_string())?;
    git::practice_run(&caller, &["add", "precious.txt"])?;
    git::practice_run(&caller, &["commit", "-m", "Caller history"])?;
    std::fs::write(caller.join("staged.txt"), "Untouched caller index\n")
        .map_err(|e| e.to_string())?;
    git::practice_run(&caller, &["add", "staged.txt"])?;
    let refs = git::practice_run(&caller, &["show-ref"])?;
    let read = |file: &Path| std::fs::read(file).map_err(|e| e.to_string());
    let index = read(&caller.join(".git/index"))?;
    let config = read(&caller.join(".git/config"))?;
    let precious = read(&caller.join("precious.txt"))?;
    let hooks = root.join("hostile-hooks");
    std::fs::create_dir(&hooks).map_err(|e| e.to_string())?;
    let trap = format!(
        "#!/bin/sh\nprintf 'escaped\\n' >> {}\nexit 97\n",
        crate::remote::sh_quote(&root.join("escaped").to_string_lossy())
    );
    let helper = root.join("hostile-helper");
    std::fs::write(&helper, &trap).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    for hook in ["pre-commit", "pre-push"] {
        let path = hooks.join(hook);
        std::fs::write(&path, &trap).map_err(|e| e.to_string())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let hostile = root.join("hostile.gitconfig");
    let helper_command = format!(
        "/bin/sh {}",
        crate::remote::sh_quote(&helper.to_string_lossy())
    );
    let prefix = std::env::temp_dir()
        .join("gitcraft-practice-")
        .to_string_lossy()
        .into_owned();
    for (key, value) in [
        ("practice.profile", "normal-tabs-keep-this"),
        ("core.hooksPath", &hooks.to_string_lossy()),
        ("core.fsmonitor", &helper_command),
        ("core.sshCommand", &helper_command),
        ("commit.gpgSign", "true"),
        ("gpg.program", &helper.to_string_lossy()),
        ("url.ssh://never-contact.invalid/.insteadOf", &prefix),
    ] {
        git::practice_run(
            &caller,
            &["config", "--file", &hostile.to_string_lossy(), key, value],
        )?;
    }
    let output = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args([
            "--exact",
            "app::practice::isolation::normal_practice_tab_operations_and_shells_are_isolated",
            "--nocapture",
        ])
        .env(CHILD, &root)
        .env("GIT_DIR", caller.join(".git"))
        .env("GIT_WORK_TREE", &caller)
        .env("GIT_INDEX_FILE", caller.join(".git/index"))
        .env("GIT_COMMON_DIR", caller.join(".git"))
        .env("GIT_CONFIG_GLOBAL", &hostile)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", &hooks)
        .env(
            "GIT_CONFIG_PARAMETERS",
            "'practice.injected=from-environment'",
        )
        .env("GIT_AUTHOR_NAME", "Leaked caller")
        .env("GIT_AUTHOR_EMAIL", "leaked@example.invalid")
        .env("GIT_SSH_COMMAND", &helper_command)
        .env("BASH_ENV", &helper)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        output.status.success(),
        "Child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git::practice_run(&caller, &["show-ref"])?, refs);
    assert_eq!(read(&caller.join(".git/index"))?, index);
    assert_eq!(read(&caller.join(".git/config"))?, config);
    assert_eq!(read(&caller.join("precious.txt"))?, precious);
    assert!(!root.join("escaped").exists());
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}
