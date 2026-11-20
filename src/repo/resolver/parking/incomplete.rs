//! Reservations of put-asides that never completed (see
//! [`crate::resolver::park::Listing::incomplete`]): a put-aside interrupted before its archive
//! was whole. The worktree was never cleared for them, so the merge is still there and
//! nothing is lost, but their folder and ref stay until the user removes them. They are
//! never removed automatically: the user is told once, each time the set changes.
use super::*;

impl RepoTab {
    pub(super) fn report_incomplete(&mut self, incomplete: &[(String, PathBuf)]) {
        let ids: Vec<String> = incomplete.iter().map(|(id, _)| id.clone()).collect();
        if ids == self.resolver.incomplete_reported {
            return;
        }
        self.resolver.incomplete_reported = ids;
        if let Some((id, dir)) = incomplete.first() {
            let text = notice(incomplete.len(), id, dir);
            self.status = Some(crate::repo::notice::Notice::plain((text, true)));
        }
    }
}

/// What the user is told: where the first one is, and how to remove it (its folder, then
/// its ref), the merge being still in the worktree.
fn notice(count: usize, id: &str, dir: &Path) -> String {
    crate::i18n::message(
        "repo-core-incomplete-parked-archive",
        &[
            ("count", count.into()),
            ("path", dir.display().to_string().into()),
            ("ref", format!("{}{id}", park::REF_PREFIX).into()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(ids: &[&str]) -> park::Listing {
        park::Listing {
            incomplete: ids
                .iter()
                .map(|id| ((*id).to_owned(), PathBuf::from(format!("parked/{id}"))))
                .collect(),
            ..park::Listing::default()
        }
    }

    #[test]
    fn an_incomplete_put_aside_is_reported_once_per_change_with_its_folder_and_ref() {
        let ctx = egui::Context::default();
        let dir = std::env::temp_dir().join("gitcraft-parked-incomplete-report");
        let mut tab = RepoTab::new_deferred(dir, &ctx);
        tab.parked_read(Ok(listing(&["torn-id"])), &ctx);
        let text = tab
            .status
            .take()
            .map(|n| n.raw().0.clone())
            .unwrap_or_default();
        let folder = PathBuf::from("parked/torn-id").display().to_string();
        assert!(text.contains(&folder), "{text}");
        assert!(text.contains("refs/gitcraft/parked/torn-id"), "{text}");
        // The same set again (every reload reads it): not told again.
        tab.parked_read(Ok(listing(&["torn-id"])), &ctx);
        assert!(tab.status.is_none());
        // Another one: told, with the count; once removed, nothing more to tell.
        tab.parked_read(Ok(listing(&["torn-id", "other"])), &ctx);
        assert!(tab.status.take().is_some());
        tab.parked_read(Ok(listing(&[])), &ctx);
        assert!(tab.status.is_none());
        assert!(tab.resolver.incomplete_reported.is_empty());
    }

    #[test]
    fn the_incomplete_put_aside_notice_is_translated_and_pluralized() {
        let dir = Path::new("parked/x");
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            let one = notice(1, "x", dir);
            let two = notice(2, "x", dir);
            assert!(!one.starts_with("repo-core-"), "{language}: {one}");
            assert_ne!(one, two, "{language}");
            assert!(two.contains('2'), "{language}: {two}");
        }
        crate::i18n::set_language("en-US");
    }
}
