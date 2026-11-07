//! The palette entries run lately, kept across restarts: they come first when nothing is typed,
//! and score higher when something is.

use serde::{Deserialize, Serialize};

/// How many entries are remembered.
const KEEP: usize = 30;
/// What the entry run last adds to its score; older ones add less.
const BONUS: i32 = 40;

/// The keys of the entries run lately, the latest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Recent(Vec<String>);

impl Recent {
    /// `key` was run: it goes first.
    pub(crate) fn used(&mut self, key: &str) {
        if key.is_empty() {
            return;
        }
        self.0.retain(|k| k != key);
        self.0.insert(0, key.to_owned());
        self.0.truncate(KEEP);
    }

    /// How recently `key` was run: 0 for the latest, `None` when it wasn't lately.
    pub(crate) fn rank(&self, key: &str) -> Option<usize> {
        self.0.iter().position(|k| k == key)
    }

    /// What having been run lately adds to an entry's score.
    pub(crate) fn bonus(&self, key: &str) -> i32 {
        self.rank(key).map_or(0, |r| {
            let left = KEEP.saturating_sub(r) as i32;
            BONUS * left / KEEP as i32
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_goes_first_without_duplicates() {
        let mut r = Recent::default();
        r.used("Fetch");
        r.used("Push");
        r.used("Fetch");
        assert_eq!(r.rank("Fetch"), Some(0));
        assert_eq!(r.rank("Push"), Some(1));
        assert_eq!(r.rank("Pull"), None);
        assert!(r.bonus("Fetch") > r.bonus("Push"));
        assert_eq!(r.bonus("Pull"), 0);
    }

    #[test]
    fn only_so_many_are_kept() {
        let mut r = Recent::default();
        for k in 0..100 {
            r.used(&format!("entry {k}"));
        }
        assert_eq!(r.0.len(), KEEP);
        assert_eq!(r.rank("entry 99"), Some(0));
        assert_eq!(r.rank("entry 0"), None);
        assert!(r.bonus(&format!("entry {}", 100 - KEEP)) > 0);
    }

    #[test]
    fn an_empty_key_is_not_remembered() {
        let mut r = Recent::default();
        r.used("");
        assert_eq!(r, Recent::default());
    }

    #[test]
    fn it_survives_a_restart_as_a_plain_list() -> Result<(), serde_json::Error> {
        let mut r = Recent::default();
        r.used("Check out ü branch");
        r.used("verb:merge");
        let text = serde_json::to_string(&r)?;
        assert_eq!(text, r#"["verb:merge","Check out ü branch"]"#);
        let back: Recent = serde_json::from_str(&text)?;
        assert_eq!(back, r);
        Ok(())
    }
}
