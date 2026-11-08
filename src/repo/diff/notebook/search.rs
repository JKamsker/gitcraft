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
            &format!("Cell {} · {:?}", change.number, change.status),
            t.accent,
        );
        for (label, cell) in [("Before", &change.old), ("After", &change.new)] {
            let Some(cell) = cell else { continue };
            doc.text(
                &format!(
                    "{label} · {} · execution {:?}",
                    cell.kind.label(),
                    cell.execution_count
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
                        &format!("Image output ({mime}); pixels are not searched"),
                        t.text_dim,
                    ),
                    Output::Other(kinds, _) => doc.text(
                        &format!("Unsupported output: {}", kinds.join(", ")),
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
