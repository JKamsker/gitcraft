//! The tools `gitcraft mcp` offers: what each is called, what it takes and does, and running
//! one. Reading tools run git here, through the same command layer as the window (so a
//! repository in WSL or over SSH is read there). Changing tools hand the operation to the
//! window that has the repository open ([`write`]).

mod attribution;
mod forge;
mod history;
mod refs;
mod work;
pub(crate) mod write;

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Map, Value, json};

use super::args::Args;

/// What a tool needs to run: the repository, who asks, and how to reach the window.
pub struct Ctx {
    pub repo: PathBuf,
    /// The agent's name, as the window shows it.
    pub agent: String,
    /// Where the windows' discovery files are (see `link::dir`); `None` when there is nowhere.
    pub link_dir: Option<PathBuf>,
    /// How long a changing tool waits for the window's answer.
    pub wait: Duration,
    /// Whether the call was cancelled (or the session ended): a wait for the window stops.
    pub cancelled: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
}

/// What a tool does to the repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Only reads.
    Read,
    /// Changes it, in the window, as a step that can be undone there.
    Write,
    /// Changes it in a way the user approves first, in the window.
    Approve,
}

/// One tool.
pub struct ToolDef {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    /// The JSON Schema of its arguments.
    pub schema: fn() -> Value,
    pub run: fn(&Ctx, &Args) -> Result<Value, String>,
}

/// Every tool, reading ones first.
pub fn all() -> Vec<&'static ToolDef> {
    [
        history::TOOLS,
        attribution::TOOLS,
        work::TOOLS,
        refs::TOOLS,
        forge::TOOLS,
        write::TOOLS,
    ]
    .concat()
}

pub fn find(name: &str) -> Option<&'static ToolDef> {
    all().into_iter().find(|t| t.name == name)
}

/// Runs `tool` with `args`: its result, or why it failed (also when the arguments are wrong:
/// the agent reads the reason and can call again).
pub fn call(tool: &ToolDef, ctx: &Ctx, args: &Map<String, Value>) -> Result<Value, String> {
    (tool.run)(ctx, &Args::new(args))
}

/// An object schema with `properties`, of which `required` must be given, and no others.
pub(super) fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

/// The schema of a whole-number argument.
pub(super) fn number(description: &str, default: u64, min: u64, max: u64) -> Value {
    json!({
        "type": "integer",
        "description": description,
        "default": default,
        "minimum": min,
        "maximum": max,
    })
}

/// The schema of an id or number that has no default.
pub(super) fn id(description: &str) -> Value {
    json!({ "type": "integer", "description": description, "minimum": 1 })
}

pub(super) fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

pub(super) fn boolean(description: &str, default: bool) -> Value {
    json!({ "type": "boolean", "description": description, "default": default })
}

/// The first 8 characters of a hash, as gitcraft shows it.
pub(super) fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_unique_name_and_an_object_schema() {
        let tools = all();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), tools.len());
        for tool in tools {
            // The names MCP allows: letters, digits, `_`, `-`, `.`.
            assert!(
                tool.name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')),
                "{}",
                tool.name
            );
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
            for required in schema["required"].as_array().into_iter().flatten() {
                let name = required.as_str().unwrap_or_default();
                assert!(schema["properties"].get(name).is_some(), "{}", tool.name);
            }
            assert!(!tool.description.is_empty() && !tool.title.is_empty());
        }
    }

    #[test]
    fn the_changing_tools_say_how_they_run() {
        for tool in all() {
            match tool.kind {
                Kind::Read => {}
                Kind::Write => assert!(tool.description.contains("undo"), "{}", tool.name),
                Kind::Approve => assert!(tool.description.contains("approve"), "{}", tool.name),
            }
        }
    }
}
