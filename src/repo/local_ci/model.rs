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
        _ => return Err("Expected a string, list or mapping".into()),
    };
    if values
        .iter()
        .any(|s| s.is_empty() || s.starts_with('-') || s.contains("${{"))
    {
        return Err("Dynamic or invalid event/job names cannot be selected locally".into());
    }
    Ok(values)
}
pub(super) fn parse(file: String, source: String) -> Result<Workflow, String> {
    if source.len() > MAX {
        return Err("Workflow exceeds 1 MiB".into());
    }
    let mut parser = Parser::new(source.chars());
    let mut depth = 0usize;
    let mut count = 0usize;
    loop {
        let (event, _) = parser.next_token().map_err(|e| e.to_string())?;
        count += 1;
        match event {
            Event::Alias(_) => return Err("YAML aliases are not supported by the local selector; open this workflow in act directly".into()),
            Event::MappingStart(..) | Event::SequenceStart(..) => depth += 1,
            Event::MappingEnd | Event::SequenceEnd => depth = depth.saturating_sub(1),
            Event::StreamEnd => break,
            _ => {}
        }
        if depth > 64 || count > 100_000 {
            return Err("Workflow structure exceeds the selector's limits".into());
        }
    }
    let mut docs = YamlLoader::load_from_str(&source).map_err(|e| e.to_string())?;
    if docs.len() != 1 {
        return Err("Expected one workflow document".into());
    }
    let yaml = docs.remove(0);
    let events = strings(&yaml["on"])?;
    let jobs = yaml["jobs"]
        .as_hash()
        .ok_or("Workflow has no jobs mapping")?
        .keys()
        .map(text)
        .collect::<Vec<_>>();
    if events.is_empty() || jobs.is_empty() {
        return Err("Workflow has no selectable events/jobs".into());
    }
    if jobs.iter().any(|s| s.starts_with('-') || s.contains("${{")) {
        return Err("Invalid job id".into());
    }
    Ok(Workflow {
        file,
        source,
        events,
        jobs,
        yaml,
    })
}
fn map() -> Yaml {
    Yaml::Hash(Default::default())
}
fn put(yaml: &mut Yaml, key: &str, value: Yaml) -> Result<(), String> {
    yaml.as_mut_hash()
        .ok_or("Expected a mapping")?
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
        let fields = matrix.as_hash().ok_or(
            "Expression matrices need an evaluated event/context; act cannot enumerate them here",
        )?;
        let mut entries = vec![map()];
        for (key, values) in fields {
            let key = key.as_str().ok_or("Matrix axis must be a string")?;
            if key == "include" || key == "exclude" {
                continue;
            }
            let values = values
                .as_vec()
                .ok_or("Expression matrix axes cannot be enumerated here")?;
            if values.iter().any(|v| text(v).contains("${{")) {
                return Err("Expression matrix values cannot be enumerated here".into());
            }
            let mut next = Vec::new();
            for entry in &entries {
                for value in values {
                    let mut e = entry.clone();
                    put(&mut e, key, value.clone())?;
                    next.push(e);
                    if next.len() > 256 {
                        return Err("Matrix exceeds 256 entries".into());
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
                let excluded = item
                    .as_hash()
                    .ok_or("Matrix exclude must contain mappings")?;
                if excluded.keys().any(|key| {
                    !fields.contains_key(key)
                        || ["include", "exclude"].contains(&key.as_str().unwrap_or(""))
                }) {
                    return Err("Matrix exclude names an unknown axis".into());
                }
            }
            entries.retain(|e| !exclude.iter().any(|p| matches(e, p)));
        } else if !matches!(matrix["exclude"], Yaml::BadValue | Yaml::Null) {
            return Err("Matrix exclude must be a list".into());
        }
        if let Some(include) = matrix["include"].as_vec() {
            for addition in include {
                let fields = addition
                    .as_hash()
                    .ok_or("Matrix include must contain mappings")?;
                if text(addition).contains("${{") {
                    return Err("Expression matrix includes cannot be enumerated here".into());
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
                                        .ok_or("Invalid matrix entry")?
                                        .insert(k.clone(), v.clone());
                                }
                                applied = true;
                            }
                        }
                    }
                }
                if !applied {
                    entries.push(addition.clone());
                }
                if entries.len() > 256 {
                    return Err("Matrix exceeds 256 entries".into());
                }
            }
        } else if !matches!(matrix["include"], Yaml::BadValue | Yaml::Null) {
            return Err("Matrix include must be a list".into());
        }
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
                return Err("Cyclic job dependencies".into());
            }
            if seen.iter().any(|s| s == j) {
                return Ok(());
            }
            if !w.jobs.iter().any(|s| s == j) {
                return Err(format!("Missing dependency job {j}"));
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
            return Err("Selection no longer belongs to this workflow".into());
        }
        if !self.matrices(job)?.contains(entry) {
            return Err("Selected matrix no longer exists".into());
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
                .ok_or("Missing selected job")?;
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
