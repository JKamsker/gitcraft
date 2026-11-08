//! Explicit native-file tools, independent of repository tabs and connection profiles.
pub(crate) mod access;
mod cli;
pub(crate) mod directory;
mod files;
mod model;
mod permissions;
pub(crate) mod registration;
mod save;
mod ui;
#[cfg(any(windows, test))]
mod windows_metadata;

pub use cli::{Invocation, USAGE, parse, usage};

/// Presentation adapter for the standalone native-window title.
pub fn window_title() -> &'static str {
    crate::i18n::text("interaction-standalone-window-title")
}

pub fn run(invocation: Invocation, options: eframe::NativeOptions) -> Result<u8, String> {
    if !matches!(invocation, Invocation::Diff(_) | Invocation::Merge(_)) {
        return Err(crate::i18n::text("interaction-standalone-error-expected-tool").into());
    }
    let outcome = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(1));
    let result = std::sync::Arc::clone(&outcome);
    eframe::run_native(
        "gitcraft-file-tools",
        options,
        Box::new(move |cc| Ok(Box::new(ui::ToolApp::new(invocation, result, &cc.egui_ctx)))),
    )
    .map_err(|e| e.to_string())?;
    Ok(outcome.load(std::sync::atomic::Ordering::Acquire))
}

fn submit(job: impl FnOnce() + Send + 'static) -> Result<(), String> {
    crate::tasks::pool()?
        .submit(
            crate::tasks::cancellation().child(),
            crate::tasks::Priority::Interactive,
            None,
            Box::new(job),
        )
        .map_err(str::to_owned)
}

#[cfg(test)]
mod localization_tests {
    use fluent_bundle::{FluentArgs, FluentResource, FluentValue, concurrent::FluentBundle};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn paired_standalone_catalogs_cover_every_id_and_named_argument_without_fallback()
    -> Result<(), String> {
        let variable =
            regex::Regex::new(r"\$([A-Za-z][A-Za-z0-9_-]*)").map_err(|e| e.to_string())?;
        let mut contracts = Vec::new();
        for (locale, source) in [
            (
                "en-US",
                include_str!("../../locales/en-US/interaction-standalone.ftl"),
            ),
            (
                "de",
                include_str!("../../locales/de/interaction-standalone.ftl"),
            ),
        ] {
            let resource = FluentResource::try_new(source.to_owned())
                .map_err(|(_, errors)| format!("{locale}: {errors:?}"))?;
            let mut contract: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
            let mut current = "";
            for line in source.lines() {
                if !line.starts_with(char::is_whitespace)
                    && let Some((id, _)) = line.split_once(" = ")
                {
                    assert!(id.starts_with("interaction-standalone-"));
                    assert!(
                        contract.insert(id, BTreeSet::new()).is_none(),
                        "duplicate {id}"
                    );
                    current = id;
                }
                if let Some(vars) = contract.get_mut(current) {
                    vars.extend(variable.captures_iter(line).map(|c| c[1].to_owned()));
                }
            }
            assert_eq!(contract.len(), 122);
            let mut bundle =
                FluentBundle::new_concurrent(vec![locale.parse().map_err(|e| format!("{e}"))?]);
            bundle.set_use_isolating(false);
            bundle
                .add_resource(resource)
                .map_err(|e| format!("{e:?}"))?;
            crate::i18n::set_language(locale);
            for (id, variables) in &contract {
                let args: Vec<_> = variables
                    .iter()
                    .map(|name| {
                        let value = if matches!(name.as_str(), "count" | "installed" | "total") {
                            FluentValue::from(1234)
                        } else {
                            FluentValue::from("opaque ' $() & ü\n{}")
                        };
                        (name.as_str(), value)
                    })
                    .collect();
                let mut fluent = FluentArgs::new();
                for (name, value) in &args {
                    fluent.set(*name, value.clone());
                }
                let message = bundle.get_message(id).ok_or_else(|| id.to_string())?;
                assert!(message.attributes().next().is_none());
                let pattern = message.value().ok_or_else(|| id.to_string())?;
                let mut errors = Vec::new();
                let expected = bundle.format_pattern(pattern, Some(&fluent), &mut errors);
                assert!(errors.is_empty(), "{locale}/{id}: {errors:?}");
                assert_eq!(
                    crate::i18n::message(id, &args),
                    expected,
                    "embedded {locale}/{id}"
                );
                if variables.is_empty() {
                    assert_eq!(crate::i18n::text(id), expected);
                } else if variables
                    .iter()
                    .any(|n| !matches!(n.as_str(), "count" | "installed" | "total"))
                {
                    assert!(
                        expected.contains("opaque ' $() & ü\n{}"),
                        "lost payload in {id}"
                    );
                }
            }
            contracts.push(contract);
        }
        assert_eq!(contracts[0], contracts[1]);
        crate::i18n::set_language("de");
        assert_eq!(super::window_title(), "GitCraft-Dateiwerkzeuge");
        let value = crate::i18n::message(
            "interaction-standalone-registration-complete",
            &[("count", 1234.into())],
        );
        assert!(value.starts_with("1.234 Einstellungen"));
        let single = crate::i18n::message(
            "interaction-standalone-registration-complete",
            &[("count", 1.into())],
        );
        assert!(single.starts_with("Eine Einstellung"));
        crate::i18n::set_language("en-US");
        assert_eq!(super::window_title(), "GitCraft file tools");
        assert!(
            crate::i18n::message(
                "interaction-standalone-registration-complete",
                &[("count", 1234.into())],
            )
            .starts_with("Registered 1,234 named-tool settings")
        );
        Ok(())
    }
}
