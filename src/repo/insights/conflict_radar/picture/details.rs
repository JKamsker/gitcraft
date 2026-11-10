//! Conflict pair table, selection details, and affected path regions.

use super::*;

/// Every pair, clickable.
pub(in crate::repo::insights::conflict_radar) fn table(
    ui: &mut Ui,
    radar: &Radar,
    moved: &HashSet<usize>,
    selected: Option<Picked>,
    acts: &mut Vec<Action>,
) {
    let widths = [180.0, 180.0, 90.0, 90.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Branch".into(),
            "With".into(),
            "Conflicts".into(),
            "Overlap".into(),
            "Read".into(),
        ],
        true,
        false,
    );
    egui::ScrollArea::vertical()
        .id_salt("conflict-radar-table")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, e) in radar.edges.iter().enumerate() {
                let stale = moved.contains(&e.a) || moved.contains(&e.b);
                let conflicts = match e.result.merge_error() {
                    Some(_) => "not known".to_owned(),
                    None => e.result.conflicts().len().to_string(),
                };
                let resp = common::table_row(
                    ui,
                    &widths,
                    &[
                        radar.nodes[e.a].name.clone(),
                        radar.nodes[e.b].name.clone(),
                        conflicts,
                        e.result.overlap.len().to_string(),
                        read_word(e, stale).to_owned(),
                    ],
                    false,
                    selected == Some(Picked::Pair(i)),
                );
                if resp.clicked() {
                    acts.push(act(Act::SelectPair(i)));
                }
            }
            order(ui, radar, acts);
        });
}

/// The landing order, each branch clickable.
pub(super) fn order(ui: &mut Ui, radar: &Radar, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let Some(target) = radar.target() else {
        ui.label(
            RichText::new("No default branch found: no landing order to suggest.")
                .size(11.5)
                .color(t.text_faint),
        );
        return;
    };
    common::heading(
        ui,
        &format!("A possible order to land them on {}", target.name),
    );
    for (n, step) in radar.order.iter().enumerate() {
        let name = &radar.nodes[step.branch].name;
        let mut why = Vec::new();
        if step.with_target > 0 {
            why.push(format!(
                "{} files conflict with {}",
                step.with_target, target.name
            ));
        }
        for (earlier, files) in &step.with_earlier {
            why.push(format!(
                "{files} files conflict with {}",
                radar.nodes[*earlier].name
            ));
        }
        if !step.unknown.is_empty() {
            why.push("not known against some landed before".to_owned());
        }
        let text = if step.clean() {
            format!("{}. {name}: lands cleanly", n + 1)
        } else {
            format!("{}. {name}: {}", n + 1, why.join("; "))
        };
        if ui.small_button(text).clicked() {
            acts.push(act(Act::SelectBranch(step.branch)));
        }
    }
    ui.label(
        RichText::new(
            "From the dry-run merges: the most branches that conflict with none of each other \
             first, then the fewest files to resolve.",
        )
        .size(11.5)
        .color(t.text_faint),
    );
}

/// What is picked: a pair's files and regions, or a branch's pairs.
pub(in crate::repo::insights::conflict_radar) fn details(
    ui: &mut Ui,
    radar: &Radar,
    moved: &HashSet<usize>,
    picked: Picked,
    hunks: Option<&Vec<FileHunks>>,
    (loading, error): (bool, Option<&str>),
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    match picked {
        Picked::Branch(i) => {
            let Some(n) = radar.nodes.get(i) else {
                return;
            };
            common::heading(ui, &n.name);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("tip {}", short(&n.tip))).size(12.0));
                if ui.button("Show its tip commit").clicked() {
                    acts.push(Action::SelectHash(n.tip.clone()));
                }
                buttons(ui, acts);
            });
            ui.horizontal_wrapped(|ui| {
                for (k, e) in radar.edges.iter().enumerate() {
                    if (e.a == i || e.b == i) && e.shown() {
                        let other = &radar.nodes[if e.a == i { e.b } else { e.a }].name;
                        let text = format!("with {other}: {}", short_verdict(e));
                        if ui.small_button(text).clicked() {
                            acts.push(act(Act::SelectPair(k)));
                        }
                    }
                }
            });
            order(ui, radar, acts);
        }
        Picked::Pair(k) => {
            let Some(e) = radar.edges.get(k) else {
                return;
            };
            let (a, b) = (&radar.nodes[e.a], &radar.nodes[e.b]);
            common::heading(ui, &format!("{} and {}", a.name, b.name));
            let stale = moved.contains(&e.a) || moved.contains(&e.b);
            ui.label(RichText::new(verdict(e)).size(12.0));
            ui.label(
                RichText::new(format!(
                    "{} @ {} and {} @ {} · {}",
                    a.name,
                    short(&a.tip),
                    b.name,
                    short(&b.tip),
                    read_word(e, stale)
                ))
                .size(11.5)
                .color(if stale { t.danger } else { t.text_faint }),
            );
            if let Some(why) = &e.result.partial {
                ui.label(
                    RichText::new(format!("partial: {why}"))
                        .size(11.5)
                        .color(t.warning),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if let Some(base) = &e.result.base {
                    let counts = |tip: &str| e.result.changed.get(tip).copied().unwrap_or(0);
                    ui.label(
                        RichText::new(format!(
                            "merge-base {} · {} changed {} files, {} changed {}",
                            short(base),
                            a.name,
                            counts(&a.tip),
                            b.name,
                            counts(&b.tip)
                        ))
                        .size(11.5)
                        .color(t.text_dim),
                    );
                    if ui.small_button("Show the merge-base").clicked() {
                        acts.push(Action::SelectHash(base.clone()));
                    }
                } else {
                    ui.label(
                        RichText::new(
                            "No merge-base found: they share no history, or it couldn't be read \
                             here (a shallow clone); read again to retry",
                        )
                        .size(11.5)
                        .color(t.text_dim),
                    );
                }
                buttons(ui, acts);
            });
            let base = e.result.base.as_deref();
            if !e.result.conflicts().is_empty() {
                common::heading(ui, "Conflicted by a dry-run merge");
                for path in e.result.conflicts() {
                    path_row(ui, path, base, (&a.name, &a.tip), (&b.name, &b.tip), acts);
                    regions(ui, path, hunks, loading, error, (&a.name, &b.name));
                }
            }
            let only = e.result.overlap_only();
            if !only.is_empty() {
                common::heading(ui, "Changed on both, merging cleanly");
                for path in only {
                    path_row(ui, path, base, (&a.name, &a.tip), (&b.name, &b.tip), acts);
                }
            }
            order(ui, radar, acts);
        }
    }
}

fn short_verdict(e: &Edge) -> String {
    if e.result.merge_error().is_some() {
        "not known".to_owned()
    } else if e.result.conflicts().is_empty() {
        format!("{} overlapping", e.result.overlap.len())
    } else {
        format!("{} conflicting", e.result.conflicts().len())
    }
}

fn buttons(ui: &mut Ui, acts: &mut Vec<Action>) {
    if ui
        .button("Collision matrix")
        .on_hover_text("Every pair of the worktrees' branches, merged in memory")
        .clicked()
    {
        acts.push(Action::OpenCollisions);
    }
    if ui.button("Deselect (Esc)").clicked() {
        acts.push(act(Act::Deselect));
    }
}

/// A file, with a button for each side's diff of it since the merge-base.
pub(super) fn path_row(
    ui: &mut Ui,
    path: &str,
    base: Option<&str>,
    a: (&str, &str),
    b: (&str, &str),
    acts: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(path).font(theme::mono(12.0)));
        let Some(base) = base else {
            return;
        };
        for (name, tip) in [a, b] {
            if ui
                .small_button(format!("{name}'s change"))
                .on_hover_text(format!(
                    "{path} from the merge-base {} to {name} @ {}",
                    short(base),
                    short(tip)
                ))
                .clicked()
            {
                acts.push(Action::OpenDiff(DiffKey::Range {
                    from: base.to_owned(),
                    to: Some(tip.to_owned()),
                    path: path.to_owned(),
                }));
            }
        }
    });
}

/// The conflicted regions of `path`, as the dry-run merge wrote them.
pub(super) fn regions(
    ui: &mut Ui,
    path: &str,
    hunks: Option<&Vec<FileHunks>>,
    loading: bool,
    error: Option<&str>,
    (a, b): (&str, &str),
) {
    let t = Tokens::get(ui.ctx());
    let faint = |s: String| RichText::new(s).size(11.5).color(t.text_faint);
    let Some(file) = hunks.and_then(|h| h.iter().find(|f| f.path == path)) else {
        let said = match (loading, error) {
            (true, _) => "reading the conflicted regions…".to_owned(),
            (_, Some(e)) => format!("regions not read: {e}"),
            _ => "regions not read (past the files read for a pair)".to_owned(),
        };
        ui.label(faint(said));
        return;
    };
    match &file.hunks {
        Err(e) => {
            ui.label(faint(format!("regions not read: {e}")));
        }
        Ok(list) if list.is_empty() => {
            ui.label(faint(
                "no conflicted text (a file deleted or renamed on one side, or binary)".to_owned(),
            ));
        }
        Ok(list) => {
            for h in list {
                let mut text = format!("line {}\n<<<<<<< {a}\n", h.line);
                for l in &h.ours {
                    text.push_str(l);
                    text.push('\n');
                }
                text.push_str("=======\n");
                for l in &h.theirs {
                    text.push_str(l);
                    text.push('\n');
                }
                text.push_str(&format!(">>>>>>> {b}"));
                if h.cut {
                    text.push_str("\n(lines past the first ones left out)");
                }
                egui::Frame::new()
                    .fill(t.field)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(text).font(theme::mono(11.5)).color(t.text),
                            )
                            .wrap(),
                        );
                    });
                ui.add_space(2.0);
            }
            if file.more {
                ui.label(faint("more regions than shown".to_owned()));
            }
        }
    }
    ui.add_space(2.0);
}
