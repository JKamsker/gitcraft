//! Embedded Fluent catalogs shared by immediate/deferred UI and background presentation.
//! Source/argv/ref names are never translated; only application-owned presentation is.
mod number;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
#[cfg(not(test))]
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

pub(crate) use fluent_bundle::FluentValue as Value;
use fluent_bundle::{FluentArgs, FluentResource, concurrent::FluentBundle};
use unic_langid::LanguageIdentifier;

include!(concat!(env!("OUT_DIR"), "/gitcraft_catalogs.rs"));

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Language {
    #[default]
    English,
    German,
}
impl Language {
    pub fn tag(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::German => "de",
        }
    }
}

/// Explicit settings are authoritative; an unsupported choice falls back to English.
/// System preferences are considered in their original order only for the empty setting.
pub(crate) fn resolve(selected: &str, system: &[String]) -> Language {
    let supported = |tag: &str| {
        let normalized = tag
            .split(['.', '@'])
            .next()
            .unwrap_or(tag)
            .replace('_', "-");
        let id = normalized.parse::<LanguageIdentifier>().ok()?;
        match id.language.as_str() {
            "en" => Some(Language::English),
            "de" => Some(Language::German),
            _ => None,
        }
    };
    if selected.is_empty() {
        system.iter().find_map(|s| supported(s)).unwrap_or_default()
    } else {
        supported(selected).unwrap_or_default()
    }
}

#[cfg(not(test))]
static LANGUAGE: AtomicU8 = AtomicU8::new(0);
#[cfg(not(test))]
static GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
thread_local! {
    static SELECTION: std::cell::Cell<(Language, u64)> = const { std::cell::Cell::new((Language::English, 0)) };
}

pub(crate) fn language() -> Language {
    #[cfg(not(test))]
    {
        if LANGUAGE.load(Ordering::Acquire) == 1 {
            Language::German
        } else {
            Language::English
        }
    }
    #[cfg(test)]
    {
        SELECTION.with(|s| s.get().0)
    }
}
/// Revision for cached menus/search/layout presentation; do not use translated text as IDs.
pub(crate) fn generation() -> u64 {
    #[cfg(not(test))]
    {
        GENERATION.load(Ordering::Acquire)
    }
    #[cfg(test)]
    {
        SELECTION.with(|s| s.get().1)
    }
}

pub(crate) fn set_language(selected: &str) -> bool {
    #[cfg(not(test))]
    let system = {
        static SYSTEM: OnceLock<Vec<String>> = OnceLock::new();
        SYSTEM
            .get_or_init(|| sys_locale::get_locales().collect())
            .as_slice()
    };
    #[cfg(test)]
    let system: &[String] = &[]; // headless tests never query native locale preferences
    let next = resolve(selected, system);
    #[cfg(not(test))]
    {
        let changed = LANGUAGE.swap(u8::from(next == Language::German), Ordering::AcqRel)
            != u8::from(next == Language::German);
        if changed {
            GENERATION.fetch_add(1, Ordering::AcqRel);
        }
        changed
    }
    #[cfg(test)]
    {
        SELECTION.with(|s| {
            let (old, generation) = s.get();
            let changed = old != next;
            s.set((next, generation + u64::from(changed)));
            changed
        })
    }
}

struct Catalog {
    bundle: FluentBundle<FluentResource>,
    // Only catalog literals are interned, once per fixed embedded language. No per-render
    // or per-selection allocations leak, and existing &'static str UI interfaces survive.
    plain: BTreeMap<&'static str, &'static str>,
}
impl Catalog {
    fn load(language: Language, sources: &[(&'static str, &'static str)]) -> Result<Self, String> {
        let id = language
            .tag()
            .parse()
            .map_err(|e| format!("Invalid catalog language: {e}"))?;
        let mut bundle = FluentBundle::new_concurrent(vec![id]);
        bundle
            .add_builtins()
            .map_err(|e| format!("Fluent builtins: {e}"))?;
        // Catalogs are currently LTR English/German; UI strings must retain literal argv and
        // copyable paths. Revisit bidi isolation together with a supported RTL layout.
        bundle.set_use_isolating(false);
        match language {
            Language::English => bundle.set_formatter(Some(|v, _| number::format(v, false))),
            Language::German => bundle.set_formatter(Some(|v, _| number::format(v, true))),
        }
        let mut ids = BTreeSet::new();
        for (path, source) in sources {
            let resource = FluentResource::try_new((*source).to_owned())
                .map_err(|(_, errors)| format!("{path}: {errors:?}"))?;
            bundle
                .add_resource(resource)
                .map_err(|e| format!("{path}: {e:?}"))?;
            ids.extend(message_ids(source));
        }
        let mut plain = BTreeMap::new();
        for id in ids {
            if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                let text = bundle.format_pattern(pattern, None, &mut errors);
                if errors.is_empty() {
                    plain.insert(id, &*Box::leak(text.into_owned().into_boxed_str()));
                }
            }
        }
        Ok(Self { bundle, plain })
    }
    fn format(&self, id: &str, args: &[(&str, Value<'_>)]) -> Option<String> {
        let pattern = self.bundle.get_message(id)?.value()?;
        let mut fluent = FluentArgs::new();
        for (name, value) in args {
            fluent.set(*name, value.clone());
        }
        let mut errors = Vec::new();
        let value = self
            .bundle
            .format_pattern(pattern, Some(&fluent), &mut errors);
        errors.is_empty().then(|| value.into_owned())
    }
}
fn message_ids(source: &'static str) -> impl Iterator<Item = &'static str> {
    source.lines().filter_map(|line| {
        if !line.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return None;
        }
        line.split_once('=').map(|(id, _)| id.trim())
    })
}
struct Catalogs {
    english: Catalog,
    german: Catalog,
}
fn catalogs() -> &'static Catalogs {
    static CATALOGS: OnceLock<Catalogs> = OnceLock::new();
    CATALOGS.get_or_init(|| Catalogs {
        english: Catalog::load(Language::English, ENGLISH)
            .expect("validated embedded English catalog"),
        german: Catalog::load(Language::German, GERMAN).expect("validated embedded German catalog"),
    })
}
fn chosen() -> &'static Catalog {
    match language() {
        Language::English => &catalogs().english,
        Language::German => &catalogs().german,
    }
}
/// No-argument application label. The bounded static cache preserves existing UI lifetimes.
/// Missing/invalid selected entries fall back to English, then to the stable message ID.
pub(crate) fn text(id: &'static str) -> &'static str {
    chosen()
        .plain
        .get(id)
        .or_else(|| catalogs().english.plain.get(id))
        .copied()
        .unwrap_or(id)
}
/// Fluent interpolation and plural selection. Pass numbers as numeric Values, never as
/// preformatted English strings. Missing/invalid selected entries retry the English message.
pub(crate) fn message(id: &str, args: &[(&str, Value<'_>)]) -> String {
    chosen()
        .format(id, args)
        .or_else(|| catalogs().english.format(id, args))
        .unwrap_or_else(|| id.to_owned())
}
pub(crate) fn number(value: impl Into<Value<'static>>) -> String {
    message("format-number", &[("value", value.into())])
}
/// Localized civil date from Git's ISO date; protocol parsing/storage remains ISO.
/// Invalid/user-provided values are preserved verbatim rather than misinterpreted.
pub(crate) fn date_iso(iso: &str) -> String {
    let Some(day) = crate::git::activity::Day::parse(iso) else {
        return iso.to_owned();
    };
    message(
        "format-date",
        &[
            ("year", format!("{:04}", day.year).into()),
            ("month", format!("{:02}", day.month).into()),
            ("day", format!("{:02}", day.day).into()),
        ],
    )
}
