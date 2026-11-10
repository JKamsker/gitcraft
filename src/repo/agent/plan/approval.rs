//! Approval identity and presentation, with raw equality retained.

mod presentation;

/// What the approval card says.
#[derive(Clone, Debug)]
pub(crate) struct Ask {
    /// What and on what: "Delete branch feature/x".
    pub title: String,
    /// Exactly what will happen, in words.
    pub body: String,
    /// What undo can and can't do about it afterwards, said once under the commands.
    pub undo_note: String,
    pub(in crate::repo::agent) presentation: Option<AskReceipt>,
}

pub(super) const RAW_NO_UNDO: &str = "This can't be undone from gitcraft.";

/// Facts retained only by the seven existing application-owned confirmation producers.
#[derive(Clone, Debug)]
pub(in crate::repo::agent) enum AskPresentation {
    Stage {
        conflicted: Vec<String>,
    },
    HardReset {
        target: String,
        from: String,
        to: String,
        subject: String,
        lost: usize,
        left: usize,
    },
    MixedReset {
        target: String,
        from: String,
        to: String,
        subject: String,
        files: usize,
    },
    Drop {
        commit: String,
        subject: String,
        branch: String,
        after: usize,
    },
    DeleteBranch {
        name: String,
        commit: String,
        subject: String,
        force: bool,
    },
    Push {
        branch: String,
        remote: String,
        there: String,
        publish: bool,
        force: bool,
        ahead: u32,
        behind: u32,
    },
    Fallback {
        label: String,
    },
}

#[derive(Clone, Debug)]
pub(in crate::repo::agent) struct AskReceipt {
    kind: AskPresentation,
    original: (String, String, String),
    source_suffix: String,
}

impl AskPresentation {
    pub(super) fn display(&self) -> Ask {
        presentation::display(self)
    }

    pub(in crate::repo::agent::plan) fn raw(&self) -> Ask {
        match self.clone() {
            Self::Stage { conflicted } => Ask {
                title: "Mark conflicts resolved".to_owned(),
                undo_note: RAW_NO_UNDO.to_owned(),
                body: format!(
                    "Stage {} as resolved: {}. git then forgets the conflict and its three \
                     sides.",
                    if conflicted.len() == 1 {
                        "this conflicted file"
                    } else {
                        "these conflicted files"
                    },
                    conflicted.join(", ")
                ),
                presentation: None,
            },
            Self::HardReset {
                target,
                from,
                to,
                subject,
                lost,
                left,
            } => {
                let lost = match lost {
                    0 => "There are no uncommitted changes to tracked files now.".to_owned(),
                    1 => "The uncommitted changes to 1 tracked file are thrown away for good."
                        .to_owned(),
                    n => format!(
                        "The uncommitted changes to {n} tracked files are thrown away for good."
                    ),
                };
                let left = match left {
                    0 => String::new(),
                    1 => format!(" The commit after {} is no longer on {target}.", to.clone()),
                    n => format!(
                        " The {n} commits after {} are no longer on {target}.",
                        to.clone()
                    ),
                };
                Ask {
                    title: format!("Hard reset {target} to {}", to.clone()),
                    body: format!(
                        "Move {target} from {} to {} (“{}”) and make the working tree match it. \
             {lost}{left} Untracked files stay.",
                        from.clone(),
                        to.clone(),
                        subject.clone()
                    ),
                    undo_note: format!(
                        "Ctrl+Z moves {target} back afterwards; the thrown-away changes stay lost."
                    ),
                    presentation: None,
                }
            }
            Self::MixedReset {
                target,
                from,
                to,
                subject,
                files,
            } => Ask {
                title: format!("Reset {target} to {} (mixed)", to.clone()),
                body: format!(
                    "Move {target} from {} to {} (“{}”) and unstage everything: the {files} staged \
             file{} keep their changes, unstaged. The working tree stays as it is.",
                    from.clone(),
                    to.clone(),
                    subject.clone(),
                    if files == 1 { "" } else { "s" },
                ),
                undo_note: format!(
                    "Ctrl+Z moves {target} back afterwards, but doesn't stage those changes again."
                ),
                presentation: None,
            },
            Self::Drop {
                commit,
                subject,
                branch,
                after,
            } => Ask {
                title: format!("Drop commit {}", commit.clone()),
                body: format!(
                    "Remove {} (“{}”) and its changes from {branch}, and replay the {after} commit{} \
             after it. If a later commit conflicts, the rebase stops for you to finish or abort.",
                    commit.clone(),
                    subject.clone(),
                    if after == 1 { "" } else { "s" },
                ),
                undo_note: format!("Ctrl+Z puts {branch} back as it is now."),
                presentation: None,
            },
            Self::DeleteBranch {
                name,
                commit,
                subject,
                force,
            } => {
                let merged_note = if force {
                    " even if it isn't merged"
                } else {
                    " (git refuses if it isn't merged)"
                };
                Ask {
                    title: format!("Delete branch {name}"),
                    body: format!(
                        "Delete the local branch “{name}”, at {} (“{}”),{merged_note}. Its copy on a \
                 remote stays.",
                        commit.clone(),
                        subject.clone()
                    ),
                    undo_note: "Ctrl+Z makes it again at that commit.".to_owned(),
                    presentation: None,
                }
            }
            Self::Push {
                branch,
                remote,
                there,
                publish,
                force,
                ahead,
                behind,
            } => {
                let to = format!("{remote}/{there}");
                let plural = |n: u32| if n == 1 { "" } else { "s" };
                let (title, body) = if publish {
                    (
                        format!("Publish {branch} on {remote}"),
                        format!("Send {branch} to {remote} as a new branch {there}, and track it."),
                    )
                } else if force {
                    let gone = match behind {
                        0 => format!(
                            "{to} has no commits that {branch} lacks, as of the last fetch."
                        ),
                        n => format!(
                            "The {n} commit{} on {to} that {branch} doesn't have are removed from it, for \
                 everyone who uses it.",
                            plural(n)
                        ),
                    };
                    (
                        format!("Force push {branch} to {to}"),
                        format!(
                            "Overwrite {to} with {branch}. {gone} git refuses if {to} has changed since \
                 it was last fetched."
                        ),
                    )
                } else {
                    (
                        format!("Push {branch} to {to}"),
                        format!(
                            "Send {branch}'s {ahead} new commit{} to {to}.",
                            plural(ahead)
                        ),
                    )
                };
                Ask {
                    title,
                    body,
                    undo_note: RAW_NO_UNDO.to_owned(),
                    presentation: None,
                }
            }
            Self::Fallback { label } => Ask {
                title: label.clone(),
                body: label,
                undo_note: RAW_NO_UNDO.to_owned(),
                presentation: None,
            },
        }
    }
}

impl PartialEq for Ask {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title && self.body == other.body && self.undo_note == other.undo_note
    }
}
impl Eq for Ask {}

impl Ask {
    #[cfg(test)]
    pub(crate) fn opaque(title: &str, body: &str, undo_note: &str) -> Self {
        Self {
            title: title.to_owned(),
            body: body.to_owned(),
            undo_note: undo_note.to_owned(),
            presentation: None,
        }
    }

    pub(in crate::repo::agent) fn from_presentation(kind: AskPresentation) -> Self {
        let mut ask = kind.raw();
        ask.presentation = Some(AskReceipt {
            original: (ask.title.clone(), ask.body.clone(), ask.undo_note.clone()),
            kind,
            source_suffix: String::new(),
        });
        ask
    }
    pub(in crate::repo::agent) fn append_source_suffix(&mut self, suffix: &str) {
        self.body.push_str(suffix);
        if let Some(receipt) = &mut self.presentation {
            receipt.source_suffix.push_str(suffix);
        }
    }
    pub(in crate::repo::agent) fn for_display(&self) -> Self {
        let Some(receipt) = &self.presentation else {
            return self.clone();
        };
        if self.title != receipt.original.0
            || self.undo_note != receipt.original.2
            || self.body != format!("{}{}", receipt.original.1, receipt.source_suffix)
        {
            return self.clone();
        }
        let mut shown = receipt.kind.display();
        shown.body.push_str(&receipt.source_suffix);
        shown
    }
}

/// Said under a card's commands for what can't be undone from gitcraft.
pub(super) const NO_UNDO: &str = "repo-workflows-agent--14b0fc42";
