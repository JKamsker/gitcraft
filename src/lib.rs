//! gitcraft: a small, personal visual git client with native local reads and Git CLI fallback.

mod a11y;
mod across;
mod agent;
mod ai;
pub mod app;
mod app_buttons;
mod authorship;
mod avatars;
mod ci;
mod codeowners;
mod collision;
pub mod connections;
mod custom_commands;
mod desktop_entry;
mod diagnostics;
mod drafts;
mod drift;
mod error_card;
mod fence;
mod finder_actions;
mod folder_browser;
mod forge;
mod gerrit;
mod git;
mod glossary;
mod graph;
mod icons;
mod image_cache;
mod image_fit;
mod instance;
mod keymap;
pub mod launch;
mod manual;
pub mod mcp;
mod menu_item;
mod mergiraf;
mod nautilus_scripts;
mod network;
mod os_shell;
mod package;
mod parallel_bisect;
mod portable;
mod pr_post;
mod pr_threads;
mod practice;
mod processes;
mod pull_request;
mod release;
mod remote;
mod repo;
mod safe_read;
mod settings;
mod shell_path;
mod spinner;
pub mod standalone;
mod svg;
mod tab_look;
mod tasks;
mod teleport;
mod terminal;
mod theme;
mod tour;
mod trackers;
mod update;
mod vscode_theme;
mod weakened;

mod audio;
mod connection_doctor;
pub(crate) mod push_preflight;

mod portability;
/// Packaged process endpoint used for durable local operations, before UI startup.
pub fn owned_endpoint_helper() -> Option<i32> {
    match std::env::args_os()
        .nth(1)
        .as_deref()
        .and_then(|v| v.to_str())
    {
        Some("--gitcraft-owned-endpoint") => Some(remote::native_owner::entrypoint()),
        Some("--gitcraft-workspace-claim") => Some(remote::workspace_claim::entrypoint()),
        _ => None,
    }
}
