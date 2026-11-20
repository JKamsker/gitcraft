//! The in-process policy protocol against the reviewed PowerShell helper, on real files.
use super::native_fixtures::{icacls, native, scope_of, write};
use super::*;
use std::path::PathBuf;

const LABEL: u32 = 0x10;

/// Inherited, granted, denied, low-integrity-labelled, and protected under a parent whose
/// inheritable ACE widens access. Each with the folder a copy target should be made in.
fn fixtures(dir: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let wide = dir.join("wide");
    std::fs::create_dir(&wide).map_err(|e| e.to_string())?;
    icacls(&wide, &["/grant", "*S-1-1-0:(OI)(CI)(F)"])?;
    let mut files = Vec::new();
    for (name, args) in [
        ("inherited", &[][..]),
        ("granted ' $() & ü", &["/grant:r", "*S-1-1-0:(R)"][..]),
        ("denied", &["/deny", "*S-1-1-0:(WD)"][..]),
        ("labelled", &["/setintegritylevel", "L"][..]),
    ] {
        let path = dir.join(name);
        write(&path, b"policy fixture")?;
        if !args.is_empty() {
            icacls(&path, args)?;
        }
        files.push((path, dir.to_path_buf()));
    }
    let protected = wide.join("protected");
    write(&protected, b"policy fixture")?;
    icacls(&protected, &["/inheritance:d"])?;
    icacls(&protected, &["/remove:g", "*S-1-1-0"])?;
    files.push((protected, wide));
    Ok(files)
}

#[test]
fn native_snapshots_equal_the_helpers_in_its_scope_and_tell_policies_apart() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let files = fixtures(&temp.0)?;
    let mut seen = Vec::new();
    for (path, _) in &files {
        let helper = script_snapshot(path)?;
        assert_eq!(
            native(path, scope_of(&helper)?)?,
            helper,
            "{}",
            path.display()
        );
        let widest = snapshot(path)?;
        assert_eq!(widest, snapshot(path)?, "repeated reads are stable");
        assert_ne!(
            scope_of(&widest)? & LABEL,
            0,
            "the label is in scope unprivileged"
        );
        seen.push(widest);
    }
    for (i, a) in seen.iter().enumerate() {
        for b in &seen[i + 1..] {
            assert_ne!(a, b, "every fixture policy is distinct");
        }
    }
    Ok(())
}

#[test]
fn a_low_integrity_label_is_captured_and_kept_across_a_copy() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let source = temp.0.join("source");
    write(&source, b"labelled")?;
    let unlabelled = snapshot(&source)?;
    icacls(&source, &["/setintegritylevel", "L"])?;
    let labelled = snapshot(&source)?;
    assert_ne!(labelled, unlabelled, "the label is part of the policy");
    let target = temp.0.join("target");
    write(&target, b"candidate")?;
    assert_ne!(snapshot(&target)?, labelled);
    prepare(&source, &target)?;
    assert_eq!(snapshot(&target)?, labelled);
    // The helper's narrower scope never saw the label; the native copy agrees within it.
    let basic = scope_of(&script_snapshot(&source)?)?;
    assert_eq!(native(&target, basic)?, script_snapshot(&source)?);
    Ok(())
}

#[test]
fn native_copies_reproduce_every_fixture_policy_and_match_the_helpers() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    for (i, (source, folder)) in fixtures(&temp.0)?.iter().enumerate() {
        let native_target = folder.join(format!("native-{i}"));
        let helper_target = folder.join(format!("helper-{i}"));
        write(&native_target, b"candidate")?;
        write(&helper_target, b"candidate")?;
        let expected = snapshot(source)?;
        if i > 0 {
            // Only the plainly inherited fixture already equals a fresh sibling.
            assert_ne!(snapshot(&native_target)?, expected, "{}", source.display());
        }
        if let Err(e) = prepare(source, &native_target) {
            let hex = |b: Vec<u8>| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
            return Err(format!(
                "{}: {e}\nsource {}\ntarget {}",
                source.display(),
                hex(snapshot(source)?),
                hex(snapshot(&native_target)?)
            ));
        }
        assert_eq!(snapshot(&native_target)?, expected, "{}", source.display());
        script_prepare(source, &helper_target)?;
        let helper = script_snapshot(source)?;
        let scope = scope_of(&helper)?;
        assert_eq!(native(&native_target, scope)?, helper);
        assert_eq!(native(&helper_target, scope)?, helper);
    }
    Ok(())
}
