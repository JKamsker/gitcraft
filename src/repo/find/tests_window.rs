struct Window {
    ctx: egui::Context,
    tab: RepoTab,
    /// What the last frame painted.
    shapes: Vec<egui::epaint::ClippedShape>,
}

impl Window {
    fn open(dir: &Path) -> Self {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let tab = RepoTab::new(dir.to_path_buf(), &ctx);
        Self {
            ctx,
            tab,
            shapes: Vec::new(),
        }
    }

    /// A frame with `events`, `modifiers` held down.
    fn frame(&mut self, events: Vec<Event>, modifiers: Modifiers) {
        let mut events = events;
        events.insert(0, Event::ModifiersChanged(modifiers));
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let tab = &mut self.tab;
        let mut out = self.ctx.run_ui(input, |ui| tab.ui(ui));
        self.shapes = std::mem::take(&mut out.shapes);
        out.drop_without_applying_deltas();
    }

    /// Rectangles of the last frame filled with `color`.
    fn filled(&self, color: egui::Color32) -> usize {
        self.shapes
            .iter()
            .filter(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == color))
            .count()
    }

    /// Draws frames until `done` (the background reads come in meanwhile); whether it was.
    fn until(&mut self, done: impl Fn(&RepoTab) -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            self.frame(Vec::new(), Modifiers::NONE);
            if done(&self.tab) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// A key pressed and let go, then a frame without it.
    fn key(&mut self, key: Key, modifiers: Modifiers) {
        let event = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        self.frame(vec![event(true), event(false)], modifiers);
        self.frame(Vec::new(), Modifiers::NONE);
    }

    fn type_text(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_owned())], Modifiers::NONE);
        self.frame(Vec::new(), Modifiers::NONE);
    }

    /// The current match: its line and first character.
    fn current(&self) -> Option<(usize, usize)> {
        self.tab.find.current().map(|h| (h.line, h.start))
    }
}

#[test]
fn ctrl_f_finds_in_the_diff_file_or_blame_shown_and_in_the_commits_otherwise() -> Result<(), String>
{
    let dir = repo("keys")?;
    let hash = head(&dir)?;
    let mut w = Window::open(&dir);
    let opened = w.until(|t| !t.opening() && t.settled());

    // The graph: Ctrl+F is the commit search.
    w.key(Key::F, Modifiers::COMMAND);
    let graph_search = (w.tab.search.open, w.tab.find.open);
    w.key(Key::Escape, Modifiers::NONE);
    let graph_closed = !w.tab.search.open;

    // A diff: the find bar, the commit search left alone.
    w.tab.queue(Action::OpenDiff(DiffKey::Commit {
        hash: hash.clone(),
        path: PATH.to_owned(),
    }));
    let diff_read = w.until(|t| matches!(&t.diff, Some(d) if matches!(d.data, Some(Ok(_)))));
    w.key(Key::F, Modifiers::COMMAND);
    let in_diff = (w.tab.find.open, w.tab.search.open);
    w.type_text("e");
    let diff_count = w.tab.find.count();
    let first = w.current();
    // Marked: the current match strongly, the others softly.
    let warning = crate::theme::Tokens::get(&w.ctx).warning;
    let painted = (
        w.filled(warning.gamma_multiply(0.6)),
        w.filled(warning.gamma_multiply(0.28)),
    );
    // Enter in the field steps on, F3 too, Shift+F3 back.
    w.key(Key::Enter, Modifiers::NONE);
    let after_enter = w.current();
    w.key(Key::F3, Modifiers::NONE);
    let after_f3 = w.current();
    w.key(Key::F3, Modifiers::SHIFT);
    let after_shift_f3 = w.current();
    // Esc closes the bar, not the diff; the next one closes the diff.
    w.key(Key::Escape, Modifiers::NONE);
    let bar_closed = (!w.tab.find.open, w.tab.diff.is_some());
    w.key(Key::Escape, Modifiers::NONE);
    let diff_closed = w.tab.diff.is_none();

    // The blame: Ctrl+F again finds in it, with the text typed before.
    w.tab.queue(Action::FileView {
        path: PATH.to_owned(),
        rev: None,
        tab: FileTab::Blame,
    });
    let blamed = w.until(|t| matches!(&t.file_view, Some(v) if matches!(v.blame, Some(Ok(_)))));
    w.key(Key::F, Modifiers::COMMAND);
    w.until(|t| t.find.summary().is_some());
    let in_blame = (
        w.tab.find.open,
        w.tab.find.query.text.clone(),
        w.tab.find.count(),
    );
    // The file: the same, from its own lines.
    w.tab.queue(Action::FileView {
        path: PATH.to_owned(),
        rev: None,
        tab: FileTab::File,
    });
    let read = w.until(|t| matches!(&t.file_view, Some(v) if matches!(v.text, Some(Ok(_)))));
    // Ctrl+F selects what was typed: typing replaces it.
    w.key(Key::F, Modifiers::COMMAND);
    w.type_text("omega");
    let in_file = w.tab.find.count();
    // History has nothing to search: back to the commit search.
    w.tab.queue(Action::FileView {
        path: PATH.to_owned(),
        rev: None,
        tab: FileTab::History,
    });
    w.frame(Vec::new(), Modifiers::NONE);
    w.frame(Vec::new(), Modifiers::NONE);
    let left = !w.tab.find.open;
    let _ = std::fs::remove_dir_all(&dir);

    assert!(opened, "the repository never opened");
    assert_eq!(graph_search, (true, false));
    assert!(graph_closed);
    assert!(diff_read, "the diff never came");
    assert_eq!(in_diff, (true, false));
    // "e": "Grüße" and "GRÜSSE" (1 each), "needle b" and "Needle A2" (3 each), "omega" (1).
    assert_eq!(diff_count, 9);
    assert!(first.is_some());
    assert_eq!(painted, (1, diff_count - 1));
    assert_ne!(after_enter, first);
    assert_ne!(after_f3, after_enter);
    assert_eq!(after_shift_f3, after_enter);
    assert_eq!(bar_closed, (true, true));
    assert!(diff_closed);
    assert!(blamed, "the blame never came");
    // HEAD's file: "GRÜSSE" (1), "Needle A2" (3), "plain B2" (0), "omega" (1).
    assert_eq!(in_blame, (true, "e".to_owned(), 5));
    assert!(read, "the file never came");
    assert_eq!(in_file, 1);
    assert!(left);
    Ok(())
}
