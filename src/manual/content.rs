//! The embedded manual outline and tip identities. Translations stay in Fluent catalogs.
//!
//! `docs/manual-features.txt` lists each heading followed by its paragraph IDs, with blank
//! lines between sections. Order is display order. IDs are permanent: never renumber them
//! when adding prose. Keep both locale catalogs and their whole-message translations paired.

use std::sync::LazyLock;

type Section = (&'static str, Vec<&'static str>);

pub(super) fn features() -> &'static [Section] {
    static FEATURES: LazyLock<Vec<Section>> =
        LazyLock::new(|| parse_outline(include_str!("../../docs/manual-features.txt")));
    &FEATURES
}

fn parse_outline(source: &'static str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut lines = source.lines();
    while let Some(title) = lines.next() {
        if title.is_empty() {
            continue;
        }
        let mut points = Vec::new();
        for point in lines.by_ref() {
            if point.is_empty() {
                break;
            }
            points.push(point);
        }
        sections.push((title, points));
    }
    sections
}

/// The "Did you know" tips under the graph: features above, each said in one short line of its
/// own (`Ctrl` as written in them). Keep them in step with [`features`].
pub(super) const TIPS: &[&str] = &[
    "interaction-manual-tip-1",
    "interaction-manual-tip-2",
    "interaction-manual-tip-3",
    "interaction-manual-tip-4",
    "interaction-manual-tip-5",
    "interaction-manual-tip-6",
    "interaction-manual-tip-7",
    "interaction-manual-tip-8",
    "interaction-manual-tip-9",
    "interaction-manual-tip-10",
    "interaction-manual-tip-11",
    "interaction-manual-tip-12",
    "interaction-manual-tip-13",
    "interaction-manual-tip-14",
    "interaction-manual-tip-15",
    "interaction-manual-tip-16",
    "interaction-manual-tip-17",
    "interaction-manual-tip-18",
    "interaction-manual-tip-19",
    "interaction-manual-tip-20",
    "interaction-manual-tip-21",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn outline_preserves_order_and_boundaries_for_lf_and_crlf() {
        let expected = vec![
            ("heading-a", vec!["a-02", "a-01"]),
            ("heading-b", vec!["b"]),
        ];
        for source in [
            "heading-a\na-02\na-01\n\nheading-b\nb\n",
            "\r\nheading-a\r\na-02\r\na-01\r\n\r\n\r\nheading-b\r\nb",
        ] {
            assert_eq!(parse_outline(source), expected);
        }
        assert!(parse_outline("").is_empty());
        assert!(parse_outline("\n\n").is_empty());
    }

    #[test]
    fn embedded_outline_covers_both_catalogs_once_with_valid_sections() -> Result<(), String> {
        let sections = features();
        assert_eq!(sections.len(), 25);
        assert_eq!(
            sections
                .iter()
                .map(|(_, points)| points.len())
                .sum::<usize>(),
            219
        );
        let mut ids = BTreeSet::new();
        for (title, points) in sections {
            assert!(title.starts_with("interaction-manual-feature-"));
            assert!(title.ends_with("-title"));
            assert!(!points.is_empty(), "empty section {title}");
            assert!(ids.insert(*title), "duplicate title {title}");
            let prefix = title
                .strip_suffix("title")
                .ok_or_else(|| format!("section title lacks title suffix: {title}"))?;
            for point in points {
                assert!(point.starts_with(prefix), "misplaced paragraph {point}");
                assert!(ids.insert(*point), "duplicate paragraph {point}");
            }
        }
        for source in [
            include_str!("../../locales/en-US/interaction-manual-features.ftl"),
            include_str!("../../locales/de/interaction-manual-features.ftl"),
        ] {
            let catalog_ids: Vec<_> = source
                .lines()
                .filter_map(|line| line.split_once(" = ").map(|(id, _)| id))
                .collect();
            assert_eq!(catalog_ids.len(), ids.len());
            assert_eq!(catalog_ids.into_iter().collect::<BTreeSet<_>>(), ids);
        }
        Ok(())
    }
}
