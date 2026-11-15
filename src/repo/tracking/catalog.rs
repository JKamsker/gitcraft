//! Search preparation belongs to the snapshot, not the repaint loop.
use super::{BTreeMap, BTreeSet, View};

#[derive(Default)]
pub(super) struct Catalog {
    pub(super) captions: Vec<String>,
    normalized: Vec<String>,
    pub(super) filtered: Vec<usize>,
    query: Option<String>,
    include_hidden: bool,
    hidden: BTreeSet<String>,
    pub(super) upstream_available: bool,
    #[cfg(test)]
    pub(super) examined: usize,
}
impl Catalog {
    pub(super) fn new(view: &View, labels: &BTreeMap<String, crate::ref_labels::Label>) -> Self {
        let mut catalog = Self::default();
        for choice in &view.choices {
            let caption = labels
                .get(&choice.full)
                .map_or(&choice.path, |l| &l.caption);
            catalog.captions.push(caption.clone());
            catalog.normalized.push(
                format!(
                    "{} {} {} {caption}",
                    choice.full, choice.path, choice.remote
                )
                .to_lowercase(),
            );
            catalog.upstream_available |= choice.full == view.current_full;
        }
        catalog
    }
    pub(super) fn filter(
        &mut self,
        view: &View,
        query: &str,
        include_hidden: bool,
        hidden: &BTreeSet<String>,
    ) {
        if self.query.as_deref() == Some(query)
            && self.include_hidden == include_hidden
            && &self.hidden == hidden
        {
            return;
        }
        self.query = Some(query.into());
        self.include_hidden = include_hidden;
        self.hidden.clone_from(hidden);
        let query = query.to_lowercase();
        self.filtered.clear();
        for (index, choice) in view.choices.iter().enumerate() {
            #[cfg(test)]
            {
                self.examined += 1;
            }
            if (include_hidden || !hidden.contains(&choice.remote))
                && self.normalized[index].contains(&query)
            {
                self.filtered.push(index);
            }
        }
    }
}
