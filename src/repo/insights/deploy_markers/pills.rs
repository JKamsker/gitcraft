//! The environments' pills on a commit row: outlined (a branch's pill is filled), with a small
//! deploy triangle, faint when stale and dashed when unknown. Hover tells where the record
//! came from and how old it is; the menu compares environments.

use egui::{Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::super::InsightsAct;
use super::model::{Marker, Standing};
use super::tooltip::{heading, tooltip};
use crate::i18n::{message, text};
use crate::menu_item::MenuItem;
use crate::repo::{Action, RepoTab, WIP_HASH};
use crate::theme::{self, Tokens};

/// So many named environments get a pill of their own on a row; the rest are collapsed.
const MAX_PILLS: usize = 2;
/// So many characters of an environment's name are shown on its pill.
const MAX_LABEL: usize = 14;
const PILL_H: f32 = 16.0;
const GAP: f32 = 4.0;

/// One pill: one marker, or several collapsed.
enum Pill<'a> {
    One(&'a Marker),
    Many(Vec<&'a Marker>),
}

impl RepoTab {
    /// Paints the environments' pills on commit `hash`'s row from `left`, centred on `y`;
    /// returns the width they take.
    pub(in crate::repo) fn deploy_marker_pills(
        &self,
        ui: &mut Ui,
        left: f32,
        y: f32,
        hash: &str,
        acts: &mut Vec<Action>,
    ) -> f32 {
        let state = &self.deploy_markers;
        if !state.visible || state.markers.is_empty() || self.ghost.drawing() {
            return 0.0;
        }
        let (named, previews) = on_commit(&state.markers, hash);
        let split = named.len().min(MAX_PILLS);
        let rest: Vec<&Marker> = named[split..].iter().chain(&previews).copied().collect();
        let mut pills: Vec<Pill> = named[..split].iter().map(|m| Pill::One(*m)).collect();
        if !rest.is_empty() {
            pills.push(Pill::Many(rest));
        }
        let now = crate::git::now();
        let mut x = left;
        for (n, pill) in pills.iter().enumerate() {
            let resp = self.deploy_pill(ui, pos2(x, y), (hash, n), pill, now);
            x = resp.rect.right() + GAP;
            if resp.clicked() {
                acts.push(Action::SelectHash(hash.to_owned()));
            }
            crate::menu_item::context(&resp)
                .style(theme::menu_style)
                .show(|ui| match pill {
                    Pill::One(m) => self.deploy_marker_menu(ui, m, acts),
                    Pill::Many(list) => {
                        for m in list {
                            ui.menu_button(&m.environment, |ui| {
                                self.deploy_marker_menu(ui, m, acts);
                            });
                        }
                    }
                });
        }
        x - left
    }

    fn deploy_pill(
        &self,
        ui: &mut Ui,
        left_center: Pos2,
        (hash, n): (&str, usize),
        pill: &Pill,
        now: i64,
    ) -> Response {
        let state = &self.deploy_markers;
        let stale = state.read_stale(now);
        let (label, standing) = match pill {
            Pill::One(m) => (
                pill_label(m, m.standing(now, stale)),
                m.standing(now, stale),
            ),
            Pill::Many(list) => (
                collapsed_label(list),
                // The group is as trustworthy as its least trustworthy marker.
                list.iter()
                    .map(|m| m.standing(now, stale))
                    .max_by_key(|s| rank(*s))
                    .unwrap_or(Standing::Unknown),
            ),
        };
        let t = Tokens::get(ui.ctx());
        let rect = paint(ui, &t, left_center, &label, standing);
        let id = ui.id().with(("deploy-marker", hash, n));
        let resp = ui.interact(rect, id, Sense::click());
        let spoken = label.clone();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &spoken));
        resp.on_hover_ui(|ui| match pill {
            Pill::One(m) => {
                for line in tooltip(m, m.standing(now, stale), state, now) {
                    ui.label(line);
                }
            }
            Pill::Many(list) => {
                for m in list {
                    ui.label(heading(m, m.standing(now, stale)));
                }
                ui.label(text("graph-deployments-evidence"));
            }
        })
    }

    /// A marker's menu: select its commit, compare it with another environment's, ask whether
    /// the selected commit reached it, open it on the forge.
    fn deploy_marker_menu(&self, ui: &mut Ui, m: &Marker, acts: &mut Vec<Action>) {
        if MenuItem::new(text("graph-deployments-menu-select"))
            .icon("command")
            .show(ui)
            .clicked()
        {
            acts.push(Action::SelectHash(m.sha().to_owned()));
        }
        let here = m.present != Some(false);
        for other in &self.deploy_markers.markers {
            if other.environment == m.environment || other.sha() == m.sha() {
                continue;
            }
            let label = message(
                "graph-deployments-menu-compare",
                &[
                    ("environment", m.environment.clone().into()),
                    ("other", other.environment.clone().into()),
                ],
            );
            let missing = !here || other.present == Some(false);
            if MenuItem::new(label)
                .icon("file-diff")
                .refused(missing.then(|| text("graph-deployments-not-here")))
                .show(ui)
                .on_hover_text(text("graph-deployments-menu-compare-tip"))
                .clicked()
            {
                acts.push(compare(m, other));
            }
        }
        if let Some(commit) = self.deploy_reached_subject(m) {
            let label = message(
                "graph-deployments-menu-reached",
                &[
                    ("commit", crate::repo::short(&commit).to_owned().into()),
                    ("environment", m.environment.clone().into()),
                ],
            );
            if MenuItem::new(label)
                .icon("command")
                .show(ui)
                .on_hover_text(text("graph-deployments-menu-reached-tip"))
                .clicked()
            {
                acts.push(Action::Insights(InsightsAct::DeployMarkers(
                    super::Act::Reached {
                        commit,
                        environment: m.environment.clone(),
                    },
                )));
            }
        }
        if m.deployment.url.starts_with("https://")
            && MenuItem::new(text("graph-deployments-menu-open"))
                .icon("command")
                .show(ui)
                .clicked()
        {
            acts.push(Action::OpenUrl(m.deployment.url.clone()));
        }
    }

    /// The selected commit, when one is selected that isn't the marker's own.
    fn deploy_reached_subject(&self, m: &Marker) -> Option<String> {
        let commit = self.commits.get(self.selected?)?;
        let real = commit.hash != WIP_HASH && self.stash_at(self.selected?).is_none();
        (real && !m.on(&commit.hash)).then(|| commit.hash.clone())
    }
}

/// The markers on commit `hash`: named environments, then previews.
pub(super) fn on_commit<'a>(
    markers: &'a [Marker],
    hash: &str,
) -> (Vec<&'a Marker>, Vec<&'a Marker>) {
    markers
        .iter()
        .filter(|m| m.on(hash))
        .partition(|m| !m.preview)
}

/// The range diff between two environments' commits, `base` first.
pub(super) fn compare(base: &Marker, target: &Marker) -> Action {
    Action::Compare {
        base: base.sha().to_owned(),
        base_label: base.environment.clone(),
        target: Some(target.sha().to_owned()),
        target_label: target.environment.clone(),
        captions: [None, None],
        since_fork: false,
    }
}

fn rank(standing: Standing) -> u8 {
    match standing {
        Standing::Current => 0,
        Standing::Stale => 1,
        Standing::Unknown => 2,
    }
}

pub(super) fn pill_label(m: &Marker, standing: Standing) -> String {
    let mut name: String = m.environment.chars().take(MAX_LABEL).collect();
    if m.environment.chars().count() > MAX_LABEL {
        name.push('…');
    }
    if standing == Standing::Unknown {
        name.push_str(" ?");
    }
    name
}

fn collapsed_label(list: &[&Marker]) -> String {
    let count = list.len() as i64;
    let id = if list.iter().all(|m| m.preview) {
        "graph-deployments-previews"
    } else {
        "graph-deployments-more"
    };
    message(id, &[("count", count.into())])
}

/// Paints a pill from `left_center`: outlined, with a deploy triangle; returns its rect.
fn paint(ui: &Ui, t: &Tokens, left_center: Pos2, label: &str, standing: Standing) -> Rect {
    let (color, ink) = match standing {
        Standing::Current => (t.success, t.text),
        Standing::Stale => (t.text_faint, t.text_dim),
        Standing::Unknown => (t.text_faint, t.text_faint),
    };
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(label.to_owned(), theme::medium(10.5), ink);
    let w = galley.size().x + 22.0;
    let rect = Rect::from_min_size(
        pos2(left_center.x, left_center.y - PILL_H / 2.0),
        vec2(w, PILL_H),
    );
    let stroke = Stroke::new(1.0, color);
    match standing {
        Standing::Current => {
            painter.rect_filled(rect, 4.0, t.text_tint(color, 0.14, t.dock));
            painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);
        }
        Standing::Stale => {
            painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);
        }
        Standing::Unknown => {
            let r = rect.shrink(0.5);
            let corners = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
            ];
            let path: Vec<Pos2> = corners.iter().chain(&corners[..1]).copied().collect();
            painter.extend(Shape::dashed_line(&path, stroke, 3.0, 2.0));
        }
    }
    let c = pos2(rect.left() + 9.0, rect.center().y);
    let triangle = vec![c + vec2(0.0, -4.0), c + vec2(4.0, 3.0), c + vec2(-4.0, 3.0)];
    if standing == Standing::Current {
        painter.add(Shape::convex_polygon(triangle, color, Stroke::NONE));
    } else {
        painter.add(Shape::closed_line(triangle, stroke));
    }
    painter.galley(
        pos2(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    rect
}
