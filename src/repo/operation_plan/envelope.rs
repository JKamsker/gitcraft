use super::*;

pub(in crate::repo) fn check_command_envelopes(
    ops: &[Vec<String>],
    saved_redo: bool,
    read: &mut dyn FnMut(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    let mut commands = Vec::new();
    collect_command_envelopes(ops, saved_redo, &mut commands)?;
    if saved_redo && !commands.is_empty() {
        let names = read(&["--list-cmds=builtins"])?;
        if commands
            .iter()
            .any(|command| !names.lines().any(|name| name == command))
        {
            return Err("Saved Git aliases or external commands cannot be replayed safely. Run the operation again explicitly.".into());
        }
    }
    Ok(())
}

/// Validate the entire envelope before even read-only repository preparation.
pub(in crate::repo) fn collect_command_envelopes(
    ops: &[Vec<String>],
    saved_redo: bool,
    commands: &mut Vec<String>,
) -> Result<(), String> {
    for op in ops {
        if let [mark, _, args @ ..] = op.as_slice()
            && mark == TRY
        {
            if command_envelope(args)
                .is_ok_and(|(_, command)| matches!(command, "pull" | "submodule" | "stash"))
            {
                return Err("Pull, stash and submodule operations require checked dispatch; run the operation again explicitly.".into());
            }
            collect_command_envelopes(&[args.to_vec()], saved_redo, commands)?;
            continue;
        }
        if is_expectation(op) || op.first().is_some_and(|word| word.starts_with('\0')) {
            continue; // Application-owned expectations and try-step envelopes.
        }
        let envelope = command_envelope(op);
        let (at, command) = match envelope {
            Ok(value) => value,
            Err(error) if saved_redo => return Err(error),
            Err(_) => continue, // Git diagnoses unsupported normal commands.
        };
        if command == "stash" && at != 0 {
            return Err("Stash commands with global options cannot be validated safely. Run Stash again from the toolbar.".into());
        }
        if command == "pull" && at != 0 {
            return Err("Saved pull commands with global options cannot be validated safely. Run Pull again from the toolbar.".into());
        }
        if command == "submodule"
            && (op.get(at + 1).is_some_and(|word| word.starts_with('-'))
                || (at != 0 && op.get(at + 1).is_some_and(|word| word == "update")))
        {
            return Err("Submodule updates with global or command-prefix options cannot be validated safely. Run Update submodules again.".into());
        }
        commands.push(command.to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod grammar_tests {
    use super::*;

    #[test]
    fn option_values_are_not_remote_subcommands() -> Result<(), String> {
        for argv in [
            &["--git-dir", "/tmp/r.git", "fetch"][..],
            &["--git-dir=/tmp/r.git", "fetch"],
            &["--work-tree", "push", "fetch"],
            &["-c", "alias.x=push", "fetch"],
            &["-Celsewhere", "fetch"],
        ] {
            let op = ops(&[argv]).remove(0);
            assert_eq!(
                command_envelope(&op)
                    .map_err(|error| format!("{error:?}"))?
                    .1,
                "fetch"
            );
            assert!(reaches_a_remote(&op));
        }
        for argv in [
            &["--git-dir", "fetch", "status"][..],
            &["--git-dir"],
            &["--unknown", "fetch"],
            &[EXPECT, "HEAD", "push"],
        ] {
            let op = ops(&[argv]).remove(0);
            assert!(!reaches_a_remote(&op));
        }
        Ok(())
    }
}

#[cfg(test)]
mod network_policy_tests {
    use super::*;
    #[test]
    fn shared_syntax_keeps_network_policy_and_internal_step_dispatch_distinct() -> Result<(), String>
    {
        for argv in [
            &["remote", "add", "origin", "https://example.invalid/r"][..],
            &["remote", "show", "-n", "origin"],
            &[TRY, "optional", "fetch"],
            &[EXPECT, "HEAD", "push"],
            &["--git-dir", "push", "status"],
        ] {
            assert!(!reaches_a_remote(&ops(&[argv])[0]));
        }
        for argv in [
            &["remote", "show", "origin"][..],
            &["remote", "update"],
            &["lfs", "pull"],
            &[crate::push_preflight::APPROVED, "registered-plan"],
        ] {
            assert!(reaches_a_remote(&ops(&[argv])[0]));
        }
        Ok(())
    }
}
