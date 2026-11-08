//! Git remote.c's ordered rewrite groups, for metadata-only literal routes.

#[derive(Default)]
struct Rewrites<'a> {
    groups: Vec<(&'a str, Vec<&'a str>)>,
}

impl<'a> Rewrites<'a> {
    fn add(&mut self, base: &'a str, prefix: &'a str) {
        if let Some((_, prefixes)) = self.groups.iter_mut().find(|(b, _)| *b == base) {
            prefixes.push(prefix);
        } else {
            self.groups.push((base, vec![prefix]));
        }
    }

    fn apply(&self, operand: &str) -> Option<String> {
        let mut best: Option<(&str, &str)> = None;
        for (base, prefixes) in &self.groups {
            for prefix in prefixes {
                if operand.starts_with(prefix)
                    && best.is_none_or(|(_, previous)| prefix.len() > previous.len())
                {
                    best = Some((base, prefix));
                }
            }
        }
        best.map(|(base, prefix)| format!("{base}{}", &operand[prefix.len()..]))
    }
}

pub(super) fn resolve(configuration: &str, operand: &str, push: bool) -> Result<String, String> {
    let mut ordinary = Rewrites::default();
    let mut pushing = Rewrites::default();
    if !configuration.is_empty() && !configuration.ends_with('\0') {
        return Err("Unterminated Git URL rewrite metadata".into());
    }
    for record in configuration.split_terminator('\0') {
        let (key, prefix) = record
            .split_once('\n')
            .ok_or("Malformed Git URL rewrite metadata")?;
        let key = key
            .strip_prefix("url.")
            .ok_or("Unexpected Git URL rewrite metadata key")?;
        if let Some(base) = key.strip_suffix(".insteadof") {
            ordinary.add(base, prefix);
        } else if let Some(base) = key.strip_suffix(".pushinsteadof") {
            pushing.add(base, prefix);
        } else {
            return Err("Unexpected Git URL rewrite metadata variable".into());
        }
    }
    Ok(push
        .then(|| pushing.apply(operand))
        .flatten()
        .or_else(|| ordinary.apply(operand))
        .unwrap_or_else(|| operand.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_metadata_preserves_opaque_operands_without_rules() -> Result<(), String> {
        for value in [
            "https://example.invalid/a",
            "ssh://host/a",
            "host:path",
            "./local",
            "/local",
            "ext::canary",
            "no-such-remote",
        ] {
            assert_eq!(resolve("", value, false)?, value);
            assert_eq!(resolve("", value, true)?, value);
        }
        Ok(())
    }

    #[test]
    fn literal_rewrites_use_longest_prefix_and_one_substitution() -> Result<(), String> {
        let rules = "url.ordinary:.insteadof\nshort:\0url.long:.insteadof\nshort:long/\0url.push:.pushinsteadof\nshort:\0url.wrong:.insteadof\npush:\0";
        assert_eq!(resolve(rules, "short:long/repo", false)?, "long:repo");
        assert_eq!(resolve(rules, "short:long/repo", true)?, "push:long/repo");
        assert_eq!(
            resolve("url.fallback:.insteadof\nx:\0", "x:r", true)?,
            "fallback:r"
        );
        Ok(())
    }

    #[test]
    fn literal_rewrite_ties_follow_base_group_order() -> Result<(), String> {
        for variable in ["insteadof", "pushinsteadof"] {
            let rules = format!(
                "url.A:.{variable}\nunmatched:\0url.B:.{variable}\nsame:\0url.A:.{variable}\nsame:\0"
            );
            assert_eq!(resolve(&rules, "same:repo", true)?, "A:repo");
        }
        Ok(())
    }

    #[test]
    fn literal_rewrite_metadata_preserves_case_empty_prefix_and_newlines() -> Result<(), String> {
        assert_eq!(
            resolve(
                "url.HTTPS://Mixed.Example/A.B/.insteadof\nx:\0",
                "x:r",
                false
            )?,
            "HTTPS://Mixed.Example/A.B/r"
        );
        assert_eq!(resolve("url.base/.insteadof\n\0", "r", false)?, "base/r");
        assert_eq!(
            resolve("url.base/.insteadof\nx\ny\0", "x\nyr", false)?,
            "base/r"
        );
        assert_eq!(resolve("url.base/.insteadof\nX:\0", "x:r", false)?, "x:r");
        for malformed in [
            "url.a.insteadof\nx",
            "url.a.insteadof\0",
            "\0",
            "other.a.insteadof\nx\0",
            "url.a.other\nx\0",
        ] {
            assert!(resolve(malformed, "x", false).is_err());
        }
        Ok(())
    }
}
