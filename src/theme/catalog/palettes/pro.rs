//! Published pro palette values.
use super::super::*;

pub(in super::super) const DARK: Builtin = Builtin {
    id: "Dark",
    name: "gitcraft Dark",
    credit: "PhotoCraft's Pro (Dark)",
    palette: pro_dark(),
};

pub(in super::super) const MEDIUMGRAY: Builtin = Builtin {
    id: "MediumGray",
    name: "gitcraft Medium Gray",
    credit: "PhotoCraft's Pro (Medium Gray)",
    palette: pro_medium_gray(),
};

pub(in super::super) const LIGHT: Builtin = Builtin {
    id: "Light",
    name: "gitcraft Light",
    credit: "PhotoCraft's Pro (Light)",
    palette: pro_light(),
};
