//! Presentation from the actual recipe fields; validation and executable script stay raw.
use super::Recipe;

impl Recipe {
    /// The same ordered validation results, translated only for presentation.
    pub(crate) fn display_problems(&self) -> Vec<String> {
        self.problems()
            .iter()
            .map(|raw| self.display_problem(raw))
            .collect()
    }

    /// Use the submitted recipe for a save error, not a later edited form.
    /// Unknown storage/tool/provider errors have no inferred field context.
    pub(crate) fn display_error(&self, raw: &str) -> String {
        if let Some(first) = self.problems().first()
            && raw == format!("The recipe isn't saved: {first}")
        {
            return crate::i18n::message(
                "interaction-recipe-not-saved",
                &[("problem", self.display_problem(first).into())],
            );
        }
        raw.to_owned()
    }

    fn display_problem(&self, raw: &str) -> String {
        for path in self.copy.iter().chain(&self.link) {
            if let Some(why) = super::path_problem(path)
                && raw == format!("{path}: {why}")
            {
                let id = match why {
                    "empty path" => "interaction-recipe-path-empty",
                    "starts or ends with a space" => "interaction-recipe-path-space",
                    "a path is one line" => "interaction-recipe-path-line",
                    "give a path inside the worktree, relative to its top folder" => {
                        "interaction-recipe-path-relative"
                    }
                    "a path can't go out of the worktree (..)" => "interaction-recipe-path-outside",
                    "that is the whole worktree" => "interaction-recipe-path-whole",
                    _ => return raw.to_owned(),
                };
                return crate::i18n::message(
                    "interaction-recipe-path",
                    &[
                        ("path", path.as_str().into()),
                        ("reason", crate::i18n::text(id).into()),
                    ],
                );
            }
        }
        for var in &self.env {
            let name = &var.name;
            for (expected, id) in [
                (
                    format!(
                        "Variable \"{name}\": gitcraft sets that one itself, pick another name"
                    ),
                    "interaction-recipe-variable-reserved",
                ),
                (
                    format!(
                        "Variable \"{name}\": names are letters, digits and _, not starting with a digit"
                    ),
                    "interaction-recipe-variable-name",
                ),
                (
                    format!("Variable \"{name}\" is set twice"),
                    "interaction-recipe-variable-duplicate",
                ),
                (
                    format!("Variable \"{name}\": a value is one line"),
                    "interaction-recipe-variable-line",
                ),
            ] {
                if raw == expected {
                    return crate::i18n::message(id, &[("name", name.as_str().into())]);
                }
            }
        }
        raw.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::super::EnvVar;
    use super::*;

    #[test]
    fn actual_recipe_validation_keeps_order_control_and_script_across_locales() {
        let recipe = Recipe {
            copy: vec![
                String::new(),
                " space ".into(),
                "line\nbreak".into(),
                "/outside: 色".into(),
                "../色: {$x}".into(),
                "./".into(),
            ],
            env: vec![
                EnvVar {
                    name: "GITCRAFT_WORKTREE_INDEX".into(),
                    value: "0".into(),
                },
                EnvVar {
                    name: "bad: 色\" {$x}".into(),
                    value: "one\ntwo".into(),
                },
                EnvVar {
                    name: "PORT".into(),
                    value: "3000".into(),
                },
                EnvVar {
                    name: "PORT".into(),
                    value: "3001".into(),
                },
            ],
            command: "printf '%s' 'user supplied: {$x}'".into(),
            ..Recipe::default()
        };
        let raw = recipe.problems();
        let script = recipe.script();
        let json = serde_json::to_string(&recipe).unwrap_or_default();
        assert_eq!(raw.len(), 10);
        crate::i18n::set_language("de");
        let rendered = recipe.display_problems();
        assert_eq!(rendered.len(), raw.len());
        for (a, b) in raw.iter().zip(&rendered) {
            assert_ne!(a, b);
        }
        assert_eq!(
            rendered[4],
            "../色: {$x}: ein Pfad darf den Worktree nicht verlassen (..)"
        );
        assert!(rendered[7].contains("bad: 色\" {$x}"));
        let save_error = format!("The recipe isn't saved: {}", raw[0]);
        assert_eq!(
            recipe.display_error(&save_error),
            "Das Rezept wurde nicht gespeichert: : leerer Pfad"
        );
        assert_eq!(recipe.problems(), raw);
        assert_eq!(recipe.script(), script);
        assert_eq!(serde_json::to_string(&recipe).unwrap_or_default(), json);
        crate::i18n::set_language("en-US");
        assert_eq!(recipe.display_problems(), raw);
        assert_eq!(recipe.display_error(&save_error), save_error);
    }

    #[test]
    fn foreign_details_and_missing_recipe_context_are_not_reparsed() {
        crate::i18n::set_language("de");
        let recipe = Recipe {
            copy: vec!["../owned".into()],
            ..Recipe::default()
        };
        for raw in [
            "The recipe isn't saved: ../other: a path can't go out of the worktree (..)",
            "provider error: Variable \"PORT\" is set twice",
            "/user/色 can't be read: invalid JSON",
        ] {
            assert_eq!(recipe.display_error(raw), raw);
            assert_eq!(recipe.display_problem(raw), raw);
        }
        assert!(Recipe::default().display_problems().is_empty());
    }
}
