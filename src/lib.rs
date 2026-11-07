//! gitcraft: a small, personal visual git client (egui front end over the git command line).

pub mod app;
mod avatars;
mod bounded_process;
mod ci;
mod connections;
mod desktop_entry;
mod folder_browser;
mod forge;
mod gerrit;
mod git;
mod glossary;
mod graph;
mod icons;
mod image_fit;
mod manual;
mod mergiraf;
mod pr_post;
mod pr_threads;
mod practice;
mod pull_request;
mod remote;
mod repo;
mod safe_read;
mod settings;
mod shell_path;
mod spinner;
pub mod standalone;
mod tab_look;
mod terminal;
mod theme;
mod update;
mod vscode_theme;

pub(crate) mod tour;

pub(crate) mod portability;
