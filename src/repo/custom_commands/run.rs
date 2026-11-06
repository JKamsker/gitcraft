//! Running a custom command: in the background with its output in a toast, or in a terminal
//! tab of its own. Either way on the repository's machine.

use crate::custom_commands::{self as cc, CustomCommand, OutputTo, Scope};
use crate::repo::RepoTab;
use crate::terminal::{ShellSpec, Terminal};

impl RepoTab {
    /// Runs `command` about `scope`, `answers` being its prompts' answers in their order.
    pub(super) fn run_custom(
        &mut self,
        command: &CustomCommand,
        scope: &Scope,
        answers: &[String],
        ctx: &egui::Context,
    ) {
        if let Some(problem) = command.problem() {
            self.status = Some((format!("{}: {problem}", command.name.trim()), true));
            return;
        }
        let named: Vec<(String, String)> = command
            .prompts
            .iter()
            .zip(answers.iter().chain(std::iter::repeat(&String::new())))
            .map(|(p, a)| (p.variable.clone(), a.clone()))
            .collect();
        if let Some((name, _)) = named.iter().find(|(_, a)| a.contains('\0')) {
            self.status = Some((format!("${name} can't hold a NUL character"), true));
            return;
        }
        let repo = crate::remote::git_path(&self.path);
        let vars = cc::variables(scope, &repo, &named);
        let name = command.name.trim().to_owned();
        match command.output {
            OutputTo::Toast => self.run_custom_captured(name, &vars, &command.command, ctx),
            OutputTo::Terminal => {
                let script = cc::terminal_program(&vars, &command.command, &name);
                self.run_custom_in_terminal(&name, &script, ctx);
            }
        }
    }

    /// Runs the command in the background; a toast says how it ended, with its last lines.
    fn run_custom_captured(
        &mut self,
        name: String,
        vars: &[(String, String)],
        command: &str,
        ctx: &egui::Context,
    ) {
        let script = cc::captured_program(vars, command);
        let repo = self.path.clone();
        self.finished.push((format!("Running {name}…"), false));
        self.then(
            ctx,
            move || crate::git::run_shell(&repo, &script),
            move |tab, ran, _| {
                let toast = match ran {
                    Ok(out) => cc::toast_text(&name, out.code, &out.stdout, &out.stderr),
                    Err(e) => (format!("{name} couldn't be run: {e}"), true),
                };
                tab.finished.push(toast);
            },
        );
    }

    /// Opens a terminal tab that runs the script, there or on the repository's machine.
    fn run_custom_in_terminal(&mut self, name: &str, script: &str, ctx: &egui::Context) {
        let (spec, cwd) = match crate::remote::parse(&self.path) {
            Some(target) => {
                let (program, args, _) = crate::remote::terminal_running(&target, script);
                (shell_spec(name, program, args), std::env::temp_dir())
            }
            None => {
                let (program, args) = crate::git::shell_command_here(&self.path, script);
                (shell_spec(name, program, args), self.path.clone())
            }
        };
        match Terminal::spawn(&cwd, &spec, ctx) {
            Ok(term) => {
                self.terminals.push(term);
                self.active_terminal = self.terminals.len() - 1;
                self.terminal_error = None;
            }
            Err(e) => self.terminal_error = Some(format!("{name} couldn't be started: {e}")),
        }
        self.terminal_open = true;
    }
}

fn shell_spec(name: &str, program: String, args: Vec<String>) -> ShellSpec {
    ShellSpec {
        label: name.to_owned(),
        program,
        args,
    }
}
