//! Presentation using the same stored text and actual command validator, never guessed I/O.
use super::{Contents, FILE, MAX_BYTES, RepoFile};

impl RepoFile {
    pub fn display_error(&self) -> Option<String> {
        let raw = self.commands.as_ref().err()?;
        if !self.parsed_text {
            return Some(raw.clone());
        }
        let render = || -> Option<String> {
            if self.text.len() > MAX_BYTES {
                let expected = format!(
                    "{FILE} is larger than {} KiB, so it isn't read",
                    MAX_BYTES / 1024
                );
                return (raw == &expected).then(|| {
                    crate::i18n::message(
                        "interaction-custom-display-large",
                        &[("file", FILE.into()), ("limit", (MAX_BYTES / 1024).into())],
                    )
                });
            }
            let contents: Contents = match serde_json::from_str(&self.text) {
                Ok(contents) => contents,
                Err(detail) => {
                    let expected = format!("{FILE} can't be read: {detail}");
                    return (raw == &expected).then(|| {
                        crate::i18n::message(
                            "interaction-custom-display-json",
                            &[("file", FILE.into()), ("detail", detail.to_string().into())],
                        )
                    });
                }
            };
            for (i, command) in contents.commands.iter().enumerate() {
                if let Some(problem) = command.problem() {
                    let name = command.name.trim();
                    let raw_which = if name.is_empty() {
                        format!("Command {}", i + 1)
                    } else {
                        format!("“{name}”")
                    };
                    if raw != &format!("{FILE}: {raw_which}: {problem}") {
                        return None;
                    }
                    let which = if name.is_empty() {
                        crate::i18n::message(
                            "interaction-custom-display-index",
                            &[("index", (i + 1).into())],
                        )
                    } else {
                        crate::i18n::message(
                            "interaction-custom-display-name",
                            &[("name", name.into())],
                        )
                    };
                    return Some(crate::i18n::message(
                        "interaction-custom-display-problem",
                        &[
                            ("file", FILE.into()),
                            ("command", which.into()),
                            ("problem", super::super::display_problem(&problem).into()),
                        ],
                    ));
                }
            }
            None
        };
        Some(render().unwrap_or_else(|| raw.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_repo_file_parse_errors_render_from_own_text_and_leave_raw_trust_identity()
    -> Result<(), serde_json::Error> {
        let invalid_command =
            serde_json::json!({"commands":[{"name":"Name: “色” {$x}","command":""}]}).to_string();
        let fixtures = [
            " ".to_owned(),
            "x".repeat(MAX_BYTES + 1),
            invalid_command,
            "{\"commands\":[{\"name\":\"\",\"command\":\"true\"}]}".to_owned(),
        ];
        for text in fixtures {
            let file = RepoFile::new(text);
            let before = file.clone();
            let raw = file.commands.as_ref().err().cloned().unwrap_or_default();
            assert!(!raw.is_empty());
            crate::i18n::set_language("de");
            assert_ne!(file.display_error().as_deref(), Some(raw.as_str()));
            assert_eq!(file, before);
            assert_eq!(super::super::digest(&file.text), file.digest);
            crate::i18n::set_language("en-US");
            assert_eq!(file.display_error().as_deref(), Some(raw.as_str()));
        }
        let text =
            serde_json::json!({"commands":[{"name":"Name: “色” {$x}","command":""}]}).to_string();
        let file = RepoFile::new(text);
        crate::i18n::set_language("de");
        assert!(
            file.display_error()
                .is_some_and(|s| s.contains("Name: “色” {$x}"))
        );
        let valid = RepoFile::new(
            serde_json::json!({"commands":[{"name":"Name","command":"true"}]}).to_string(),
        );
        assert_eq!(valid.display_error(), None);
        Ok(())
    }

    #[test]
    fn unreadable_io_lookalikes_and_mutated_stale_text_have_no_parser_provenance() {
        let raw = super::super::parse_repo_file("").err().unwrap_or_default();
        crate::i18n::set_language("de");
        assert_ne!(
            RepoFile::new(String::new()).display_error().as_deref(),
            Some(raw.as_str())
        );
        assert_eq!(
            RepoFile::unreadable(raw.clone()).display_error().as_deref(),
            Some(raw.as_str())
        );
        let mut file = RepoFile::new("bad JSON".into());
        let original = file.commands.as_ref().err().cloned();
        file.text = "{\"commands\":[]}".into();
        assert_eq!(file.display_error(), original);
    }
}
