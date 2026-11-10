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
#[path = "direct_tests/columns_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "direct_tests/columns_exchange_tests.rs"]
mod exchange_tests;

#[cfg(test)]
#[path = "direct_tests/columns_snapshot_allocation_tests.rs"]
mod snapshot_allocation_tests;

#[cfg(test)]
#[path = "direct_tests/columns_sequential_exchange_tests.rs"]
mod sequential_exchange_tests;
