//! Conservative push option parsing, separate from repository route resolution.
#[derive(Debug)]
pub(super) struct PushArguments {
    pub positional: Vec<String>,
    pub all: bool,
    pub tags: bool,
    pub deleting: bool,
}

impl PushArguments {
    pub fn parse(words: &[String]) -> Result<Self, String> {
        let mut positional = vec![];
        let mut options = true;
        let mut all = false;
        let mut tags = false;
        let mut deleting = false;
        for word in words {
            if options && word == "--" {
                options = false;
                continue;
            }
            if options && word.starts_with('-') {
                match word.as_str() {
                    "--all" => all = true,
                    "--tags" => tags = true,
                    "--delete" | "-d" => deleting = true,
                    "--porcelain"
                    | "-u"
                    | "--set-upstream"
                    | "--no-verify"
                    | "--atomic"
                    | "--progress"
                    | "-q"
                    | "--quiet"
                    | "-v"
                    | "--verbose"
                    | "-f"
                    | "--force"
                    | "--force-if-includes"
                    | "--follow-tags"
                    | "--no-follow-tags"
                    | "--recurse-submodules=no"
                    | "--recurse-submodules=check"
                    | "--dry-run"
                    | "-n"
                    | "--force-with-lease" => {}
                    w if w.starts_with("--force-with-lease=")
                        || w.starts_with("--push-option=") => {}
                    _ => {
                        return Err(format!(
                            "Reviewer echo cannot infer push option {word}; review this operation explicitly"
                        ));
                    }
                }
            } else {
                positional.push(word.clone());
            }
        }
        Ok(Self {
            positional,
            all,
            tags,
            deleting,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(words: &[&str]) -> Result<PushArguments, String> {
        PushArguments::parse(&words.iter().map(|s| (*s).into()).collect::<Vec<_>>())
    }

    #[test]
    fn option_boundary_preserves_literal_remote_and_refspec_words() -> Result<(), String> {
        let args = parse(&["--all", "--tags", "--delete", "origin", "--", "--quiet"])?;
        assert!(args.all && args.tags && args.deleting);
        assert_eq!(args.positional, ["origin", "--quiet"]);
        let literal = parse(&["--", "--all", "--tags", "--delete"])?;
        assert!(!literal.all && !literal.tags && !literal.deleting);
        assert_eq!(literal.positional, ["--all", "--tags", "--delete"]);
        Ok(())
    }

    #[test]
    fn supported_submodule_modes_preserve_targets_and_option_boundaries() -> Result<(), String> {
        for option in ["--recurse-submodules=no", "--recurse-submodules=check"] {
            let args = parse(&[option, "origin", "HEAD:refs/heads/topic"])?;
            assert_eq!(args.positional, ["origin", "HEAD:refs/heads/topic"]);
            assert!(!args.all && !args.tags && !args.deleting);
            let literal = parse(&["--", option, "HEAD:refs/heads/topic"])?;
            assert_eq!(literal.positional, [option, "HEAD:refs/heads/topic"]);
            assert!(!literal.all && !literal.tags && !literal.deleting);
        }
        Ok(())
    }

    #[test]
    fn unknown_or_separate_value_options_require_explicit_review() {
        for words in [
            &["--receive-pack=custom"][..],
            &["--push-option", "value"],
            &["--recurse-submodules=on-demand"],
            &["--upload-pack", "custom"],
        ] {
            assert!(parse(words).is_err(), "{words:?}");
        }
        assert!(parse(&["--push-option=value", "--force-with-lease=refs/heads/a:old"]).is_ok());
    }
}
