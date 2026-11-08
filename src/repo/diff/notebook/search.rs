use super::{
    NotebookData,
    parse::{CellKind, Output},
};
use crate::repo::find::rendered::Document;
use crate::theme::Tokens;

pub(in crate::repo::diff) fn document(nb: &NotebookData, t: &Tokens) -> Document {
    let mut doc = Document::default();
    for change in &nb.diff.cells {
        doc.text(
            &crate::i18n::message(
                "repo-core-1894",
                &[
                    ("number", format!("{}", change.number).into()),
                    ("status", format!("{:?}", change.status).into()),
                ],
            ),
            t.accent,
        );
        for (label, cell) in [
            (crate::i18n::text("repo-core-1564"), &change.old),
            (crate::i18n::text("repo-core-1565"), &change.new),
        ] {
            let Some(cell) = cell else { continue };
            doc.text(
                &crate::i18n::message(
                    "repo-core-1895",
                    &[
                        (
                            "label",
                            crate::repo::messages::display_operation(&label).into(),
                        ),
                        ("label2", format!("{}", cell.kind.label()).into()),
                        (
                            "execution-count",
                            format!("{:?}", cell.execution_count).into(),
                        ),
                    ],
                ),
                t.text_dim,
            );
            if cell.kind == CellKind::Markdown {
                doc.markdown(&cell.source, t.text);
            } else {
                doc.text(&cell.source, t.text);
            }
            doc.text(&cell.metadata, t.text_dim);
            for output in &cell.outputs {
                match output {
                    Output::Text(text) => doc.text(text, t.text),
                    Output::Stream { name, text } => {
                        doc.text(name, t.text_dim);
                        doc.text(text, t.text);
                    }
                    Output::Error {
                        name,
                        value,
                        traceback,
                    } => {
                        doc.text(&format!("{name}: {value}\n{traceback}"), t.danger);
                    }
                    Output::Image { mime, .. } => doc.text(
                        &crate::i18n::message(
                            "repo-core-1897",
                            &[("mime", format!("{}", mime).into())],
                        ),
                        t.text_dim,
                    ),
                    Output::Other(kinds, _) => doc.text(
                        &crate::i18n::message(
                            "repo-core-1898",
                            &[("kinds", format!("{}", kinds.join(", ")).into())],
                        ),
                        t.warning,
                    ),
                }
            }
        }
    }
    if let Some(metadata) = &nb.diff.metadata {
        for (_, text) in metadata {
            doc.text(text, t.text_dim);
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cell_sources_outputs_errors_and_metadata_are_read_without_notebook_json()
    -> Result<(), String> {
        let notebook=super::super::parse::parse(br#"{"nbformat":4,"nbformat_minor":5,"metadata":{},"cells":[{"cell_type":"code","id":"a","source":["print('hello')"],"execution_count":1,"metadata":{},"outputs":[{"output_type":"stream","name":"stdout","text":"visible result"}]}]}"#)?;
        let data = NotebookData {
            diff: super::super::compare::compare(None, Some(&notebook)),
            strip: crate::git::StripState::Off,
            compared_stripped: false,
        };
        let text = document(&data, &Tokens::get(&egui::Context::default()))
            .rows
            .into_iter()
            .map(|j| j.text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("print('hello')"));
        assert!(text.contains("visible result"));
        assert!(!text.contains("nbformat_minor"));
        Ok(())
    }
}
