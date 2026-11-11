//! Cached doctor confirmation presentation.

use super::*;

#[derive(Clone)]
pub(super) struct ConfirmPresentation {
    pub(super) raw: (String, String, String),
    pub(super) kind: ConfirmKind,
}
#[derive(Clone)]
pub(super) enum ConfirmKind {
    Remove {
        key: String,
        value: String,
        file: String,
        scope: crate::git::doctor::config::Scope,
    },
    Set {
        key: String,
        value: String,
        scope: crate::git::doctor::config::Scope,
    },
    Clear {
        paths: Vec<String>,
    },
    Maintenance {
        commands: String,
    },
}
impl Confirm {
    pub(super) fn with_presentation(mut self, kind: ConfirmKind) -> Self {
        self.presentation = Some(Box::new(ConfirmPresentation {
            raw: (self.title.clone(), self.body.clone(), self.button.clone()),
            kind,
        }));
        self
    }
    pub(super) fn displayed(&self) -> (String, String, String) {
        let raw = (self.title.clone(), self.body.clone(), self.button.clone());
        let Some(receipt) = &self.presentation else {
            return raw;
        };
        if receipt.raw != raw {
            return raw;
        }
        use crate::git::doctor::config::Scope;
        use crate::i18n::{message, text};
        match &receipt.kind {
            ConfirmKind::Remove {
                key,
                value,
                file,
                scope,
            } => (
                message(
                    "repo-workflows-doctor-remove-confirm-9861b6eb",
                    &[("origin-key-1", key.as_str().into())],
                ),
                message(
                    "repo-workflows-doctor-remove-setting",
                    &[
                        ("key", key.as_str().into()),
                        ("value", value.as_str().into()),
                        ("file", file.as_str().into()),
                        (
                            "scope",
                            match scope {
                                Scope::Global => "global",
                                Scope::System => "system",
                                _ => "local",
                            }
                            .into(),
                        ),
                    ],
                ),
                text("repo-workflows-doctor-remove-confirm-c3812fc4").into(),
            ),
            ConfirmKind::Set { key, value, scope } => (
                message(
                    "repo-workflows-doctor-set-row-a215cc6f",
                    &[
                        ("setting-key-1", key.as_str().into()),
                        (
                            "scope-choice-scope-to-lowercase-2",
                            config_section::scope_choice(*scope).to_lowercase().into(),
                        ),
                    ],
                ),
                message(
                    "repo-workflows-doctor-set-row-ccc71902",
                    &[
                        ("setting-key-1", key.as_str().into()),
                        ("value", value.as_str().into()),
                        (
                            "if-scope-scope-global-your-globa-2",
                            text(if *scope == Scope::Global {
                                "repo-workflows-doctor-set-row-e0a790f9"
                            } else {
                                "repo-workflows-doctor-set-row-cc3edd84"
                            })
                            .into(),
                        ),
                    ],
                ),
                text("repo-workflows-doctor-set-row-b6f6f3ad").into(),
            ),
            ConfirmKind::Clear { paths } => {
                let what = match paths.as_slice() {
                    [one] => one.clone(),
                    _ => message(
                        "repo-workflows-doctor-clear-ac70d25c",
                        &[("files-len-1", paths.len().into())],
                    ),
                };
                (
                    text("repo-workflows-doctor-clear-909d7fba").into(),
                    message(
                        "repo-workflows-doctor-clear-ae587a37",
                        &[("what", what.into())],
                    ),
                    text("repo-workflows-doctor-clear-83b12c22").into(),
                )
            }
            ConfirmKind::Maintenance { commands } => (
                text("repo-workflows-doctor-turn-on-action-3c97132f").into(),
                message(
                    "repo-workflows-doctor-turn-on-action-7cde0a5e",
                    &[("commands-join-1", commands.as_str().into())],
                ),
                text("repo-workflows-doctor-turn-on-action-f4830a1d").into(),
            ),
        }
    }
}
