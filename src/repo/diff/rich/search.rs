//! Rendered text for the Find reading view. This never searches an underlying lockfile/patch.
use super::{RichDiff, prose};
use crate::repo::find::rendered::Document;
use crate::theme::Tokens;

pub(in crate::repo) fn document(diff: &RichDiff, t: &Tokens) -> Document {
    let mut doc = Document::default();
    match diff {
        RichDiff::Prose(p) => {
            for part in &p.parts {
                match part {
                    prose::Part::Kept { text, .. } => doc.text(text, t.text),
                    prose::Part::Added { text, .. } => {
                        doc.text(&format!("Added · {text}"), t.success)
                    }
                    prose::Part::Removed { text, .. } => {
                        doc.text(&format!("Removed · {text}"), t.danger)
                    }
                    prose::Part::Changed { words, .. } => {
                        doc.job(super::prose_view::search_changed(words, t))
                    }
                }
            }
        }
        RichDiff::Config(c) => {
            for entry in &c.entries {
                doc.text(&format!("{} · {:?}", entry.path, entry.change), t.text);
                if let Some(old) = &entry.old {
                    doc.text(&format!("Before · {old}"), t.danger);
                }
                if let Some(new) = &entry.new {
                    doc.text(&format!("After · {new}"), t.success);
                }
                if let Some((old, new)) = entry.kinds {
                    doc.text(&format!("Type · {old} → {new}"), t.text_dim);
                }
            }
        }
        RichDiff::Api(a) => {
            for change in &a.changes {
                doc.text(
                    &format!(
                        "{} · {:?} · {}",
                        change.endpoint, change.severity, change.what
                    ),
                    t.text,
                );
            }
        }
        RichDiff::Deps(d) => {
            for change in &d.changes {
                doc.text(
                    &format!(
                        "{} · {:?} · {} → {}",
                        change.name,
                        change.change,
                        change.old.as_deref().unwrap_or("absent"),
                        change.new.as_deref().unwrap_or("absent")
                    ),
                    t.text,
                );
                if let Some((old, new)) = &change.license {
                    doc.text(
                        &format!(
                            "Licence · {} → {}",
                            old.as_deref().unwrap_or("not declared"),
                            new.as_deref().unwrap_or("not declared")
                        ),
                        t.warning,
                    );
                }
            }
        }
        RichDiff::Table(table) => {
            for column in &table.columns {
                doc.text(
                    &format!(
                        "Column · {} → {}",
                        column.old.as_ref().map_or("absent", |(_, s)| s.as_str()),
                        column.new.as_ref().map_or("absent", |(_, s)| s.as_str())
                    ),
                    t.text,
                );
            }
            for row in &table.rows {
                if let Some(old) = &row.old {
                    doc.text(&format!("Before · {}", old.join(" │ ")), t.danger);
                }
                if let Some(new) = &row.new {
                    doc.text(&format!("After · {}", new.join(" │ ")), t.success);
                }
            }
        }
        RichDiff::Dvc(d) => {
            for (old, new) in &d.pairs {
                for (side, item) in [("Before", old), ("After", new)] {
                    if let Some(item) = item {
                        if let Some(files) = item.files {
                            doc.text(&format!("{side}: a folder of {files} files"), t.text);
                        }
                        doc.text(
                            &format!(
                                "{side} · {} · {} · {} · {}",
                                item.path,
                                item.size
                                    .map(crate::git::human_size)
                                    .unwrap_or_else(|| "size not recorded".into()),
                                item.hash.as_deref().unwrap_or("hash not recorded"),
                                item.repo.as_deref().unwrap_or("")
                            ),
                            t.text,
                        );
                    }
                }
            }
        }
    }
    doc
}

pub(in crate::repo) fn with_state(
    diff: &RichDiff,
    state: &super::RichState,
    t: &Tokens,
) -> Document {
    let mut doc = document(diff, t);
    if matches!(diff, RichDiff::Deps(_)) {
        state.metadata.append_document(&mut doc, t);
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(diff: RichDiff) -> String {
        document(&diff, &Tokens::get(&egui::Context::default()))
            .rows
            .into_iter()
            .map(|r| r.text)
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn search_sources_follow_semantic_rich_values_including_folded_and_changed_text()
    -> Result<(), String> {
        use super::super::{Sides, api, config, deps, dvc, table};
        let sides = |a: &str, b: &str| Sides {
            old: Some(a.into()),
            new: Some(b.into()),
        };
        let c = config::diff("a.json", &sides(r#"{"timeout":30}"#, r#"{"timeout":60}"#))?;
        let shown = text(RichDiff::Config(c));
        assert!(shown.contains("timeout") && shown.contains("30") && shown.contains("60"));
        let t = table::diff(
            "a.csv",
            &sides("name,role\nAlice,reader\n", "name,role\nAlice,writer\n"),
        )?;
        let shown = text(RichDiff::Table(t));
        assert!(shown.contains("Alice") && shown.contains("reader") && shown.contains("writer"));
        let d = deps::diff(
            "package-lock.json",
            &sides(
                r#"{"packages":{"node_modules/x":{"version":"1.0.0","license":"MIT"}}}"#,
                r#"{"packages":{"node_modules/x":{"version":"2.0.0","license":"ISC"}}}"#,
            ),
        )?;
        let shown = text(RichDiff::Deps(d));
        assert!(shown.contains("MIT") && shown.contains("ISC") && shown.contains("2.0.0"));
        let d = dvc::diff(&sides(
            "outs:\n- path: a.csv\n  md5: abc\n  size: 3\n",
            "outs:\n- path: a.csv\n  md5: def\n  size: 4\n",
        ))?;
        let shown = text(RichDiff::Dvc(d));
        assert!(shown.contains("a.csv") && shown.contains("abc") && shown.contains("def"));
        let shown = text(RichDiff::Api(api::ApiDiff {
            version: "3.0.0".into(),
            changes: vec![api::ApiChange {
                severity: api::Severity::Breaking,
                endpoint: "GET /users".into(),
                what: "Removed endpoint".into(),
                old_line: Some(2),
                new_line: None,
            }],
        }));
        assert!(shown.contains("GET /users") && shown.contains("Removed endpoint"));
        Ok(())
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;
    #[test]
    fn changed_prose_search_retains_styled_tail_after_normal_display_cutoff() {
        let mut words = vec![(super::super::words::Change::Kept, "a ".into()); 11000];
        words.push((super::super::words::Change::Added, "TAIL_NEEDLE".into()));
        let prose = super::super::prose::ProseDiff {
            parts: vec![prose::Part::Changed {
                old: 1,
                new: 1,
                words,
            }],
        };
        let doc = document(
            &RichDiff::Prose(prose),
            &Tokens::get(&egui::Context::default()),
        );
        assert!(doc.rows.iter().any(|r| r.text.ends_with("TAIL_NEEDLE")));
        assert!(
            doc.rows.last().is_some_and(|r| r.sections.len() < 10),
            "consecutive word styles should coalesce"
        );
    }
    #[test]
    fn dvc_folder_file_counts_are_part_of_the_reading() -> Result<(), String> {
        let data = super::super::dvc::diff(&super::super::Sides {
            old: Some("outs:\n  - path: data\n    nfiles: 1729\n    md5: a.dir\n".into()),
            new: Some("outs:\n  - path: data\n    nfiles: 1730\n    md5: b.dir\n".into()),
        })?;
        let doc = document(
            &RichDiff::Dvc(data),
            &Tokens::get(&egui::Context::default()),
        );
        let text = doc
            .rows
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Before: a folder of 1729 files"));
        assert!(text.contains("After: a folder of 1730 files"));
        Ok(())
    }
}
