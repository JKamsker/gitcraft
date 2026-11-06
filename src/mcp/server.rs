//! The MCP server: initialize, tools/list and tools/call, as the official SDK runs them over
//! stdio. Each call runs on a blocking thread of its own (git, the forge's tools, the wait for
//! the window), so a slow one never holds up the protocol.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::Value;

use super::tools::{self, Ctx, Kind, ToolDef};

/// What every call is made with.
#[derive(Clone)]
pub struct Gitcraft {
    /// The repository, or why there is none.
    repo: Arc<Result<PathBuf, String>>,
    link_dir: Option<PathBuf>,
    wait: Duration,
}

impl Gitcraft {
    pub fn new(repo: Result<PathBuf, String>, link_dir: Option<PathBuf>, wait: Duration) -> Self {
        Self {
            repo: Arc::new(repo),
            link_dir,
            wait,
        }
    }
}

const INSTRUCTIONS: &str = "gitcraft is the user's git client. Its tools read the repository \
    this server was started in: the commit graph, status, diffs, blame, branches, conflicts, CI \
    builds and pull requests. The tools that change it (stage, commit, branches, checkout, \
    stash, cherry-pick, revert, reset, moving commits) run in the gitcraft window that has the \
    repository open, each as a step the user can undo there with Ctrl+Z. Pushing, deleting a \
    branch, a hard reset and dropping a commit wait for the user to approve them in that \
    window. If no window has the repository open, changing tools fail: ask the user to open it \
    in gitcraft.";

/// The tool as MCP lists it.
fn listed(def: &ToolDef) -> Tool {
    let schema = match (def.schema)() {
        Value::Object(map) => map,
        _ => Default::default(),
    };
    let annotations = ToolAnnotations::with_title(def.title)
        .read_only(def.kind == Kind::Read)
        .destructive(def.kind == Kind::Approve)
        .idempotent(def.kind == Kind::Read)
        // The forge's tools reach beyond the repository; the rest stay in it.
        .open_world(matches!(def.name, "ci_status" | "pull_requests" | "push"));
    Tool::new(def.name, def.description, Arc::new(schema))
        .with_title(def.title)
        .with_annotations(annotations)
}

/// The tools' result as an agent reads it: the JSON answer as text, or why it failed, marked as
/// an error the agent can act on (wrong arguments included: it can call again).
pub fn result(outcome: Result<Value, String>) -> CallToolResult {
    match outcome {
        Ok(value) => {
            let text = serde_json::to_string_pretty(&value).unwrap_or_else(|e| e.to_string());
            CallToolResult::success(vec![ContentBlock::text(text)])
        }
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]),
    }
}

/// Who calls, as the window names them: the client's own title or name, cleaned of anything
/// that isn't plain text, and short.
pub fn agent_name(title: Option<&str>, name: Option<&str>) -> String {
    let raw = title
        .filter(|t| !t.trim().is_empty())
        .or(name)
        .unwrap_or("");
    let known = match raw.to_lowercase().as_str() {
        "claude-code" | "claude code" => Some("Claude Code"),
        n if n.starts_with("codex") => Some("Codex"),
        _ => None,
    };
    if let Some(known) = known {
        return known.to_owned();
    }
    let clean: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .take(48)
        .collect::<String>()
        .trim()
        .to_owned();
    if clean.is_empty() {
        "An agent".to_owned()
    } else {
        clean
    }
}

impl ServerHandler for Gitcraft {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("gitcraft", crate::update::CURRENT))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(
            tools::all().into_iter().map(listed).collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some(def) = tools::find(&request.name) else {
            return Err(ErrorData::invalid_params(
                format!("Unknown tool: {}", request.name),
                None,
            ));
        };
        let agent = context.peer.peer_info().map_or_else(
            || agent_name(None, None),
            |info| {
                agent_name(
                    info.client_info.title.as_deref(),
                    Some(&info.client_info.name),
                )
            },
        );
        let repo = match self.repo.as_ref() {
            Ok(repo) => repo.clone(),
            Err(e) => return Ok(result(Err(e.clone())).into()),
        };
        let ctx = Ctx {
            repo,
            agent,
            link_dir: self.link_dir.clone(),
            wait: self.wait,
        };
        let args = request.arguments.unwrap_or_default();
        let outcome = tokio::task::spawn_blocking(move || tools::call(def, &ctx, &args))
            .await
            .unwrap_or_else(|e| Err(format!("the tool stopped: {e}")));
        Ok(result(outcome).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_are_named_as_people_know_them() {
        assert_eq!(agent_name(None, Some("claude-code")), "Claude Code");
        assert_eq!(agent_name(None, Some("codex-mcp-client")), "Codex");
        assert_eq!(agent_name(Some("Cursor"), Some("cursor-vscode")), "Cursor");
        assert_eq!(agent_name(Some(" "), Some("aider")), "aider");
        assert_eq!(agent_name(None, None), "An agent");
        assert_eq!(agent_name(None, Some("evil\u{7}\nname")), "evilname");
        assert_eq!(agent_name(None, Some(&"x".repeat(100))).len(), 48);
    }

    #[test]
    fn tools_are_listed_with_what_they_do_to_the_repository() {
        let find = |name: &str| tools::find(name).map(listed);
        let log = find("log").and_then(|t| t.annotations);
        assert_eq!(log.as_ref().and_then(|a| a.read_only_hint), Some(true));
        let push = find("push").and_then(|t| t.annotations);
        assert_eq!(push.as_ref().and_then(|a| a.destructive_hint), Some(true));
        let commit = find("commit").and_then(|t| t.annotations);
        assert_eq!(commit.as_ref().and_then(|a| a.read_only_hint), Some(false));
        assert_eq!(
            commit.as_ref().and_then(|a| a.destructive_hint),
            Some(false)
        );
    }

    #[test]
    fn a_failure_is_a_result_the_agent_reads() {
        let failed = result(Err("nope".into()));
        assert_eq!(failed.is_error, Some(true));
        let done = result(Ok(serde_json::json!({"a": 1})));
        assert_ne!(done.is_error, Some(true));
    }
}
