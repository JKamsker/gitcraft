use super::*;

impl Metadata {
    pub(super) fn license_summary(&self) -> String {
        let mut terms = Vec::new();
        if let Some(expression) = &self.license {
            terms.push(expression.clone());
        }
        if let Some((path, content)) = &self.license_file {
            terms.push(crate::i18n::message(
                "repo-core-3991",
                &[
                    ("path", path.to_string().into()),
                    (
                        "else",
                        (if content.is_err() {
                            crate::i18n::text("repo-core-3992")
                        } else {
                            ""
                        })
                        .to_string()
                        .into(),
                    ),
                ],
            ));
        }
        if let Some(note) = &self.license_note {
            terms.push(note.clone());
        }
        if terms.is_empty() {
            crate::i18n::text("repo-core-3993").into()
        } else {
            terms.join("; ")
        }
    }
    pub(super) fn license_complete(&self) -> bool {
        self.license_note.is_none()
            && self
                .license_file
                .as_ref()
                .is_none_or(|(_, text)| text.is_ok())
    }
    pub(super) fn licence_eq(&self, other: &Self) -> bool {
        self.license == other.license && self.license_file == other.license_file
    }
    pub(super) fn append_document(
        &self,
        doc: &mut crate::repo::find::rendered::Document,
        side: &str,
        t: &crate::theme::Tokens,
    ) {
        doc.text(
            &crate::i18n::message(
                "repo-core-3994",
                &[
                    ("side", side.to_string().into()),
                    ("archive-url", self.archive_url.to_string().into()),
                ],
            ),
            t.text_dim,
        );
        doc.text(
            &crate::i18n::message(
                "repo-core-3995",
                &[
                    ("side", side.to_string().into()),
                    ("license-summary", self.license_summary().to_string().into()),
                ],
            ),
            t.warning,
        );
        if let Some((path, content)) = &self.license_file {
            doc.text(
                &crate::i18n::message(
                    "repo-core-3996",
                    &[
                        ("side", side.to_string().into()),
                        ("path", path.to_string().into()),
                    ],
                ),
                t.text,
            );
            match content {
                Ok(text) => doc.text(text, t.text),
                Err(error) => doc.text(error, t.warning),
            }
        }
        if self.changelogs.is_empty() {
            doc.text(
                &crate::i18n::message("repo-core-3997", &[("side", side.to_string().into())]),
                t.text_dim,
            );
        }
        for (path, text) in &self.changelogs {
            doc.text(
                &crate::i18n::message(
                    "repo-core-3998",
                    &[
                        ("side", side.to_string().into()),
                        ("path", path.to_string().into()),
                    ],
                ),
                t.text,
            );
            doc.text(text, t.text);
        }
        for (path, reason) in &self.skipped {
            doc.text(
                &crate::i18n::message(
                    "repo-core-3999",
                    &[
                        ("side", side.to_string().into()),
                        ("path", path.to_string().into()),
                        ("reason", reason.to_string().into()),
                    ],
                ),
                t.warning,
            );
        }
    }
}
pub(super) fn side_summary(side: &Side) -> String {
    match side {
        Ok(Some(m)) => m.license_summary(),
        Ok(None) => crate::i18n::text("repo-core-4000").into(),
        Err(error) => {
            crate::i18n::message("repo-core-4001", &[("error", error.to_string().into())])
        }
    }
}
pub(super) fn changed(answer: &Answer) -> bool {
    match answer {
        (Ok(Some(a)), Ok(Some(b))) if a.license_complete() && b.license_complete() => {
            !a.licence_eq(b)
        }
        _ => false,
    }
}
