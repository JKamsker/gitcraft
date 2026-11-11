//! Cutting a release from a commit: the version the Conventional Commits since the last tag call
//! for, notes grouped by type, the version moved in the manifests (and a changelog), an
//! annotated (or signed) tag, the push, and the release on the forge with its files. Each step
//! is done and recorded on its own, so that a release that failed halfway can go on from where
//! it stopped.

pub(crate) mod conventional;
mod display;
pub(crate) use display::{display_error, display_problem};
pub(crate) mod manifest;
pub(crate) mod notes;
#[cfg(test)]
mod tests;
pub(crate) mod version;

use std::path::{Path, PathBuf};

use crate::forge::{Forge, Hosting};
use crate::git;
use conventional::Change;
use manifest::Kind;
use version::Version;

/// What a release starts from, read from the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Draft {
    /// The commit released.
    pub commit: String,
    /// The last tag before it, if any.
    pub last_tag: Option<String>,
    pub changes: Vec<Change>,
    /// The version suggested.
    pub suggested: Version,
    /// The branch checked out, when the commit is its tip: only then can a commit move the
    /// version in the manifests.
    pub branch: Option<String>,
    /// The manifests at the commit and their version, those that can be moved (clean here).
    pub manifests: Vec<(Kind, String)>,
    /// Manifests that can't be moved now, and why.
    pub manifest_note: Option<String>,
    /// `CHANGELOG.md` is there.
    pub changelog: bool,
    /// git signs tags by default (`tag.gpgSign`).
    pub sign: bool,
    /// The remote to push to.
    pub remote: Option<String>,
    /// Where on it the branch goes (`refs/heads/main`).
    pub remote_ref: Option<String>,
    /// The tags that exist, to refuse one again.
    pub tags: Vec<String>,
}

/// The file the changelog is in.
pub(crate) const CHANGELOG: &str = "CHANGELOG.md";

mod draft;
pub(crate) use draft::*;

/// What the release is to be, as chosen in its dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub commit: String,
    pub tag: String,
    pub notes: String,
    /// The branch the version commit goes on, and the manifests it moves to `version`.
    pub bump: Option<Bump>,
    pub sign: bool,
    /// The remote to push the tag (and the version commit) to.
    pub push: Option<String>,
    /// Make the release on the forge.
    pub publish: bool,
    pub prerelease: bool,
    /// Files attached to the forge's release.
    pub assets: Vec<PathBuf>,
}

/// The commit that moves the version in the files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bump {
    pub branch: String,
    /// The branch on the remote the version commit is pushed to (`refs/heads/main`): the
    /// branch's upstream there, else the branch of its name.
    pub remote_ref: String,
    /// The version written, without its prefix.
    pub version: String,
    pub manifests: Vec<(Kind, String)>,
    pub changelog: bool,
}

/// One step of a release, done on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Bump,
    Tag,
    Push,
    Publish,
    Upload,
}

impl Step {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Step::Bump => "Move the version",
            Step::Tag => "Tag",
            Step::Push => "Push",
            Step::Publish => "Make the release on the forge",
            Step::Upload => "Attach the files",
        }
    }
}

impl Plan {
    /// The steps, in order.
    pub(crate) fn steps(&self) -> Vec<Step> {
        let mut steps = Vec::new();
        if self.bump.is_some() {
            steps.push(Step::Bump);
        }
        steps.push(Step::Tag);
        if self.push.is_some() {
            steps.push(Step::Push);
        }
        if self.publish {
            steps.push(Step::Publish);
            if !self.assets.is_empty() {
                steps.push(Step::Upload);
            }
        }
        steps
    }

    /// What each step will do, in words and commands, for the dialog.
    #[cfg(test)]
    pub(crate) fn preview(&self, forge: Option<Forge>) -> Vec<(Step, String)> {
        let short = crate::repo::short(&self.commit);
        self.steps()
            .into_iter()
            .map(|step| {
                let text = match step {
                    Step::Bump => {
                        let b = self.bump.as_ref();
                        let mut files: Vec<String> = b
                            .map(|b| {
                                b.manifests
                                    .iter()
                                    .map(|(k, from)| {
                                        format!("{} ({from} → {})", k.files().0, b.version)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if b.is_some_and(|b| b.changelog) {
                            files.push(format!("{CHANGELOG} (the notes on top)"));
                        }
                        format!(
                            "Commits {} on {}: “chore(release): {}”",
                            files.join(", "),
                            b.map(|b| b.branch.as_str()).unwrap_or_default(),
                            self.tag
                        )
                    }
                    Step::Tag => format!(
                        "Makes the {} tag {} at {}, its message the notes (git tag {} {})",
                        if self.sign { "signed" } else { "annotated" },
                        self.tag,
                        if self.bump.is_some() {
                            "that commit".to_owned()
                        } else {
                            short.to_owned()
                        },
                        if self.sign { "-s" } else { "-a" },
                        self.tag
                    ),
                    Step::Push => {
                        let remote = self.push.as_deref().unwrap_or_default();
                        match &self.bump {
                            Some(b) => format!(
                                "Pushes the version commit to {remote}'s {} and the tag to \
                                 {remote}, both or neither (git push --atomic)",
                                b.remote_ref
                                    .strip_prefix("refs/heads/")
                                    .unwrap_or(&b.remote_ref)
                            ),
                            None => format!("Pushes the tag to {remote}"),
                        }
                    }
                    Step::Publish => format!(
                        "Makes the {}release {} on {} with the notes",
                        if self.prerelease { "pre-" } else { "" },
                        self.tag,
                        forge.map_or("the forge", Forge::name)
                    ),
                    Step::Upload => format!(
                        "Attaches {} to it",
                        self.assets
                            .iter()
                            .map(|a| a
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                (step, text)
            })
            .collect()
    }

    /// The tag's message: the tag, then the notes.
    fn tag_message(&self) -> String {
        if self.notes.trim().is_empty() {
            self.tag.clone()
        } else {
            format!("{}\n\n{}", self.tag, self.notes.trim())
        }
    }
}

mod publication;
pub(crate) use publication::*;

mod changelog;
pub(crate) use changelog::*;
