//! The in-process policy protocol against the reviewed PowerShell helper, on real files.
use super::*;
use std::path::PathBuf;

fn icacls() -> Result<PathBuf, String> {
    let program = PathBuf::from(std::env::var_os("SystemRoot").ok_or("No SystemRoot")?)
        .join("System32/icacls.exe");
    if !program.is_absolute() {
        return Err("Fixture requires absolute SystemRoot".into());
    }
    Ok(program)
}

fn run_fixture(command: Command) -> Result<(), String> {
    let out = crate::processes::run(
        command,
        crate::processes::Options {
            timeout: Some(std::time::Duration::from_secs(20)),
            output_limit: 1 << 20,
            stderr_limit: Some(4096),
            ..Default::default()
        },
    )?
    .complete()?;
    if !out.status.success() {
        return Err(format!(
            "Owned policy fixture failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// Inherited, explicitly granted, and granted plus low-integrity-labelled files.
fn fixtures(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let program = icacls()?;
    let mut files = Vec::new();
    for (name, grant, label) in [
        ("inherited", false, false),
        ("granted ' $() & ü", true, false),
        ("labelled", true, true),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, b"policy fixture").map_err(|e| e.to_string())?;
        if grant {
            run_fixture(fixture_acl_command(&program, &path, true))?;
        }
        if label {
            run_fixture(fixture_policy_command(&program, &path, false))?;
        }
        files.push(path);
    }
    Ok(files)
}

#[test]
fn native_snapshots_equal_the_helpers_and_tell_policies_apart() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let files = fixtures(&temp.0)?;
    let mut seen = Vec::new();
    for path in &files {
        let native = snapshot(path)?;
        assert_eq!(native, script_snapshot(path)?, "{}", path.display());
        assert_eq!(native, snapshot(path)?, "repeated reads are stable");
        seen.push(native);
    }
    assert_ne!(seen[0], seen[1], "an explicit grant is part of the policy");
    Ok(())
}

#[test]
fn native_copies_equal_the_helpers_and_verify_the_target() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let files = fixtures(&temp.0)?;
    for (i, source) in files.iter().enumerate() {
        let native = temp.0.join(format!("native-{i}"));
        let helper = temp.0.join(format!("helper-{i}"));
        std::fs::write(&native, b"candidate").map_err(|e| e.to_string())?;
        std::fs::write(&helper, b"candidate").map_err(|e| e.to_string())?;
        prepare(source, &native)?;
        script_prepare(source, &helper)?;
        let expected = script_snapshot(source)?;
        assert_eq!(snapshot(&native)?, expected, "{}", source.display());
        assert_eq!(script_snapshot(&native)?, expected);
        assert_eq!(snapshot(&helper)?, expected);
    }
    Ok(())
}

#[test]
fn named_data_streams_are_refused_like_the_helper_refuses_them() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let path = temp.0.join("streams");
    std::fs::write(&path, b"main").map_err(|e| e.to_string())?;
    std::fs::write(temp.0.join("streams:extra"), b"named").map_err(|e| e.to_string())?;
    let named = crate::i18n::text("interaction-standalone-helper-named-streams");
    assert!(snapshot(&path).is_err_and(|e| e.contains(named)));
    assert!(script_snapshot(&path).is_err_and(|e| e.contains(named)));
    let target = temp.0.join("target");
    std::fs::write(&target, b"candidate").map_err(|e| e.to_string())?;
    let before = snapshot(&target)?;
    assert!(prepare(&path, &target).is_err_and(|e| e.contains(named)));
    assert_eq!(snapshot(&target)?, before, "a refused copy changes nothing");
    assert!(snapshot(&temp.0.join("missing")).is_err());
    Ok(())
}
