//! Bounded, deterministic history columns. Preferences never contain viewport compression.
//! Description consumes spare space first; graph, refs, then trailing cells give up slack.
//! Below the sum of minima the entire table scrolls horizontally.

use egui::{Rect, pos2};
use serde::{Deserialize, Serialize};

pub(crate) const MIN_REFS_W: f32 = 70.0;
pub(super) const MIN_GRAPH_W: f32 = 60.0;
pub(super) const MIN_COL_W: f32 = 50.0;
pub(crate) const MIN_DESCRIPTION_W: f32 = 140.0;
pub(crate) const CELL_PAD: f32 = 6.0;

/// An explicit Description allocation from a manual gesture, separate from column widths.
/// At smaller viewports Description gives back this space first. No viewport visit mutates it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DescriptionSpace {
    pub viewport: f32,
    pub width: f32,
    /// Gaps between the gesture's effective preferences and displayed widths, in Bar::ALL
    /// order. These preserve the allocation shape without saving compression as preferences.
    /// None retains the scalar policy used by older saved settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<[f32; 6]>,
}
impl DescriptionSpace {
    pub fn exchanged(available: f32, prefs: &Cols, shown: &Cols, bar: Bar) -> Self {
        Self {
            viewport: available,
            width: (available - shown.refs - shown.graph - shown.trailing()).max(MIN_DESCRIPTION_W),
            compression: Some(Bar::ALL.map(|other| {
                if other == bar || prefs.width(other) == 0.0 {
                    // The dragged column's new manual preference equals its displayed width.
                    // Hidden columns make no claim on this allocation.
                    0.0
                } else {
                    (other.bounded(prefs.width(other)) - shown.width(other)).max(0.0)
                }
            })),
        }
    }

    pub fn floor(self, available: f32) -> f32 {
        if !self.viewport.is_finite() || !self.width.is_finite() {
            return MIN_DESCRIPTION_W;
        }
        (self.width - (self.viewport - available).max(0.0)).max(MIN_DESCRIPTION_W)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cols {
    pub refs: f32,
    pub graph: f32,
    pub hash: f32,
    pub author: f32,
    pub when: f32,
    pub date: f32,
    /// Compatibility with old saved settings; cascading resize is no longer used.
    pub push: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Bar {
    Refs,
    Graph,
    Hash,
    Author,
    When,
    Date,
}

impl Bar {
    pub const ALL: [Self; 6] = [
        Self::Refs,
        Self::Graph,
        Self::Hash,
        Self::Author,
        Self::When,
        Self::Date,
    ];
    pub fn min(self) -> f32 {
        match self {
            Self::Refs => MIN_REFS_W,
            Self::Graph => MIN_GRAPH_W,
            Self::Author => 80.0,
            _ => MIN_COL_W,
        }
    }
    pub fn max(self) -> f32 {
        match self {
            Self::Refs => 2000.0,
            Self::Graph => 2000.0,
            Self::Author => 800.0,
            _ => 400.0,
        }
    }
    pub fn bounded(self, width: f32) -> f32 {
        if width.is_finite() {
            width.clamp(self.min(), self.max())
        } else {
            self.min()
        }
    }
    /// A drag is measured from its starting snapshot, never from last frame's compression.
    #[cfg(test)]
    pub fn drag(self, prefs: &Cols, _last: Option<Bar>, dx: f32, available: f32) -> Cols {
        self.drag_shown(prefs, &prefs.squeezed(available, None), dx, available)
    }
    pub fn drag_shown(self, prefs: &Cols, shown: &Cols, dx: f32, available: f32) -> Cols {
        let sign = if matches!(self, Self::Refs | Self::Graph) {
            1.0
        } else {
            -1.0
        };
        let mut next = *prefs;
        if dx.is_finite() && dx != 0.0 {
            let spare =
                (available - shown.refs - shown.graph - shown.trailing() - MIN_DESCRIPTION_W)
                    .max(0.0);
            let max = (shown.width(self) + spare).min(self.max());
            let width = self.bounded(shown.width(self) + sign * dx).min(max);
            // A blocked gesture must not turn temporary compression into a manual width.
            if width != shown.width(self) {
                *next.width_mut(self) = width;
            }
        }
        next.push = 0.0;
        next
    }
}

impl Cols {
    pub fn trailing(&self) -> f32 {
        self.hash + self.author + self.when + self.date
    }
    pub fn width(&self, bar: Bar) -> f32 {
        match bar {
            Bar::Refs => self.refs,
            Bar::Graph => self.graph,
            Bar::Hash => self.hash,
            Bar::Author => self.author,
            Bar::When => self.when,
            Bar::Date => self.date,
        }
    }
    fn width_mut(&mut self, bar: Bar) -> &mut f32 {
        match bar {
            Bar::Refs => &mut self.refs,
            Bar::Graph => &mut self.graph,
            Bar::Hash => &mut self.hash,
            Bar::Author => &mut self.author,
            Bar::When => &mut self.when,
            Bar::Date => &mut self.date,
        }
    }
    pub fn minimum_width(&self) -> f32 {
        MIN_DESCRIPTION_W
            + Bar::ALL
                .into_iter()
                .filter(|&b| self.width(b) > 0.0)
                .map(Bar::min)
                .sum::<f32>()
    }
    pub fn content_width(&self, available: f32) -> f32 {
        available.max(self.minimum_width())
    }
    #[cfg(test)]
    pub fn squeezed(self, available: f32, _last: Option<Bar>) -> Self {
        self.allocated(available, DescriptionSpace::default())
    }
    pub fn allocated(self, available: f32, description: DescriptionSpace) -> Self {
        let mut out = Self { push: 0.0, ..self };
        for bar in Bar::ALL {
            let w = out.width_mut(bar);
            if *w != 0.0 {
                *w = bar.bounded(*w);
            }
        }
        let preferred = out;
        let floor = description
            .floor(available)
            .min((available - out.minimum_width() + MIN_DESCRIPTION_W).max(MIN_DESCRIPTION_W));
        if let Some(compression) = description.compression.filter(|gaps| {
            description.viewport.is_finite()
                && description.viewport > 0.0
                && description.width.is_finite()
                && description.width >= MIN_DESCRIPTION_W
                && gaps.iter().all(|gap| gap.is_finite() && *gap >= 0.0)
        }) {
            for (bar, gap) in Bar::ALL.into_iter().zip(compression) {
                let w = out.width_mut(bar);
                if *w > 0.0 {
                    *w = (*w - gap).max(bar.min());
                }
            }
            // Recover compression as the viewport grows (or automatic fitting/visibility
            // frees space), in the reverse of the fixed squeeze order. Never prioritize a
            // column because it was last dragged. Any remaining space belongs to Description.
            let mut spare = (available - floor - out.refs - out.graph - out.trailing()).max(0.0);
            for bar in SQUEEZE_ORDER.into_iter().rev() {
                let w = out.width_mut(bar);
                let take = spare.min(preferred.width(bar) - *w);
                *w += take;
                spare -= take;
            }
        }
        let mut overflow = (out.refs + out.graph + out.trailing() + floor - available).max(0.0);
        for bar in SQUEEZE_ORDER {
            let w = out.width_mut(bar);
            let take = overflow.min((*w - bar.min()).max(0.0));
            *w -= take;
            overflow -= take;
        }
        out
    }
    /// Shared boundaries: header, cell paint/hover clipping and divider interaction.
    pub fn cell(&self, rect: Rect, bar: Bar) -> Rect {
        let left = match bar {
            Bar::Refs => rect.left(),
            Bar::Graph => rect.left() + self.refs,
            Bar::Hash => rect.right() - self.trailing(),
            Bar::Author => rect.right() - self.author - self.when - self.date,
            Bar::When => rect.right() - self.when - self.date,
            Bar::Date => rect.right() - self.date,
        };
        Rect::from_min_max(
            pos2(left, rect.top()),
            pos2(left + self.width(bar), rect.bottom()),
        )
    }
    pub fn description(&self, rect: Rect) -> Rect {
        Rect::from_min_max(
            pos2(rect.left() + self.refs + self.graph, rect.top()),
            pos2(rect.right() - self.trailing(), rect.bottom()),
        )
    }
    pub fn border(&self, rect: Rect, bar: Bar) -> f32 {
        let cell = self.cell(rect, bar);
        if matches!(bar, Bar::Refs | Bar::Graph) {
            cell.right()
        } else {
            cell.left()
        }
    }
    pub fn padded(cell: Rect) -> Rect {
        cell.shrink2(egui::vec2(CELL_PAD, 0.0))
    }
}

const SQUEEZE_ORDER: [Bar; 6] = [
    Bar::Graph,
    Bar::Refs,
    Bar::Hash,
    Bar::Author,
    Bar::When,
    Bar::Date,
];

#[cfg(test)]
mod tests {
    use super::*;
    fn prefs() -> Cols {
        Cols {
            refs: 330.0,
            graph: 100.0,
            hash: 70.0,
            author: 150.0,
            when: 100.0,
            date: 118.0,
            push: 0.0,
        }
    }
    #[test]
    fn compression_is_reversible_and_independent_of_drag_history() {
        for width in [100.0, 400.0, 800.0, 1800.0] {
            let expected = prefs().squeezed(width, None);
            for bar in Bar::ALL {
                assert_eq!(expected, prefs().squeezed(width, Some(bar)));
            }
        }
        assert_eq!(prefs().squeezed(1800.0, None), prefs());
    }
    #[test]
    fn a_different_divider_in_a_narrow_window_never_saves_compression() {
        let initial = prefs();
        for bar in Bar::ALL {
            let next = bar.drag(&initial, Some(Bar::Date), 30.0, 600.0);
            for other in Bar::ALL {
                if other != bar {
                    assert_eq!(next.width(other), initial.width(other));
                }
            }
            assert_eq!(next.squeezed(1800.0, None).width(bar), next.width(bar));
        }
    }
    #[test]
    fn geometry_tiles_the_table_at_every_width_and_scale() {
        for scale in [1.0, 1.25, 1.5] {
            for width in [380.0, 900.0, 1800.0] {
                let available = width / scale;
                let cols = prefs().squeezed(available, None);
                let rect = Rect::from_min_size(
                    pos2(-80.0, 20.0),
                    egui::vec2(cols.content_width(available), 30.0),
                );
                let cells = [
                    cols.cell(rect, Bar::Refs),
                    cols.cell(rect, Bar::Graph),
                    cols.description(rect),
                    cols.cell(rect, Bar::Hash),
                    cols.cell(rect, Bar::Author),
                    cols.cell(rect, Bar::When),
                    cols.cell(rect, Bar::Date),
                ];
                assert!(cells[2].width() >= MIN_DESCRIPTION_W);
                for pair in cells.windows(2) {
                    assert_eq!(pair[0].right(), pair[1].left());
                }
                assert_eq!(cells[6].right(), rect.right());
            }
        }
    }
    #[test]
    fn a_blocked_drag_keeps_the_compressed_columns_preferred_width() {
        let initial = prefs();
        for bar in Bar::ALL {
            for dx in [-100.0, 100.0] {
                assert_eq!(bar.drag(&initial, None, dx, 100.0), initial);
            }
        }
    }

    #[test]
    fn limits_hidden_columns_and_corrupt_settings_are_bounded() {
        let mut p = prefs();
        p.author = 0.0;
        p.graph = f32::NAN;
        p.refs = f32::INFINITY;
        let shown = p.squeezed(300.0, None);
        assert_eq!(shown.author, 0.0);
        assert!(shown.graph.is_finite());
        assert!(shown.refs.is_finite());
        let next = Bar::When.drag(&prefs(), None, 10000.0, 1800.0);
        assert_eq!(next.when, Bar::When.min());
        assert_eq!(next.date, prefs().date);
    }
}

#[cfg(test)]
mod exchange_tests {
    use super::*;
    #[test]
    fn released_space_does_not_restore_an_unrelated_compressed_preference() -> Result<(), String> {
        let initial = Cols {
            refs: 280.0,
            graph: 100.0,
            hash: 0.0,
            author: 150.0,
            when: 100.0,
            date: 0.0,
            push: 0.0,
        };
        let shown = initial.allocated(600.0, DescriptionSpace::default());
        assert_eq!((shown.refs, shown.graph), (150.0, 60.0));
        let saved = Bar::Refs.drag_shown(&initial, &shown, -20.0, 600.0);
        let space = DescriptionSpace {
            viewport: 600.0,
            width: 160.0,
            ..Default::default()
        };
        let after = saved.allocated(600.0, space);
        assert_eq!(
            (after.refs, after.graph, after.author, after.when),
            (130.0, 60.0, 150.0, 100.0)
        );
        assert_eq!(saved.graph, initial.graph);
        assert_eq!(saved.allocated(580.0, space).graph, 60.0);
        assert_eq!(space.floor(580.0), 140.0);
        assert_eq!(saved.allocated(640.0, space).graph, 100.0);
        let encoded = ron::to_string(&space).map_err(|e| e.to_string())?;
        let round_trip: DescriptionSpace = ron::from_str(&encoded).map_err(|e| e.to_string())?;
        for width in [1800.0, 380.0, 580.0, 600.0, 420.0, 600.0] {
            assert_eq!(
                saved.allocated(width, space),
                saved.allocated(width, round_trip)
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod snapshot_allocation_tests {
    use super::*;
    #[test]
    fn every_divider_preserves_all_other_displayed_allocations_on_release() {
        let prefs = Cols {
            refs: 330.0,
            graph: 100.0,
            hash: 70.0,
            author: 150.0,
            when: 100.0,
            date: 118.0,
            push: 0.0,
        };
        for available in [500.0, 638.0, 850.0, 1000.0, 1800.0] {
            let shown = prefs.allocated(available, DescriptionSpace::default());
            for bar in Bar::ALL {
                for dx in [-20.0, 20.0] {
                    let saved = bar.drag_shown(&prefs, &shown, dx, available);
                    let changed = saved.width(bar) != prefs.width(bar);
                    let delta = if changed {
                        saved.width(bar) - shown.width(bar)
                    } else {
                        0.0
                    };
                    let space = if changed {
                        let target = bar.drag_shown(&shown, &shown, dx, available);
                        assert_eq!(target.width(bar) - shown.width(bar), delta);
                        DescriptionSpace::exchanged(available, &prefs, &target, bar)
                    } else {
                        DescriptionSpace::default()
                    };
                    let after = saved.allocated(available, space);
                    for other in Bar::ALL {
                        if bar == other && changed {
                            assert_eq!(after.width(bar), saved.width(bar));
                        } else {
                            assert_eq!(
                                after.width(other),
                                shown.width(other),
                                "{available}/{bar:?}/{dx} changed {other:?}"
                            );
                        }
                        if bar != other {
                            assert_eq!(saved.width(other), prefs.width(other));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod sequential_exchange_tests {
    use super::*;

    fn exchange(
        prefs: Cols,
        space: DescriptionSpace,
        available: f32,
        bar: Bar,
        change: f32,
    ) -> (Cols, DescriptionSpace) {
        let shown = prefs.allocated(available, space);
        let sign = if matches!(bar, Bar::Refs | Bar::Graph) {
            1.0
        } else {
            -1.0
        };
        let target = bar.drag_shown(&shown, &shown, sign * change, available);
        if target.width(bar) == shown.width(bar) {
            return (prefs, space);
        }
        let saved = bar.drag_shown(&prefs, &shown, sign * change, available);
        let next = DescriptionSpace::exchanged(available, &prefs, &target, bar);
        let after = saved.allocated(available, next);
        for other in Bar::ALL {
            assert_eq!(
                after.width(other),
                target.width(other),
                "{available}/{bar:?}/{change}/{other:?}"
            );
            if other != bar {
                assert_eq!(saved.width(other), prefs.width(other));
            }
        }
        (saved, next)
    }

    #[test]
    fn all_ordered_bar_pairs_exchange_both_directions_and_restore_deterministically()
    -> Result<(), String> {
        let prefs = Cols {
            refs: 280.0,
            graph: 100.0,
            hash: 70.0,
            author: 280.0,
            when: 100.0,
            date: 100.0,
            push: 0.0,
        };
        let mut grows = [0; 6];
        for available in [500.0, 600.0, 760.0, 760.25, 850.0, 1000.0, 1800.0, 6000.0] {
            for donor in Bar::ALL {
                let (first, space) =
                    exchange(prefs, DescriptionSpace::default(), available, donor, -20.0);
                for (index, bar) in Bar::ALL.into_iter().enumerate() {
                    for change in [
                        -10000.0, -21.0, -20.0, -19.0, -1.0, 0.0, 1.0, 19.0, 20.0, 21.0, 10000.0,
                    ] {
                        let before = first.allocated(available, space);
                        let (saved, next) = exchange(first, space, available, bar, change);
                        if saved.allocated(available, next).width(bar) > before.width(bar) {
                            grows[index] += 1;
                        }
                        let encoded = ron::to_string(&next).map_err(|e| e.to_string())?;
                        let restored: DescriptionSpace =
                            ron::from_str(&encoded).map_err(|e| e.to_string())?;
                        assert_eq!(next, restored);
                        let mut previous = saved.allocated(380.0, next);
                        let mut previous_description = 140.0;
                        for viewport in [
                            380.0, 500.0, 580.0, 600.0, 640.0, 760.0, 850.0, 1000.0, 1800.0, 6000.0,
                        ] {
                            let shown = saved.allocated(viewport, next);
                            assert_eq!(shown, saved.allocated(viewport, restored));
                            let rect = Rect::from_min_size(
                                pos2(0.0, 0.0),
                                egui::vec2(shown.content_width(viewport), 26.0),
                            );
                            for other in Bar::ALL {
                                assert!(shown.width(other) >= other.min());
                                assert!(shown.width(other) <= other.bounded(saved.width(other)));
                                assert!(shown.width(other) >= previous.width(other));
                            }
                            let description = shown.description(rect).width();
                            assert!(description >= MIN_DESCRIPTION_W);
                            assert!(description >= previous_description);
                            previous_description = description;
                            previous = shown;
                        }
                        // Arbitrary visits do not modify any of the saved inputs.
                        assert_eq!(
                            saved.allocated(available, next),
                            saved.allocated(available, restored)
                        );
                    }
                }
            }
        }
        assert!(grows.into_iter().all(|count| count > 0));
        Ok(())
    }

    #[test]
    fn legacy_and_invalid_compression_snapshots_keep_bounded_allocation() -> Result<(), String> {
        let legacy: DescriptionSpace =
            ron::from_str("(viewport:760.0,width:260.0)").map_err(|e| e.to_string())?;
        assert_eq!(legacy.compression, None);
        let prefs = Cols {
            refs: 5000.0,
            graph: 100.0,
            hash: 70.0,
            author: 280.0,
            when: 100.0,
            date: 0.0,
            push: 0.0,
        };
        for bad in [f32::NAN, f32::INFINITY, -1.0] {
            let invalid = DescriptionSpace {
                compression: Some([bad; 6]),
                ..legacy
            };
            for available in [100.0, 600.0, 760.0, 6000.0] {
                assert_eq!(
                    prefs.allocated(available, invalid),
                    prefs.allocated(available, legacy)
                );
            }
        }
        let shown = prefs.allocated(3000.0, DescriptionSpace::default());
        let target = Bar::Author.drag_shown(&shown, &shown, 20.0, 3000.0);
        let space = DescriptionSpace::exchanged(3000.0, &prefs, &target, Bar::Author);
        // Oversized raw preferences and hidden columns must not manufacture compression.
        assert_eq!(space.compression.unwrap(), [0.0; 6]);
        let mut saved = prefs;
        saved.author = target.author;
        assert_eq!(saved.allocated(3000.0, space), target);
        Ok(())
    }
}
