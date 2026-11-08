//! A selected matrix is materialised in a private workflow snapshot. CLI matrix filters
//! alone can select several entries (especially `include` entries).
use yaml_rust2::{
    Yaml, YamlEmitter, YamlLoader,
    parser::{Event, Parser},
};

pub(super) const MAX: usize = 1 << 20;
#[derive(Clone, Debug)]
pub(super) struct Workflow {
    pub file: String,
    pub source: String,
    pub events: Vec<String>,
    pub jobs: Vec<String>,
    yaml: Yaml,
}
pub(super) fn text(value: &Yaml) -> String {
    if let Some(s) = value.as_str() {
        return s.to_owned();
    }
    let mut s = String::new();
    let _ = YamlEmitter::new(&mut s).dump(value);
    s.trim_start_matches("---\n").trim().to_owned()
}
fn strings(value: &Yaml) -> Result<Vec<String>, String> {
    let values = match value {
        Yaml::String(s) => vec![s.clone()],
        Yaml::Array(a) => a.iter().map(text).collect(),
        Yaml::Hash(h) => h.keys().map(text).collect(),
        Yaml::Null | Yaml::BadValue => Vec::new(),
        _ => return Err(crate::i18n::text("repo-workflows-local_ci-strings-d5611771").into()),
    };
    if values
        .iter()
        .any(|s| s.is_empty() || s.starts_with('-') || s.contains("${{"))
    {
        return Err(crate::i18n::text("repo-workflows-local_ci-strings-5c665142").into());
    }
    Ok(values)
}
pub(super) fn parse(file: String, source: String) -> Result<Workflow, String> {
    parse_with_witness(file, source).map_err(|failure| failure.display())
}
fn map() -> Yaml {
    Yaml::Hash(Default::default())
}
fn put(yaml: &mut Yaml, key: &str, value: Yaml) -> Result<(), String> {
    yaml.as_mut_hash()
        .ok_or(crate::i18n::text("repo-workflows-local_ci-put-1e79a468"))?
        .insert(Yaml::String(key.into()), value);
    Ok(())
}
fn matches(entry: &Yaml, pattern: &Yaml) -> bool {
    pattern.as_hash().is_some_and(|p| {
        p.iter()
            .all(|(k, v)| entry.as_hash().and_then(|h| h.get(k)) == Some(v))
    })
}
impl Workflow {
    pub fn matrices(&self, job: &str) -> Result<Vec<Yaml>, String> {
        let matrix = &self.yaml["jobs"][job]["strategy"]["matrix"];
        if matches!(matrix, Yaml::BadValue | Yaml::Null) {
            return Ok(vec![map()]);
        }
        let fields = matrix.as_hash().ok_or(crate::i18n::text(
            "repo-workflows-local_ci-matrices-5b4af304",
        ))?;
        let mut entries = vec![map()];
        for (key, values) in fields {
            let key = key.as_str().ok_or(crate::i18n::text(
                "repo-workflows-local_ci-matrices-9c4ccca2",
            ))?;
            if key == "include" || key == "exclude" {
                continue;
            }
            let values = values.as_vec().ok_or(crate::i18n::text(
                "repo-workflows-local_ci-matrices-628bb2dc",
            ))?;
            if values.iter().any(|v| text(v).contains("${{")) {
                return Err(crate::i18n::text("repo-workflows-local_ci-matrices-57ef5431").into());
            }
            let mut next = Vec::new();
            for entry in &entries {
                for value in values {
                    let mut e = entry.clone();
                    put(&mut e, key, value.clone())?;
                    next.push(e);
                    if next.len() > 256 {
                        return Err(
                            crate::i18n::text("repo-workflows-local_ci-matrices-e3f5c014").into(),
                        );
                    }
                }
            }
            entries = next;
        }
        if !fields
            .keys()
            .any(|k| k.as_str().is_some_and(|k| k != "include" && k != "exclude"))
        {
            entries.clear();
        }
        let originals = entries.clone();
        if let Some(exclude) = matrix["exclude"].as_vec() {
            for item in exclude {
                let excluded = item.as_hash().ok_or(crate::i18n::text(
                    "repo-workflows-local_ci-matrices-36d96458",
                ))?;
                if excluded.keys().any(|key| {
                    !fields.contains_key(key)
                        || ["include", "exclude"].contains(&key.as_str().unwrap_or(""))
                }) {
                    return Err(
                        crate::i18n::text("repo-workflows-local_ci-matrices-5f87e09b").into(),
                    );
                }
            }
            entries.retain(|e| !exclude.iter().any(|p| matches(e, p)));
        } else if !matches!(matrix["exclude"], Yaml::BadValue | Yaml::Null) {
            return Err(crate::i18n::text("repo-workflows-local_ci-matrices-68e58030").into());
        }
        let mut extras = Vec::new();
        if let Some(include) = matrix["include"].as_vec() {
            for addition in include {
                let fields = addition.as_hash().ok_or(crate::i18n::text(
                    "repo-workflows-local_ci-matrices-6ba6931e",
                ))?;
                if text(addition).contains("${{") {
                    return Err(
                        crate::i18n::text("repo-workflows-local_ci-matrices-a6667f67").into(),
                    );
                }
                let mut applied = false;
                for original in &originals {
                    // Include may add/replace a field introduced by an earlier include, but
                    // cannot replace a value in the original Cartesian product.
                    let compatible = fields.iter().all(|(k, v)| {
                        original
                            .as_hash()
                            .and_then(|h| h.get(k))
                            .is_none_or(|value| value == v)
                    });
                    if compatible {
                        for entry in &mut entries {
                            if matches(entry, original) {
                                for (k, v) in fields {
                                    entry
                                        .as_mut_hash()
                                        .ok_or(crate::i18n::text(
                                            "repo-workflows-local_ci-matrices-569945ec",
                                        ))?
                                        .insert(k.clone(), v.clone());
                                }
                                applied = true;
                            }
                        }
                    }
                }
                if !applied {
                    extras.push(addition.clone());
                }
                if entries.len() + extras.len() > 256 {
                    return Err(
                        crate::i18n::text("repo-workflows-local_ci-matrices-e3f5c014").into(),
                    );
                }
            }
        } else if !matches!(matrix["include"], Yaml::BadValue | Yaml::Null) {
            return Err(crate::i18n::text("repo-workflows-local_ci-matrices-ea2779b1").into());
        }
        entries.extend(extras);
        Ok(entries)
    }
    pub fn dependencies(&self, job: &str) -> Result<Vec<String>, String> {
        fn visit(
            w: &Workflow,
            j: &str,
            seen: &mut Vec<String>,
            stack: &mut Vec<String>,
        ) -> Result<(), String> {
            if stack.iter().any(|s| s == j) {
                return Err(crate::i18n::text("repo-workflows-local_ci-visit-03e9fa88").into());
            }
            if seen.iter().any(|s| s == j) {
                return Ok(());
            }
            if !w.jobs.iter().any(|s| s == j) {
                return Err(crate::i18n::message(
                    "repo-workflows-local_ci-visit-038c0ec6",
                    &[("j", (j).clone().into())],
                ));
            }
            stack.push(j.into());
            for dep in strings(&w.yaml["jobs"][j]["needs"])? {
                visit(w, &dep, seen, stack)?;
            }
            stack.pop();
            seen.push(j.into());
            Ok(())
        }
        let mut deps = Vec::new();
        visit(self, job, &mut deps, &mut Vec::new())?;
        Ok(deps)
    }
    pub fn snapshot(
        &self,
        event: &str,
        job: &str,
        entry: &Yaml,
        nonce: &str,
    ) -> Result<String, String> {
        if !self.events.iter().any(|e| e == event) || !self.jobs.iter().any(|j| j == job) {
            return Err(crate::i18n::text("repo-workflows-local_ci-snapshot-67615d59").into());
        }
        if !self.matrices(job)?.contains(entry) {
            return Err(crate::i18n::text("repo-workflows-local_ci-snapshot-1caa7e76").into());
        }
        let deps = self.dependencies(job)?;
        let mut yaml = self.yaml.clone();
        put(&mut yaml, "name", Yaml::String(format!("gitcraft-{nonce}")))?;
        let mut jobs = map();
        for dep in deps {
            put(&mut jobs, &dep, yaml["jobs"][dep.as_str()].clone())?;
        }
        if !matches!(jobs[job]["strategy"]["matrix"], Yaml::BadValue | Yaml::Null) {
            let selected = jobs
                .as_mut_hash()
                .and_then(|h| h.get_mut(&Yaml::String(job.into())))
                .ok_or(crate::i18n::text(
                    "repo-workflows-local_ci-snapshot-da7150dd",
                ))?;
            let mut strategy = selected["strategy"].clone();
            let mut matrix = map();
            put(&mut matrix, "include", Yaml::Array(vec![entry.clone()]))?;
            put(&mut strategy, "matrix", matrix)?;
            put(selected, "strategy", strategy)?;
        }
        put(&mut yaml, "jobs", jobs)?;
        let mut out = String::new();
        YamlEmitter::new(&mut out)
            .dump(&yaml)
            .map_err(|e| e.to_string())?;
        Ok(out)
    }
}

#[cfg(test)]
mod fluent_model_tests {
    use super::*;

    #[test]
    fn fluent_partition_local_workflow_parser_localizes_errors_and_preserves_job_identity()
    -> Result<(), String> {
        let yaml = "on: [push]\njobs:\n  Compile-and-Test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo unchanged\n";
        for (language, expected) in [
            ("en-US", "Workflow has no jobs mapping"),
            ("de", "Der Workflow enthält keine jobs-Zuordnung"),
        ] {
            crate::i18n::set_language(language);
            let error = parse("literal.yml".into(), "on: push\n".into()).err();
            assert_eq!(error.as_deref(), Some(expected));
            let workflow = parse("literal.yml".into(), yaml.into())?;
            assert_eq!(workflow.jobs, ["Compile-and-Test"]);
            assert_eq!(workflow.events, ["push"]);
            assert_eq!(workflow.source, yaml);
            let entries = workflow.matrices("Compile-and-Test")?;
            assert_eq!(entries.len(), 1);
            let snapshot =
                workflow.snapshot("push", "Compile-and-Test", &entries[0], "recording-only")?;
            let snapshot = YamlLoader::load_from_str(&snapshot).map_err(|e| e.to_string())?;
            assert_eq!(
                snapshot[0]["jobs"]["Compile-and-Test"]["steps"][0]["run"].as_str(),
                Some("echo unchanged")
            );
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }
}

pub(super) fn parse_with_witness(file: String, source: String) -> Result<Workflow, ParseFailure> {
    if source.len() > MAX {
        return Err(ParseFailure::TooLarge.into());
    }
    let mut parser = Parser::new(source.chars());
    let mut depth = 0usize;
    let mut count = 0usize;
    loop {
        let (event, _) = parser.next_token().map_err(|e| e.to_string())?;
        count += 1;
        match event {
            Event::Alias(_) => {
                return Err(ParseFailure::Aliases.into());
            }
            Event::MappingStart(..) | Event::SequenceStart(..) => depth += 1,
            Event::MappingEnd | Event::SequenceEnd => depth = depth.saturating_sub(1),
            Event::StreamEnd => break,
            _ => {}
        }
        if depth > 64 || count > 100_000 {
            return Err(ParseFailure::StructureLimit.into());
        }
    }
    let mut docs = YamlLoader::load_from_str(&source).map_err(|e| e.to_string())?;
    if docs.len() != 1 {
        return Err(ParseFailure::Documents.into());
    }
    let yaml = docs.remove(0);
    let events = strings_with_witness(&yaml["on"])?;
    let jobs = yaml["jobs"]
        .as_hash()
        .ok_or(ParseFailure::NoJobs)?
        .keys()
        .map(text)
        .collect::<Vec<_>>();
    if events.is_empty() || jobs.is_empty() {
        return Err(ParseFailure::NoSelection.into());
    }
    if jobs.iter().any(|s| s.starts_with('-') || s.contains("${{")) {
        return Err(ParseFailure::InvalidJob.into());
    }
    Ok(Workflow {
        file,
        source,
        events,
        jobs,
        yaml,
    })
}

fn strings_with_witness(value: &Yaml) -> Result<Vec<String>, ParseFailure> {
    let values = match value {
        Yaml::String(s) => vec![s.clone()],
        Yaml::Array(a) => a.iter().map(text).collect(),
        Yaml::Hash(h) => h.keys().map(text).collect(),
        Yaml::Null | Yaml::BadValue => Vec::new(),
        _ => return Err(ParseFailure::ExpectedNames.into()),
    };
    if values
        .iter()
        .any(|s| s.is_empty() || s.starts_with('-') || s.contains("${{"))
    {
        return Err(ParseFailure::DynamicNames.into());
    }
    Ok(values)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ParseFailure {
    Opaque(String),
    TooLarge,
    Aliases,
    StructureLimit,
    Documents,
    NoJobs,
    NoSelection,
    InvalidJob,
    ExpectedNames,
    DynamicNames,
}
impl From<String> for ParseFailure {
    fn from(raw: String) -> Self {
        Self::Opaque(raw)
    }
}
impl ParseFailure {
    pub fn raw(&self) -> String {
        match self { Self::Opaque(s)=>s.clone(),
Self::TooLarge => "Workflow exceeds 1 MiB".into(),
Self::Aliases => "YAML aliases are not supported by the local selector; open this workflow in act directly".into(),
Self::StructureLimit => "Workflow structure exceeds the selector's limits".into(),
Self::Documents => "Expected one workflow document".into(),
Self::NoJobs => "Workflow has no jobs mapping".into(),
Self::NoSelection => "Workflow has no selectable events/jobs".into(),
Self::InvalidJob => "Invalid job id".into(),
Self::ExpectedNames => "Expected a string, list or mapping".into(),
Self::DynamicNames => "Dynamic or invalid event/job names cannot be selected locally".into(),
}
    }
    pub fn display(&self) -> String {
        match self {
            Self::Opaque(s) => s.clone(),
            Self::TooLarge => crate::i18n::text("repo-workflows-local_ci-parse-29e871ff").into(),
            Self::Aliases => crate::i18n::text("repo-workflows-local_ci-parse-be534029").into(),
            Self::StructureLimit => {
                crate::i18n::text("repo-workflows-local_ci-parse-922bc165").into()
            }
            Self::Documents => crate::i18n::text("repo-workflows-local_ci-parse-4e3ad741").into(),
            Self::NoJobs => crate::i18n::text("repo-workflows-local_ci-parse-a0a33130").into(),
            Self::NoSelection => crate::i18n::text("repo-workflows-local_ci-parse-96d73183").into(),
            Self::InvalidJob => crate::i18n::text("repo-workflows-local_ci-parse-598eb5d3").into(),
            Self::ExpectedNames => {
                crate::i18n::text("repo-workflows-local_ci-strings-d5611771").into()
            }
            Self::DynamicNames => {
                crate::i18n::text("repo-workflows-local_ci-strings-5c665142").into()
            }
        }
    }
}
