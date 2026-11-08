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
