//! Searchable reading of rich content, built from rendered text rather than patch source.
//! Closing Find restores the original rich layout. Search rows retain inline formatting.
use super::{Find, Layout, Seen, Source, SourceKey, paint_line};
use crate::theme::Tokens;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Ui};

#[derive(Default)]
pub(in crate::repo) struct Document {
    pub rows: Vec<LayoutJob>,
}
impl Document {
    /// Split very long text into complete searchable rows, preserving UTF-8 and inline styles.
    fn bound_rows(&mut self) {
        let limit = crate::repo::highlight::MAX_LINE_CHARS;
        self.rows = std::mem::take(&mut self.rows)
            .into_iter()
            .flat_map(|job| {
                let mut out = Vec::new();
                let mut start = 0;
                while start < job.text.len() {
                    let end = job.text[start..]
                        .char_indices()
                        .nth(limit)
                        .map_or(job.text.len(), |(n, _)| start + n);
                    let mut part = LayoutJob {
                        text: job.text[start..end].to_owned(),
                        ..Default::default()
                    };
                    for section in &job.sections {
                        let a = section.byte_range.start.0.max(start);
                        let b = section.byte_range.end.0.min(end);
                        if a < b {
                            let mut section = section.clone();
                            section.byte_range =
                                egui::text::ByteIndex(a - start)..egui::text::ByteIndex(b - start);
                            part.sections.push(section);
                        }
                    }
                    out.push(part);
                    start = end;
                }
                if out.is_empty() {
                    out.push(job);
                }
                out
            })
            .collect();
    }
    pub fn job(&mut self, mut job: LayoutJob) {
        job.text = job.text.replace(['\n', '\r'], " ");
        job.wrap.max_width = f32::INFINITY;
        self.rows.push(job);
    }
    pub fn text(&mut self, text: &str, color: Color32) {
        for line in text.lines() {
            let mut job = LayoutJob::default();
            job.append(
                line,
                0.0,
                TextFormat::simple(FontId::proportional(14.0), color),
            );
            self.rows.push(job);
        }
    }
    pub fn markdown(&mut self, markdown: &str, color: Color32) {
        use pulldown_cmark::{Event, Tag, TagEnd};
        let mut job = LayoutJob::default();
        let mut heading = false;
        let mut strong = 0usize;
        let mut emphasis = 0usize;
        let mut strike = 0usize;
        let mut code = false;
        let mut link = 0usize;
        let flush = |doc: &mut Self, job: &mut LayoutJob| {
            if !job.text.is_empty() {
                doc.rows.push(std::mem::take(job));
            }
        };
        for event in
            pulldown_cmark::Parser::new_ext(markdown, crate::pull_request::markdown_options())
        {
            match event {
                Event::Start(Tag::Heading { .. }) => {
                    flush(self, &mut job);
                    heading = true;
                }
                Event::End(TagEnd::Heading(_)) => {
                    flush(self, &mut job);
                    heading = false;
                }
                Event::Start(Tag::Strong) => strong += 1,
                Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
                Event::Start(Tag::Emphasis) => emphasis += 1,
                Event::End(TagEnd::Emphasis) => emphasis = emphasis.saturating_sub(1),
                Event::Start(Tag::Strikethrough) => strike += 1,
                Event::End(TagEnd::Strikethrough) => strike = strike.saturating_sub(1),
                Event::Start(Tag::Link { .. }) => link += 1,
                Event::End(TagEnd::Link) => link = link.saturating_sub(1),
                Event::Start(Tag::CodeBlock(_)) => {
                    flush(self, &mut job);
                    code = true;
                }
                Event::End(TagEnd::CodeBlock) => {
                    flush(self, &mut job);
                    code = false;
                }
                Event::Start(Tag::Item) => {
                    flush(self, &mut job);
                    job.append(
                        "• ",
                        0.0,
                        TextFormat::simple(FontId::proportional(14.0), color),
                    );
                }
                Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::TableRow)
                | Event::HardBreak
                | Event::Rule => flush(self, &mut job),
                Event::SoftBreak => job.append(
                    " ",
                    0.0,
                    TextFormat::simple(FontId::proportional(14.0), color),
                ),
                Event::Text(text)
                | Event::Html(text)
                | Event::InlineHtml(text)
                | Event::InlineMath(text)
                | Event::DisplayMath(text) => {
                    for (index, line) in text.split('\n').enumerate() {
                        if index > 0 {
                            flush(self, &mut job);
                        }
                        let mut format = TextFormat::simple(
                            if code {
                                FontId::monospace(13.0)
                            } else {
                                FontId::proportional(if heading { 18.0 } else { 14.0 })
                            },
                            color,
                        );
                        format.italics = emphasis > 0;
                        if strong > 0 {
                            format.font_id = crate::theme::medium(format.font_id.size);
                        }
                        if link > 0 {
                            format.underline = egui::Stroke::new(1.0, color);
                        }
                        if strike > 0 {
                            format.strikethrough = egui::Stroke::new(1.0, color);
                        }
                        job.append(line, 0.0, format);
                    }
                }
                Event::Code(text) => job.append(
                    &text,
                    0.0,
                    TextFormat::simple(FontId::monospace(13.0), color),
                ),
                Event::TaskListMarker(done) => job.append(
                    if done { "☑ " } else { "☐ " },
                    0.0,
                    TextFormat::simple(FontId::proportional(14.0), color),
                ),
                Event::End(TagEnd::TableCell) => job.append(
                    "  │  ",
                    0.0,
                    TextFormat::simple(FontId::proportional(14.0), color),
                ),
                _ => {}
            }
        }
        flush(self, &mut job);
    }
}
impl Find {
    pub(in crate::repo) fn rendered(
        &mut self,
        ui: &mut Ui,
        identity: String,
        build: impl FnOnce(&Tokens) -> Document,
    ) {
        let t = Tokens::get(ui.ctx());
        let (key, doc) = match self.rendered_document.take() {
            Some((key, doc)) if key == identity => (key, doc),
            _ => {
                let mut doc = build(&t);
                doc.bound_rows();
                (identity, doc)
            }
        };
        ui.label(
            egui::RichText::new(
                "Find reading view · rendered text · Close Find to restore the rich layout",
            )
            .small()
            .color(t.text_dim),
        );
        let source = SourceKey {
            what: Source::Rendered(key.clone()),
            at: doc.rows.as_ptr() as usize,
            len: doc.rows.len(),
        };
        self.update(
            ui.ctx(),
            source,
            doc.rows.len(),
            |i| doc.rows.get(i).map(|j| j.text.as_str()),
            Layout::Lines,
        );
        let mut scroll = egui::ScrollArea::both()
            .id_salt(("find-rendered", &key))
            .auto_shrink(false);
        if let Some(hit) = self.take_reveal() {
            if let Some(row) = doc.rows.get(hit.line) {
                let galley = ui.fonts_mut(|f| f.layout_job(row.clone()));
                let left = galley
                    .pos_from_cursor(egui::text::CCursor::new(hit.start))
                    .left()
                    + 12.0;
                let right = galley
                    .pos_from_cursor(egui::text::CCursor::new(hit.end))
                    .right()
                    + 12.0;
                if left < self.seen.x || right > self.seen.x + ui.available_width() {
                    scroll = scroll
                        .horizontal_scroll_offset((left - ui.available_width() / 3.0).max(0.0));
                }
            }
            scroll = scroll.vertical_scroll_offset(
                (hit.row as f32 * 24.0 - ui.available_height() / 2.0).max(0.0),
            );
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let result = scroll.show_rows(ui, 24.0, doc.rows.len(), |ui, range| {
            for index in range {
                let galley = ui.fonts_mut(|f| f.layout_job(doc.rows[index].clone()));
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(galley.size().x + 24.0), 24.0),
                    egui::Sense::hover(),
                );
                paint_line(
                    ui.painter(),
                    (rect.left() + 12.0, rect.center().y, 24.0),
                    galley,
                    self.marks(index),
                    &t,
                );
            }
        });
        self.saw(Seen {
            x: result.state.offset.x,
            y: result.state.offset.y,
            height: result.inner_rect.height(),
            text_w: result.inner_rect.width(),
            line_h: 24.0,
        });
        self.rendered_document = Some((key, doc));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::find::Query;
    fn painted(output: &egui::FullOutput) -> String {
        output
            .shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    Some(t.galley.text())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn markdown_searches_visible_text_across_inline_styles_and_not_link_destinations() {
        let mut doc = Document::default();
        doc.markdown(
            "# Héllo **world**\n\n[visible label](https://hidden.invalid/secret) and `code`",
            Color32::WHITE,
        );
        let text = doc
            .rows
            .iter()
            .map(|j| j.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Héllo world"), "{text}");
        assert!(text.contains("visible label and code"), "{text}");
        assert!(!text.contains("hidden.invalid"));
        assert!(!text.contains("**"));
        assert!(doc.rows[0].sections.len() > 1);
    }
    #[test]
    fn rendered_reader_marks_steps_and_invalidates_exact_source() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut find = Find {
            open: true,
            query: Query {
                text: "café".into(),
                ..Query::default()
            },
            ..Find::default()
        };
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                find.rendered(ui, "a".into(), |t| {
                    let mut doc = Document::default();
                    doc.markdown("**café** then café", t.text);
                    doc
                })
            });
            assert!(painted(&output).contains("café then café"));
            assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.fill==Tokens::get(&ctx).warning.gamma_multiply(0.6))));
            output.drop_without_applying_deltas();
        }
        assert_eq!(find.count(), 2);
        assert_eq!(find.current().map(|h| h.start), Some(0));
        find.step(1);
        assert_eq!(find.current().map(|h| h.start), Some(10));
        find.step(-1);
        assert_eq!(find.current().map(|h| h.start), Some(0));
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            find.rendered(ui, "b".into(), |t| {
                let mut doc = Document::default();
                doc.text("new content", t.text);
                doc
            })
        });
        output.drop_without_applying_deltas();
        assert_eq!(find.count(), 0);
        find.close();
        assert!(find.rendered_document.is_none());
    }
    #[test]
    fn long_unicode_rows_keep_all_text_and_section_byte_ranges() {
        let mut doc = Document::default();
        let text = "é".repeat(crate::repo::highlight::MAX_LINE_CHARS + 10);
        doc.text(&text, Color32::WHITE);
        doc.bound_rows();
        assert_eq!(doc.rows.len(), 2);
        assert_eq!(
            doc.rows.iter().map(|j| j.text.as_str()).collect::<String>(),
            text
        );
        for row in &doc.rows {
            for section in &row.sections {
                assert!(
                    row.text
                        .get(section.byte_range.start.0..section.byte_range.end.0)
                        .is_some()
                );
            }
        }
    }
}
