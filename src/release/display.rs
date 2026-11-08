//! Release presentation only. Raw step labels, plans, notes and command previews stay stable.

use super::{CHANGELOG, Plan, Step};
use crate::forge::Forge;
use crate::i18n::{message, text};

impl Step {
    pub(crate) fn display_label(self) -> &'static str {
        text(match self {
            Self::Bump => "interaction-release-step-bump",
            Self::Tag => "interaction-release-step-tag",
            Self::Push => "interaction-release-step-push",
            Self::Publish => "interaction-release-step-publish",
            Self::Upload => "interaction-release-step-upload",
        })
    }
}

impl Plan {
    /// Localized explanatory prose for painting; never passed to release execution.
    pub(crate) fn display_preview(&self, forge: Option<Forge>) -> Vec<(Step, String)> {
        self.steps()
            .into_iter()
            .map(|step| {
                let line = match step {
                    Step::Bump => {
                        let bump = self.bump.as_ref();
                        let mut files: Vec<String> = bump
                            .map(|b| {
                                b.manifests
                                    .iter()
                                    .map(|(kind, from)| {
                                        format!("{} ({from} → {})", kind.files().0, b.version)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if bump.is_some_and(|b| b.changelog) {
                            files.push(message(
                                "interaction-release-changelog",
                                &[("file", CHANGELOG.into())],
                            ));
                        }
                        message(
                            "interaction-release-bump",
                            &[
                                ("files", files.join(", ").into()),
                                (
                                    "branch",
                                    bump.map(|b| b.branch.as_str()).unwrap_or_default().into(),
                                ),
                                ("tag", self.tag.as_str().into()),
                            ],
                        )
                    }
                    Step::Tag => {
                        let kind = if self.sign { "signed" } else { "annotated" };
                        if self.bump.is_some() {
                            message(
                                "interaction-release-tag-version",
                                &[("kind", kind.into()), ("tag", self.tag.as_str().into())],
                            )
                        } else {
                            message(
                                "interaction-release-tag-object",
                                &[
                                    ("kind", kind.into()),
                                    ("tag", self.tag.as_str().into()),
                                    ("commit", crate::repo::short(&self.commit).into()),
                                ],
                            )
                        }
                    }
                    Step::Push => {
                        let remote = self.push.as_deref().unwrap_or_default();
                        match &self.bump {
                            Some(b) => message(
                                "interaction-release-push-bump",
                                &[
                                    ("remote", remote.into()),
                                    (
                                        "branch",
                                        b.remote_ref
                                            .strip_prefix("refs/heads/")
                                            .unwrap_or(&b.remote_ref)
                                            .into(),
                                    ),
                                ],
                            ),
                            None => message(
                                "interaction-release-push-tag",
                                &[("remote", remote.into())],
                            ),
                        }
                    }
                    Step::Publish => message(
                        "interaction-release-publish",
                        &[
                            ("pre", if self.prerelease { "yes" } else { "no" }.into()),
                            ("tag", self.tag.as_str().into()),
                            (
                                "forge",
                                forge
                                    .map_or_else(|| text("interaction-release-forge"), Forge::name)
                                    .into(),
                            ),
                        ],
                    ),
                    Step::Upload => message(
                        "interaction-release-upload",
                        &[(
                            "files",
                            self.assets
                                .iter()
                                .map(|a| {
                                    a.file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_default()
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                                .into(),
                        )],
                    ),
                };
                (step, line)
            })
            .collect()
    }
}

/// Resolve the current tag validator's error only when it matches these exact inputs.
/// Other messages retain their opaque detail through the finite release display adapter.
pub(crate) fn display_problem(raw: &str, tag: &str, taken: &[String]) -> String {
    let tag = tag.trim();
    if super::tag_problem(tag, taken).as_deref() == Some(raw) {
        if tag.is_empty() {
            return text("interaction-release-tag-empty").to_owned();
        }
        let id = if taken.iter().any(|t| t == tag) {
            "interaction-release-tag-taken"
        } else {
            "interaction-release-tag-invalid"
        };
        return message(id, &[("tag", tag.into())]);
    }
    display_error(raw)
}

/// Known release metadata/worker errors for paint. Raw producers and execution stay unchanged.
pub(crate) fn display_error(raw: &str) -> String {
    let id = match raw {
        "The primary forge changed during this release; publication stopped. Local changes remain undoable. Close this release before starting one on the new forge." => {
            Some("dev-baseline-guard-20")
        }
        "Publication needs the release's push destination" => Some("dev-baseline-guard-19"),
        "Only the tip of the branch checked out can move the version in its files" => {
            Some("interaction-release-note-tip")
        }
        "Commit or stash the changes to the files the version is in first" => {
            Some("interaction-release-note-dirty")
        }
        "nothing to move" => Some("interaction-release-nothing"),
        "no remote to push to" => Some("interaction-release-no-remote"),
        "The repository isn't on a forge gitcraft knows" => {
            Some("interaction-release-unknown-forge")
        }
        _ => None,
    };
    if let Some(id) = id {
        return text(id).to_owned();
    }
    if let Some((branch, tail)) = raw.split_once(" isn't checked out at ")
        && !branch.is_empty()
        && !branch.chars().any(|c| c.is_whitespace() || c == ':')
        && let Some(commit) = tail.strip_suffix(" any more: plan the release again")
        && commit.len() == 8
        && commit.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return message(
            "interaction-release-branch-moved",
            &[("branch", branch.into()), ("commit", commit.into())],
        );
    }
    if let Some(files) =
        raw.strip_suffix(" changed since the release was planned: commit or stash that first")
        && files.split(", ").all(manifest_file)
    {
        return message(
            "interaction-release-files-changed",
            &[("files", files.into())],
        );
    }
    if let Some(file) = raw.strip_prefix("Couldn't find the version in ")
        && manifest_file(file)
    {
        return message(
            "interaction-release-version-missing",
            &[("file", file.into())],
        );
    }
    if let Some(tail) = raw.strip_prefix("Couldn't write ")
        && let Some((file, reason)) = tail.split_once(": ")
        && manifest_file(file)
    {
        return message(
            "interaction-release-write-failed",
            &[
                ("file", file.into()),
                ("reason", crate::git::display_error(reason).into()),
            ],
        );
    }
    crate::git::display_error(raw)
}

fn manifest_file(file: &str) -> bool {
    matches!(
        file,
        "Cargo.toml" | "Cargo.lock" | "package.json" | "package-lock.json" | CHANGELOG
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::{Bump, manifest::Kind};

    fn plan() -> Plan {
        Plan {
            commit: "0123456789abcdef".into(),
            tag: "v1.234.5-rc.1".into(),
            notes: "Opaque release notes 日本語".into(),
            bump: Some(Bump {
                branch: "release/日本語".into(),
                remote_ref: "refs/heads/publish/日本語".into(),
                version: "1.234.5-rc.1".into(),
                manifests: vec![(Kind::Cargo, "1.233.0".into())],
                changelog: true,
            }),
            sign: true,
            push: Some("origin-日本語".into()),
            publish: true,
            prerelease: true,
            assets: vec!["/opaque/installer 日本語.zip".into()],
        }
    }

    #[test]
    fn localized_release_preview_preserves_raw_plan_labels_refs_commands_and_prose_contracts() {
        let plan = plan();
        let snapshot = plan.clone();
        crate::i18n::set_language("en-US");
        let raw = plan.preview(Some(Forge::GitHub));
        assert_eq!(plan.display_preview(Some(Forge::GitHub)), raw);
        crate::i18n::set_language("de");
        assert_eq!(Step::Bump.label(), "Move the version");
        assert_eq!(Step::Bump.display_label(), "Version aktualisieren");
        assert_eq!(Step::Publish.label(), "Make the release on the forge");
        assert_eq!(
            Step::Publish.display_label(),
            "Release auf der Forge erstellen"
        );
        let de = plan.display_preview(Some(Forge::GitHub));
        assert_eq!(de.iter().map(|(s, _)| *s).collect::<Vec<_>>(), plan.steps());
        assert_eq!(
            de[0].1,
            "Committet Cargo.toml (1.233.0 → 1.234.5-rc.1), CHANGELOG.md (Release Notes am Anfang) auf release/日本語: „chore(release): v1.234.5-rc.1“"
        );
        assert!(de[1].1.contains("git tag -s v1.234.5-rc.1"));
        assert!(de[2].1.contains("publish/日本語 auf origin-日本語"));
        assert!(de[2].1.contains("git push --atomic"));
        assert!(de[3].1.contains("Vorab-Release v1.234.5-rc.1 auf GitHub"));
        assert_eq!(de[4].1, "Hängt installer 日本語.zip daran an");
        assert_eq!(plan, snapshot);
        assert_eq!(plan.preview(Some(Forge::GitHub)), raw);
        let mut direct = plan.clone();
        direct.bump = None;
        direct.sign = false;
        direct.prerelease = false;
        let de = direct.display_preview(None);
        assert!(
            de[0]
                .1
                .contains("annotierten Tag v1.234.5-rc.1 an 01234567")
        );
        assert!(de[0].1.contains("git tag -a v1.234.5-rc.1"));
        assert_eq!(de[1].1, "Pusht den Tag nach origin-日本語");
        assert!(de[2].1.contains("auf der Forge"));
        assert!(!de[2].1.contains("Vorab-Release"));
        crate::i18n::set_language("en-US");
        assert_eq!(direct.display_preview(None), direct.preview(None));
    }
}

#[cfg(test)]
mod validation_localization_tests {
    use super::*;

    #[test]
    fn actual_tag_validation_keeps_raw_results_and_only_renders_matching_current_inputs()
    -> Result<(), String> {
        crate::i18n::set_language("de");
        let taken = vec!["v1.234.5-日本語".to_owned()];
        for (tag, raw, de) in [
            (" ", "Name the version", "Benenne die Version"),
            (
                "v1.234.5-日本語",
                "v1.234.5-日本語 is a tag already",
                "v1.234.5-日本語 ist bereits ein Tag",
            ),
            (
                "invalid tag 日本語",
                "invalid tag 日本語 can't be a tag's name",
                "invalid tag 日本語 ist kein gültiger Tag-Name",
            ),
        ] {
            let actual = super::super::tag_problem(tag, &taken).ok_or("missing raw validation")?;
            assert_eq!(actual, raw);
            assert_eq!(display_problem(&actual, tag, &taken), de);
            assert_eq!(super::super::tag_problem(tag, &taken).as_deref(), Some(raw));
        }
        assert_eq!(
            display_problem("foreign is a tag already", "v2", &taken),
            "foreign is a tag already"
        );
        assert!(super::super::tag_problem("v2", &taken).is_none());
        assert_eq!(
            display_error("Couldn't write Cargo.toml: External diagnostic 日本語"),
            "Cargo.toml konnte nicht geschrieben werden: External diagnostic 日本語"
        );
        assert_eq!(
            display_error(
                "topic-日本語 isn't checked out at abcdef12 any more: plan the release again"
            ),
            "topic-日本語 ist nicht mehr bei abcdef12 ausgecheckt: Plane das Release erneut"
        );
        assert_eq!(
            display_error(
                "Cargo.toml, CHANGELOG.md changed since the release was planned: commit or stash that first"
            ),
            "Cargo.toml, CHANGELOG.md wurden seit der Release-Planung geändert: Committe oder stashe zuerst diese Änderungen"
        );
        assert_eq!(
            display_error("Couldn't find the version in package.json"),
            "Die Version konnte in package.json nicht gefunden werden"
        );
        for raw in [
            "Unknown provider error 日本語",
            "Couldn't write external/path: output",
            "external prose isn't checked out at opaque any more: plan the release again",
        ] {
            assert_eq!(display_error(raw), raw);
        }
        crate::i18n::set_language("en-US");
        assert_eq!(
            display_problem("Name the version", "", &taken),
            "Name the version"
        );
        Ok(())
    }
}
