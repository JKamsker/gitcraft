use super::*;

/// Why an agent that ended with `code` and said `stderr` gave nothing, as the user reads it.
pub(super) fn failure(engine: Engine, code: Option<i32>, stderr: &str) -> String {
    let program = match engine {
        Engine::Claude => "claude",
        Engine::Codex => "codex",
    };
    match code {
        Some(127 | 9009) => format!(
            "{program} isn't installed here (or isn't on the PATH gitcraft's commands get). \
             Install {}, or pick the other agent in Settings → AI.",
            engine.label()
        ),
        _ => {
            let said: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
            let tail = said[said.len().saturating_sub(4)..].join(" ");
            let how =
                code.map_or_else(|| "stopped".to_owned(), |c| format!("exited with code {c}"));
            if tail.is_empty() {
                format!("{} {how}", engine.label())
            } else {
                format!(
                    "{} {how}: {}",
                    engine.label(),
                    crate::resolver::report::clean(&tail, 400)
                )
            }
        }
    }
}
