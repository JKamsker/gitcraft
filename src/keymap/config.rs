//! Stable, bounded persisted key sequences. Empty sequences explicitly unbind an action.
use std::collections::BTreeMap;

use egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};

use super::{BINDINGS, Command};

pub(crate) type Overrides = BTreeMap<String, Vec<Stroke>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stroke {
    pub key: String,
    pub command: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Stroke {
    pub(crate) fn new(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key: key.name().to_owned(),
            command: modifiers.command,
            ctrl: modifiers.ctrl && (cfg!(target_os = "macos") || !modifiers.command),
            alt: modifiers.alt,
            shift: modifiers.shift,
        }
    }

    fn modifiers(&self) -> Modifiers {
        Modifiers {
            command: self.command,
            ctrl: self.ctrl,
            alt: self.alt,
            shift: self.shift,
            mac_cmd: false,
        }
    }

    pub(crate) fn matches(&self, key: Key, modifiers: Modifiers) -> bool {
        Key::from_name(&self.key) == Some(key) && modifiers.matches_exact(self.modifiers())
    }

    pub(crate) fn label(&self) -> String {
        Key::from_name(&self.key)
            .map(|k| super::combo(self.modifiers(), k))
            .unwrap_or_else(|| "Invalid key".into())
    }

    fn equivalent(&self, other: &Self) -> bool {
        self.key == other.key
            && self.alt == other.alt
            && self.shift == other.shift
            && if cfg!(target_os = "macos") {
                self.command == other.command && self.ctrl == other.ctrl
            } else {
                (self.command || self.ctrl) == (other.command || other.ctrl)
            }
    }
}

pub(crate) fn commands() -> impl Iterator<Item = Command> {
    BINDINGS.iter().map(|b| b.command)
}

pub(crate) fn effective(overrides: &Overrides, command: Command) -> Vec<Stroke> {
    overrides.get(command.id()).cloned().unwrap_or_else(|| {
        BINDINGS
            .iter()
            .filter(|b| b.command == command)
            .map(|b| Stroke::new(b.key, b.modifiers))
            .collect()
    })
}

/// Window/repository actions can be used together; local contexts added by features must
/// declare their exclusivity here rather than silently stealing one another's bindings.
fn overlaps(a: Command, b: Command) -> bool {
    a.scope().overlaps(b.scope())
}

pub(crate) fn validate(overrides: &Overrides) -> Result<(), String> {
    for (id, sequence) in overrides {
        if !commands().any(|c| c.id() == id) {
            return Err(format!("Unknown action: {id}"));
        }
        if sequence.len() > 4 {
            return Err("Use at most four keys in a chord".into());
        }
        for s in sequence {
            if Key::from_name(&s.key).is_none() {
                return Err(format!("Unknown key: {}", s.key));
            }
            if s.key == "Escape" {
                return Err("Escape is reserved to cancel a chord or editor".into());
            }
        }
    }
    let all: Vec<_> = commands().map(|c| (c, effective(overrides, c))).collect();
    for (i, (a, x)) in all.iter().enumerate() {
        if x.is_empty() {
            continue;
        }
        for (b, y) in &all[i + 1..] {
            if y.is_empty() || !overlaps(*a, *b) {
                continue;
            }
            if x.iter().zip(y).all(|(x, y)| x.equivalent(y)) {
                return Err(format!(
                    "{} conflicts with {} (same keys or chord prefix)",
                    a.title(),
                    b.title()
                ));
            }
        }
    }
    Ok(())
}

/// A malformed entry cannot make the entire saved application state unreadable. Preserve
/// valid entries and report the rejected ones in the editor; dispatch uses the safe defaults.
pub(crate) fn usable(overrides: &Overrides) -> Overrides {
    if validate(overrides).is_ok() {
        overrides.clone()
    } else {
        Overrides::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conflicts_prefixes_unbind_reset_and_roundtrip() {
        let mut o = Overrides::new();
        o.insert(
            Command::Fetch.id().into(),
            vec![
                Stroke::new(Key::K, Modifiers::COMMAND),
                Stroke::new(Key::F, Modifiers::NONE),
            ],
        );
        assert!(validate(&o).is_ok());
        let json = serde_json::to_string(&o).unwrap_or_default();
        assert_eq!(
            serde_json::from_str::<Overrides>(&json).ok(),
            Some(o.clone())
        );
        o.insert(
            Command::Push.id().into(),
            vec![Stroke::new(Key::K, Modifiers::COMMAND)],
        );
        assert!(validate(&o).is_err());
        o.insert(Command::Push.id().into(), vec![]);
        assert!(validate(&o).is_ok());
        assert!(effective(&o, Command::Push).is_empty());
        o.remove(Command::Fetch.id());
        assert_eq!(effective(&o, Command::Fetch).len(), 1);
        o.insert("future_action".into(), vec![]);
        assert!(validate(&o).is_err());
        assert!(usable(&o).is_empty());
    }
}
