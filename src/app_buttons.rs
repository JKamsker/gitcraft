//! The app's own buttons: settings, the command palette, the theme menu and the manual. Each
//! sits in the title bar (the tab strip) or in the footer of a repository tab's left or right
//! side panel, where they stack up the panel's strip when it is folded. They are dragged
//! between those places and along them: the button stays under the pointer while an opening
//! for it moves to the place nearest the pointer (the others sliding apart to make it), and it
//! slides into that opening when let go (Esc puts it back). A right click moves one
//! as well. Wherever they are drawn, what they are used for is passed back to the app
//! ([`take_events`]), which carries it out.

use egui::{Align, Id, Pos2, Rect, Response, Sense, Ui, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::icons;
use crate::settings::{self, Settings, ThemePick};
use crate::theme::{self, ThemeId, Tokens};

/// Every button is this size, wherever it is.
const SIZE: f32 = 28.0;
/// A margin around a place's buttons counted as the place itself.
const CATCH: f32 = 16.0;
/// Seconds buttons take to slide into their places.
const SLIDE: f32 = 0.18;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum AppButton {
    Settings,
    Palette,
    Theme,
    Manual,
}

impl AppButton {
    pub const ALL: [AppButton; 4] = [
        AppButton::Settings,
        AppButton::Palette,
        AppButton::Theme,
        AppButton::Manual,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            AppButton::Settings => "settings",
            AppButton::Palette => "command",
            AppButton::Theme => "palette",
            AppButton::Manual => "book-open",
        }
    }

    fn command(self) -> Option<crate::keymap::Command> {
        use crate::keymap::Command;
        match self {
            AppButton::Settings => Some(Command::Settings),
            AppButton::Palette => Some(Command::Palette),
            AppButton::Manual => Some(Command::Manual),
            AppButton::Theme => None,
        }
    }

    pub fn tooltip(self, ctx: &egui::Context) -> String {
        let text = if self == AppButton::Manual {
            crate::i18n::text("interaction-presentation-manual-tip")
        } else {
            self.label()
        };
        self.command().map_or_else(
            || text.to_owned(),
            |command| crate::keymap::tip(ctx, text, command),
        )
    }

    /// Its name in a menu.
    pub fn label(self) -> &'static str {
        match self {
            AppButton::Settings => crate::i18n::text("interaction-presentation-settings"),
            AppButton::Palette => crate::i18n::text("interaction-presentation-palette"),
            AppButton::Theme => crate::i18n::text("interaction-presentation-theme"),
            AppButton::Manual => crate::i18n::text("interaction-presentation-manual"),
        }
    }
}

/// Where buttons sit.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Place {
    TitleBar,
    /// The footer of the branches panel, after its toggle.
    BottomLeft,
    /// The footer of the detail panel, before its toggle.
    BottomRight,
}

impl Place {
    pub const ALL: [Place; 3] = [Place::TitleBar, Place::BottomLeft, Place::BottomRight];

    pub fn label(self) -> &'static str {
        match self {
            Place::TitleBar => crate::i18n::text("interaction-presentation-title-bar"),
            Place::BottomLeft => crate::i18n::text("interaction-presentation-bottom-left"),
            Place::BottomRight => crate::i18n::text("interaction-presentation-bottom-right"),
        }
    }
}

/// The buttons each place holds, in order from the place's outer end: the window's edge for the
/// title bar and the bottom right, the panel's toggle for the bottom left (a setting).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ButtonLayout {
    pub title_bar: Vec<AppButton>,
    pub bottom_left: Vec<AppButton>,
    pub bottom_right: Vec<AppButton>,
}

impl Default for ButtonLayout {
    fn default() -> Self {
        Self {
            title_bar: AppButton::ALL.to_vec(),
            bottom_left: Vec::new(),
            bottom_right: Vec::new(),
        }
    }
}

impl ButtonLayout {
    pub fn at(&self, place: Place) -> &[AppButton] {
        match place {
            Place::TitleBar => &self.title_bar,
            Place::BottomLeft => &self.bottom_left,
            Place::BottomRight => &self.bottom_right,
        }
    }

    fn at_mut(&mut self, place: Place) -> &mut Vec<AppButton> {
        match place {
            Place::TitleBar => &mut self.title_bar,
            Place::BottomLeft => &mut self.bottom_left,
            Place::BottomRight => &mut self.bottom_right,
        }
    }

    pub fn place_of(&self, button: AppButton) -> Place {
        Place::ALL
            .into_iter()
            .find(|&p| self.at(p).contains(&button))
            .unwrap_or(Place::TitleBar)
    }

    /// This layout with `button` at `place`, `index` among the buttons there once it is out of
    /// them (past the end puts it last).
    pub fn moved(&self, button: AppButton, place: Place, index: usize) -> ButtonLayout {
        let mut next = self.clone();
        for p in Place::ALL {
            next.at_mut(p).retain(|&b| b != button);
        }
        let list = next.at_mut(place);
        list.insert(index.min(list.len()), button);
        next
    }

    /// Every button in exactly one place: one missing (saved before it existed) goes at the end
    /// of the title bar, one listed twice stays where it is first.
    pub fn normalized(&self) -> ButtonLayout {
        let mut seen = Vec::new();
        let mut next = ButtonLayout {
            title_bar: Vec::new(),
            bottom_left: Vec::new(),
            bottom_right: Vec::new(),
        };
        for p in Place::ALL {
            for &b in self.at(p) {
                if !seen.contains(&b) {
                    seen.push(b);
                    next.at_mut(p).push(b);
                }
            }
        }
        for b in AppButton::ALL {
            if !seen.contains(&b) {
                next.title_bar.push(b);
            }
        }
        next
    }
}

/// The layout the settings hold, made whole.
pub fn layout(ctx: &egui::Context) -> ButtonLayout {
    Settings::get(ctx).button_layout.normalized()
}

/// What was done with a button, for the app to carry out.
#[derive(Clone, Debug)]
pub enum Event {
    Clicked(AppButton),
    Theme(ThemePick),
    /// Moved with a right click: to the end of the place.
    Move(AppButton, Place),
}

fn events_id() -> Id {
    Id::new("gitcraft-app-button-events")
}

fn push(ctx: &egui::Context, event: Event) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<Event>>(events_id())
            .push(event);
    });
}

/// What was done with the buttons since this was last asked.
pub fn take_events(ctx: &egui::Context) -> Vec<Event> {
    ctx.data_mut(|d| d.remove_temp::<Vec<Event>>(events_id()))
        .unwrap_or_default()
}

fn theme_id() -> Id {
    Id::new("gitcraft-app-button-theme")
}

/// The theme in use, which the theme menu marks wherever it opens from.
pub fn set_current_theme(ctx: &egui::Context, id: &ThemeId) {
    ctx.data_mut(|d| d.insert_temp(theme_id(), id.clone()));
}

fn current_theme(ctx: &egui::Context) -> ThemeId {
    ctx.data(|d| d.get_temp::<ThemeId>(theme_id()))
        .unwrap_or_default()
}

/// How a place's buttons are drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    /// The title bar's.
    Bar,
    /// A panel footer's, like the panel's toggle.
    Panel,
}

mod drag;
pub use drag::*;

mod render;
pub use render::*;

#[cfg(test)]
mod tests;
