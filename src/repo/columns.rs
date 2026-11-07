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
    pub fn drag(self, prefs: &Cols, _last: Option<Bar>, dx: f32, available: f32) -> Cols {
        let shown = prefs.squeezed(available, None);
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
    pub fn squeezed(self, available: f32, _last: Option<Bar>) -> Self {
        let mut out = Self { push: 0.0, ..self };
        for bar in Bar::ALL {
            let w = out.width_mut(bar);
            if *w != 0.0 {
                *w = bar.bounded(*w);
            }
        }
        let mut overflow =
            (out.refs + out.graph + out.trailing() + MIN_DESCRIPTION_W - available).max(0.0);
        for bar in [
            Bar::Graph,
            Bar::Refs,
            Bar::Hash,
            Bar::Author,
            Bar::When,
            Bar::Date,
        ] {
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
