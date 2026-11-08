//! Paint-only adapters for this partition. Never feed these strings back into control,
//! protocol, cache, storage, command or worker-event identities. Unknown details stay opaque.
use crate::i18n;

#[cfg(test)]
const EXACT: &[(&str, &str)] = &[
    (
        "its program has ended",
        "interaction-terminal-program-ended",
    ),
    (
        "the program running there doesn't take a pasted message: start the agent there first",
        "interaction-terminal-paste-unavailable",
    ),
];

#[cfg(test)]
const DETAIL: &[(&str, &str, &str, &str, &str)] = &[
    (
        "could not open a terminal: ",
        "",
        "interaction-terminal-open-failed",
        "detail",
        "could not open a terminal: {detail}",
    ),
    (
        "terminal: ",
        "",
        "interaction-terminal-io-failed",
        "detail",
        "terminal: {detail}",
    ),
];

/// Only pass application-generated diagnostics from this partition, at paint time.
/// stdout/stderr, custom-command output and arbitrary provider text must bypass this API.
/// Nested detail captures are intentionally opaque; no recursive search/replacement.
#[cfg(test)]
pub(crate) fn diagnostic(raw: &str) -> String {
    let exact = raw.strip_suffix('\n').unwrap_or(raw);
    let shown = raw;
    if let Some((_, id)) = EXACT.iter().find(|(source, _)| *source == exact) {
        return i18n::text(id).to_owned();
    }
    if let Some(rest) = shown.strip_prefix("could not start ")
        && let Some((program, detail)) = rest.split_once(": ")
    {
        return i18n::message(
            "interaction-terminal-start-failed",
            &[("program", program.into()), ("detail", detail.into())],
        );
    }
    for &(prefix, suffix, id, name, _) in DETAIL {
        if let Some(value) = shown
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(suffix))
        {
            return i18n::message(id, &[(name, value.into())]);
        }
    }
    raw.to_owned()
}

/// Resolve only built-in shell labels; keep user-selected program and arguments raw.
#[cfg(test)]
pub(crate) fn shell_label(raw: &str) -> &str {
    match raw {
        "Windows PowerShell" => i18n::text("interaction-terminal-shell-powershell"),
        "PowerShell 7" => i18n::text("interaction-terminal-shell-powershell7"),
        "Command Prompt" => i18n::text("interaction-terminal-shell-command-prompt"),
        "Git Bash" => i18n::text("interaction-terminal-shell-git-bash"),
        "WSL" => i18n::text("interaction-terminal-shell-wsl"),
        "bash" => i18n::text("interaction-terminal-shell-bash"),
        "zsh" => i18n::text("interaction-terminal-shell-zsh"),
        "fish" => i18n::text("interaction-terminal-shell-fish"),
        "sh" => i18n::text("interaction-terminal-shell-sh"),
        _ => raw,
    }
}

pub(super) fn scrollback(count: usize) -> String {
    i18n::message(
        "interaction-terminal-scrollback",
        &[("count", count.into())],
    )
}

pub(super) fn accessibility() -> &'static str {
    i18n::text("interaction-terminal-accessibility")
}

#[cfg(test)]
mod execution_fluent_tests {
    use super::*;

    #[test]
    fn supplied_terminal_presentation_keeps_programs_payloads_and_protocol_raw() {
        for (locale, single, multiple, command_prompt) in [
            ("en-US", "↑ 1 line", "↑ 2,345 lines", "Command Prompt"),
            ("de", "↑ 1 Zeile", "↑ 2.345 Zeilen", "Eingabeaufforderung"),
        ] {
            i18n::set_language(locale);
            assert_eq!(scrollback(1), single);
            assert_eq!(scrollback(2345), multiple);
            assert_eq!(shell_label("Command Prompt"), command_prompt);
            assert_eq!(
                shell_label("my shell $HOME --flag"),
                "my shell $HOME --flag"
            );
            assert!(
                accessibility().contains("Shift+Escape")
                    || accessibility().contains("Umschalt+Escape")
            );
            let detail = "provider: could not start xyz: $HOME {count}\n\x1b[31m";
            assert!(diagnostic(&format!("terminal: {detail}")).contains(detail));
            assert!(
                diagnostic(&format!("could not start C:\\tools\\sh.exe: {detail}"))
                    .contains("C:\\tools\\sh.exe")
            );
            assert!(
                diagnostic(&format!("could not start C:\\tools\\sh.exe: {detail}"))
                    .contains(detail)
            );
            assert_eq!(diagnostic(detail), detail);
        }
        i18n::set_language("de");
        assert!(diagnostic("could not open a terminal: native error").starts_with("Ein Terminal"));
        assert!(diagnostic("its program has ended").starts_with("Das Programm"));
        // Supplied ShellSpec only: no discovery, spawn, native terminal or drop.
        let shell = super::super::ShellSpec {
            label: "Command Prompt".into(),
            program: "cmd.exe".into(),
            args: vec!["/D".into()],
        };
        assert_eq!(shell_label(&shell.label), "Eingabeaufforderung");
        assert_eq!(shell.label, "Command Prompt");
        assert_eq!(shell.program, "cmd.exe");
        assert_eq!(shell.args, ["/D"]);
        i18n::set_language("en-US");
    }
}
