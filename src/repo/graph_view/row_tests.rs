use super::*;

#[test]
fn painted_graph_bodies_and_wip_chips_meet_aa_in_combined_row_states() {
    let ctx = theme::test_paint::context();
    for builtin in theme::catalog::BUILTIN {
        let t = builtin.palette().tokens();
        theme::set_tokens(&ctx, t);
        let mut tab = RepoTab::new_deferred("/nonexistent/contrast-paint".into(), &ctx);
        let file = git::StatusFile {
            path: "file".into(),
            old_path: None,
            code: 'M',
            area: git::Area::Unstaged,
        };
        tab.work.unstaged.push(file.clone());
        tab.work.staged.push(file.clone());
        tab.work.conflicted.push(file);
        let commit = Commit {
            summary: "Subject".into(),
            body: "faintbody".into(),
            ..Default::default()
        };
        for state in 0..4 {
            let mut output = ctx.run_ui(Default::default(), |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(t.dock))
                    .show(ctx, |ui| {
                        let painter = ui.painter();
                        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(700.0, ROW_PITCH));
                        let mut row = Row {
                            i: 0,
                            commit: &commit,
                            rect,
                            cols: Cols {
                                refs: 0.0,
                                graph: 0.0,
                                hash: 0.0,
                                author: 0.0,
                                when: 0.0,
                                date: 0.0,
                                push: 0.0,
                            },
                            gx: 0.0,
                            lane_w: 0.0,
                            node_x: None,
                            color: t.accent,
                        };
                        row_background(
                            painter,
                            &t,
                            row.rect,
                            state == 2,
                            state == 1,
                            state == 3,
                            false,
                        );
                        description(ui, painter, &t, &row, (state == 3, false), 0.0, 0.0);
                        row.rect = row.rect.translate(vec2(0.0, ROW_PITCH));
                        let background = row_background(
                            painter,
                            &t,
                            row.rect,
                            state == 2,
                            state == 1,
                            state == 3,
                            false,
                        );
                        tab.wip_description(painter, &t, &row, background);
                    });
            });
            output.textures_delta.clear();
            for label in ["faintbody", "1 conflict", "1 staged", "1 unstaged"] {
                theme::test_paint::assert_text_aa(
                    &output.shapes,
                    t.dock,
                    label,
                    &format!("{} state {state}", builtin.name),
                );
            }
        }
    }
}
