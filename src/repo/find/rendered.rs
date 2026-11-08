//! Searchable reading of rich content, built from rendered text rather than patch source.
//! Closing Find restores the original rich layout. Search rows retain inline formatting.
use super::{Find, Layout, Seen, Source, SourceKey, paint_line};
use crate::theme::Tokens;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Ui};

#[derive(Default)]
pub(in crate::repo) struct Document {
    pub rows: Vec<LayoutJob>,
    segments: Vec<Segment>,
    bytes: usize,
    limited: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Segment {
    pub line: usize,
    pub bytes: std::ops::Range<usize>,
    pub chars: std::ops::Range<usize>,
}
impl Segment {
    fn job(&self, source: &LayoutJob) -> LayoutJob {
        let mut part = LayoutJob {
            text: source.text[self.bytes.clone()].to_owned(),
            ..Default::default()
        };
        let first = source
            .sections
            .partition_point(|s| s.byte_range.end.0 <= self.bytes.start);
        for section in source.sections[first..]
            .iter()
            .take_while(|s| s.byte_range.start.0 < self.bytes.end)
        {
            let (a, b) = (
                section.byte_range.start.0.max(self.bytes.start),
                section.byte_range.end.0.min(self.bytes.end),
            );
            if a < b {
                let mut section = section.clone();
                section.byte_range = egui::text::ByteIndex(a - self.bytes.start)
                    ..egui::text::ByteIndex(b - self.bytes.start);
                part.sections.push(section);
            }
        }
        part
    }
}
impl Document {
    /// Logical rows remain whole for matching. Only visible display segments are sliced.
    fn bound_rows(&mut self) {
        let limit = crate::repo::highlight::MAX_LINE_CHARS;
        for (line, job) in self.rows.iter().enumerate() {
            let (mut byte, mut chars, mut count) = (0, 0, 0);
            for (end, _) in job.text.char_indices() {
                if count == limit {
                    self.segments.push(Segment {
                        line,
                        bytes: byte..end,
                        chars: chars..chars + count,
                    });
                    byte = end;
                    chars += count;
                    count = 0;
                }
                count += 1;
            }
            self.segments.push(Segment {
                line,
                bytes: byte..job.text.len(),
                chars: chars..chars + count,
            });
        }
    }
    fn push(&mut self, job: LayoutJob) {
        // Reject an over-budget reading explicitly rather than allocating millions of
        // LayoutJobs or presenting a truncated document as complete search results.
        if self.limited {
            return;
        }
        self.bytes = self.bytes.saturating_add(job.text.len());
        if self.rows.len() >= 32768 || self.bytes > 20 << 20 {
            self.limited = true;
            self.rows.clear();
            return;
        }
        self.rows.push(job);
    }
    pub fn job(&mut self, mut job: LayoutJob) {
        job.text = job.text.replace(['\n', '\r'], " ");
        job.wrap.max_width = f32::INFINITY;
        self.push(job);
    }
    pub fn text(&mut self, text: &str, color: Color32) {
        for line in text.lines() {
            if self.limited {
                break;
            }
            let mut job = LayoutJob::default();
            job.append(
                line,
                0.0,
                TextFormat::simple(FontId::proportional(14.0), color),
            );
            self.push(job);
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
                doc.push(std::mem::take(job));
            }
        };
        for event in
            pulldown_cmark::Parser::new_ext(markdown, crate::pull_request::markdown_options())
        {
            if self.limited {
                break;
            }
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
                Event::FootnoteReference(text) => job.append(
                    &format!("[{text}]"),
                    0.0,
                    TextFormat::simple(FontId::proportional(14.0), color),
                ),
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
                crate::i18n::text("repo-core-2516"),
            )
            .small()
            .color(t.text_dim),
        );
        if doc.limited {
            self.drop_results();
            ui.label(crate::i18n::text("repo-core-2517"));
            self.rendered_document = Some((key, doc));
            return;
        }
        if let Some(hit) = self.current
            && let Some(row) = doc.rows.get(hit.line)
            && ui.small_button(crate::i18n::text("repo-core-2518")).clicked()
        {
            ui.ctx().copy_text(row.text.clone());
        }
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
            Layout::Rendered(&doc.segments),
        );
        let mut scroll = egui::ScrollArea::both()
            .id_salt(("find-rendered", &key))
            .auto_shrink(false);
        if let Some(hit) = self.take_reveal() {
            if let Some(segment) = doc.segments.get(hit.row) {
                let row = segment.job(&doc.rows[segment.line]);
                let galley = ui.fonts_mut(|f| f.layout_job(row));
                let left = galley
                    .pos_from_cursor(egui::text::CCursor::new(
                        hit.start.saturating_sub(segment.chars.start),
                    ))
                    .left()
                    + 12.0;
                let right = galley
                    .pos_from_cursor(egui::text::CCursor::new(
                        hit.end
                            .min(segment.chars.end)
                            .saturating_sub(segment.chars.start),
                    ))
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
        let result = scroll.show_rows(ui, 24.0, doc.segments.len(), |ui, range| {
            for index in range {
                let segment = &doc.segments[index];
                let galley = ui.fonts_mut(|f| f.layout_job(segment.job(&doc.rows[segment.line])));
                let marks = self.marks(segment.line);
                let ranges: Vec<_> = marks
                    .iter()
                    .flat_map(|m| m.ranges)
                    .filter_map(|r| {
                        let (a, b) = (
                            r.start.max(segment.chars.start),
                            r.end.min(segment.chars.end),
                        );
                        (a < b).then(|| a - segment.chars.start..b - segment.chars.start)
                    })
                    .collect();
                let current = self
                    .current
                    .filter(|h| h.line == segment.line)
                    .and_then(|hit| {
                        let (a, b) = (
                            hit.start.max(segment.chars.start),
                            hit.end.min(segment.chars.end),
                        );
                        (a < b)
                            .then(|| {
                                ranges.iter().position(|r| {
                                    *r == (a - segment.chars.start..b - segment.chars.start)
                                })
                            })
                            .flatten()
                    });
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(galley.size().x + 24.0), 24.0),
                    egui::Sense::hover(),
                );
                paint_line(
                    ui.painter(),
                    (rect.left() + 12.0, rect.center().y, 24.0),
                    galley,
                    (!ranges.is_empty()).then_some(super::LineMarks {
                        ranges: &ranges,
                        current,
                    }),
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
        assert_eq!(doc.rows.len(), 1);
        assert_eq!(doc.segments.len(), 2);
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

#[cfg(test)]
mod revision_tests {
    use super::*;
    use crate::repo::find::Query;
    fn search(text: &str, query: Query) -> Find {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut find = Find {
            open: true,
            query,
            ..Default::default()
        };
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                find.rendered(ui, "logical-source".into(), |t| {
                    let mut doc = Document::default();
                    doc.text(text, t.text);
                    doc
                })
            });
            output.drop_without_applying_deltas();
        }
        find
    }
    #[test]
    fn logical_matches_cross_visual_cuts_without_inventing_regex_or_word_boundaries() {
        let text = format!("{}needlex", "é".repeat(4997));
        let find = search(
            &text,
            Query {
                text: "needle".into(),
                ..Default::default()
            },
        );
        assert_eq!(find.count(), 1);
        assert_eq!(
            find.current().map(|h| (h.start, h.end, h.row)),
            Some((4997, 5003, 0))
        );
        let find = search(
            &text,
            Query {
                text: "needle".into(),
                whole_word: true,
                ..Default::default()
            },
        );
        assert_eq!(find.count(), 0);
        let text = format!("{}needle", "x".repeat(5000));
        let find = search(
            &text,
            Query {
                text: "needle".into(),
                whole_word: true,
                ..Default::default()
            },
        );
        assert_eq!(find.count(), 0);
        let find = search(
            &text,
            Query {
                text: "^needle".into(),
                regex: true,
                ..Default::default()
            },
        );
        assert_eq!(find.count(), 0);
        let text = format!("{}needle{}", "x".repeat(4994), "x".repeat(10));
        let find = search(
            &text,
            Query {
                text: "needle$".into(),
                regex: true,
                ..Default::default()
            },
        );
        assert_eq!(find.count(), 0);
        let find = search(
            &format!("{} needle", "é".repeat(10005)),
            Query {
                text: "needle".into(),
                ..Default::default()
            },
        );
        assert_eq!(find.current().map(|h| (h.start, h.row)), Some((10006, 2)));
    }
    #[test]
    fn near_limit_many_line_documents_refuse_explicitly_without_retaining_millions_of_jobs() {
        let source = "x\n".repeat(4 << 20);
        let mut doc = Document::default();
        doc.text(&source, Color32::WHITE);
        assert!(doc.limited);
        assert!(doc.rows.is_empty());
        let ctx = egui::Context::default();
        let mut find = Find {
            open: true,
            query: Query {
                text: "x".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            find.rendered(ui, "bounded".into(), |_| std::mem::take(&mut doc))
        });
        let text = output
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
            .join("\n");
        assert!(text.contains("no partial result count"), "{text}");
        assert_eq!(find.count(), 0);
        output.drop_without_applying_deltas();
    }
}
