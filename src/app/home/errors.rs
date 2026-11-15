//! Retained Home error presentation.
use std::path::{Path, PathBuf};
/// A retained Home error owns both its original English envelope and, only when supplied by
/// the actual source, the finite arguments needed to paint it in the current language.
/// Plain replacement always discards presentation metadata, even for identical raw text.
#[derive(Clone, Debug)]
pub(in crate::app) struct AppError {
    raw: String,
    presentation: Option<HomeError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HomeError {
    Init(String),
    Folder {
        folder: PathBuf,
        error: String,
        not_repo: bool,
    },
    Unsupported(String),
    PatchTarget {
        has_repo: bool,
        name: String,
    },
}

impl From<String> for AppError {
    fn from(raw: String) -> Self {
        Self {
            raw,
            presentation: None,
        }
    }
}

impl std::ops::Deref for AppError {
    type Target = str;

    fn deref(&self) -> &str {
        &self.raw
    }
}

impl AppError {
    fn sourced(source: HomeError) -> Self {
        Self {
            raw: source.raw(),
            presentation: Some(source),
        }
    }

    pub(in crate::app) fn init(error: String) -> Self {
        Self::sourced(HomeError::Init(error))
    }

    pub(in crate::app) fn folder(folder: &Path, error: &str, not_repo: bool) -> Self {
        Self::sourced(HomeError::Folder {
            folder: folder.to_owned(),
            error: error.to_owned(),
            not_repo,
        })
    }

    pub(in crate::app) fn unsupported(name: String) -> Self {
        Self::sourced(HomeError::Unsupported(name))
    }

    pub(in crate::app) fn patch_target(has_repo: bool, name: &str) -> Self {
        Self::sourced(HomeError::PatchTarget {
            has_repo,
            name: name.to_owned(),
        })
    }

    /// Historical toasts keep their emission language. Ordinary errors retain their original
    /// raw toast transport and its existing finite paint fallback.
    pub(in crate::app) fn delivery_text(&self) -> String {
        match &self.presentation {
            Some(source) if self.raw == source.raw() => source.display(),
            _ => self.raw.clone(),
        }
    }

    pub(super) fn display(&self) -> String {
        match &self.presentation {
            Some(source) if self.raw == source.raw() => source.display(),
            Some(_) => self.raw.clone(),
            None => super::super::panels::display_app_message(&self.raw),
        }
    }
}

impl HomeError {
    fn raw(&self) -> String {
        match self {
            Self::Init(error) => format!("git init failed: {error}"),
            Self::Folder {
                folder,
                error,
                not_repo,
            } => {
                let name = super::super::dropped::name_of(folder);
                if *not_repo {
                    format!(
                        "{name} isn't in a git repository: drop a repository's folder to open it"
                    )
                } else {
                    format!("Could not open {name}: {error}")
                }
            }
            Self::Unsupported(name) => format!(
                "Can't open {name}: drop a folder to open it, or a .patch, .diff, .eml or .mbox file to apply it"
            ),
            Self::PatchTarget {
                has_repo: true,
                name,
            } => format!("To apply {name}, show its repository's tab, then drop the patch there"),
            Self::PatchTarget {
                has_repo: false,
                name,
            } => format!(
                "Applying {name} needs a repository open first: open one, then drop the patch on it"
            ),
        }
    }

    pub(super) fn display(&self) -> String {
        match self {
            Self::Init(error) => crate::i18n::message(
                "interaction-home-init-error",
                &[("error", error.as_str().into())],
            ),
            Self::Folder {
                folder,
                error,
                not_repo,
            } => {
                let name = super::super::dropped::name_of(folder);
                if *not_repo {
                    crate::i18n::message(
                        "interaction-drop-folder-not-repo",
                        &[("name", name.into())],
                    )
                } else {
                    crate::i18n::message(
                        "interaction-drop-folder-error",
                        &[
                            ("name", name.into()),
                            ("error", crate::git::display_error(error).into()),
                        ],
                    )
                }
            }
            Self::Unsupported(name) => crate::i18n::message(
                "interaction-drop-unsupported",
                &[("name", name.as_str().into())],
            ),
            Self::PatchTarget { has_repo, name } => crate::i18n::message(
                if *has_repo {
                    "interaction-drop-show-repo"
                } else {
                    "interaction-drop-open-repo"
                },
                &[("name", name.as_str().into())],
            ),
        }
    }
}

/// The actual Home error painter, also usable with supplied inert state in headless tests.
pub(super) fn error_label(ui: &mut egui::Ui, color: egui::Color32, error: &AppError) {
    ui.colored_label(color, error.display());
}

#[cfg(test)]
#[path = "retained_error_tests.rs"]
mod retained_error_tests;
