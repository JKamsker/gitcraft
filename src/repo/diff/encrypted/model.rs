//! Recognition and metadata comparison inspect ciphertext only. No key lookup here.
use serde_json::{Map, Value};
use yaml_rust2::{
    Yaml, YamlLoader,
    scanner::{Scanner, TokenType},
};

pub(super) const MAX_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_LINES: usize = 20_000;
const BAD: &str = "repo-core-1724";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Format {
    Json,
    Yaml,
    Dotenv,
    Ini,
    Binary,
}
impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Dotenv => "dotenv",
            Self::Ini => "ini",
            Self::Binary => "binary",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Json => "gitcraft.json",
            Self::Yaml => "gitcraft.yaml",
            Self::Dotenv => "gitcraft.env",
            Self::Ini => "gitcraft.ini",
            Self::Binary => "gitcraft.bin",
        }
    }
}

pub(super) struct Envelope {
    pub format: Format,
    pub metadata: Value,
    // A binary store and a JSON document with one `data` field have identical
    // ciphertext shapes. Neither the filename nor the encrypted value disambiguates.
    pub ambiguous_json: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum JsonEncoding {
    #[default]
    Ask,
    Document,
    Binary,
}
impl JsonEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => crate::i18n::text("repo-core-1734"),
            Self::Document => crate::i18n::text("repo-core-1735"),
            Self::Binary => crate::i18n::text("repo-core-1736"),
        }
    }
}

pub(super) fn inspect(path: &str, text: &str) -> Result<Option<Envelope>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    if text.len() > MAX_BYTES || text.contains('\0') {
        return Err(crate::i18n::text("repo-core-1737").into());
    }
    let lower = path.to_ascii_lowercase();
    let json = if text.trim_start().starts_with('{') {
        serde_json::from_str::<Value>(text).ok()
    } else {
        None
    };
    let ambiguous_json = json.as_ref().is_some_and(|value| {
        value.as_object().is_some_and(|object| {
            object.len() == 2 && object.contains_key("data") && object.contains_key("sops")
        })
    });
    let (format, metadata) = if let Some(value) = json {
        (Format::Json, value.get("sops").cloned())
    } else if lower.ends_with(".env") || text.lines().any(|line| line.starts_with("sops_mac=")) {
        let mut meta = Map::new();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=')
                && let Some(key) = key.strip_prefix("sops_")
                && meta
                    .insert(key.to_owned(), Value::String(value.trim().to_owned()))
                    .is_some()
            {
                return Err(crate::i18n::text(BAD).into());
            }
        }
        (
            Format::Dotenv,
            (!meta.is_empty()).then_some(Value::Object(meta)),
        )
    } else if lower.ends_with(".ini") || text.lines().any(|line| line.trim() == "[sops]") {
        let (mut meta, mut inside) = (Map::new(), false);
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                inside = line == "[sops]";
                continue;
            }
            if inside && !line.starts_with(['#', ';']) && !line.is_empty() {
                let (key, value) = line.split_once('=').ok_or(crate::i18n::text(BAD))?;
                if meta
                    .insert(
                        key.trim().to_owned(),
                        Value::String(value.trim().to_owned()),
                    )
                    .is_some()
                {
                    return Err(crate::i18n::text(BAD).into());
                }
            }
        }
        (
            Format::Ini,
            (!meta.is_empty()).then_some(Value::Object(meta)),
        )
    } else {
        // yaml-rust's loader clones aliases. Reject them before loading, and bound nesting
        // and total tokens so repository input cannot expand into an unbounded graph.
        let mut scanner = Scanner::new(text.chars());
        let (mut depth, mut tokens) = (0usize, 0usize);
        for token in scanner.by_ref() {
            tokens += 1;
            match token.1 {
                TokenType::Alias(_)|TokenType::Anchor(_)=>return Err(crate::i18n::text("repo-core-1745").into()),
                TokenType::BlockSequenceStart|TokenType::BlockMappingStart|TokenType::FlowSequenceStart|TokenType::FlowMappingStart=>depth+=1,
                TokenType::BlockEnd|TokenType::FlowSequenceEnd|TokenType::FlowMappingEnd=>depth=depth.saturating_sub(1),
                _=>{}
            }
            if depth > 64 || tokens > 100_000 {
                return Err(crate::i18n::text("repo-core-1746").into());
            }
        }
        if scanner.get_error().is_some() {
            return Err(crate::i18n::text(BAD).into());
        }
        let docs = YamlLoader::load_from_str(text).map_err(|_| crate::i18n::text(BAD))?;
        let mut metadata = Vec::new();
        for doc in docs {
            if let Some(map) = doc.as_hash()
                && let Some(meta) = map.get(&Yaml::String("sops".into()))
            {
                metadata.push(yaml_value(meta)?);
            }
        }
        (
            Format::Yaml,
            (!metadata.is_empty()).then_some(Value::Array(metadata)),
        )
    };
    let Some(metadata) = metadata else {
        return Ok(None);
    };
    // One canonical document-list shape keeps JSON and YAML metadata comparable.
    let metadata = match metadata {
        Value::Array(docs) => Value::Array(docs),
        one => Value::Array(vec![one]),
    };
    let valid = |meta: &Value| {
        meta.get("version")
            .and_then(Value::as_str)
            .is_some_and(|v| !v.is_empty())
            && meta
                .get("mac")
                .and_then(Value::as_str)
                .is_some_and(|s| s.starts_with("ENC[") && s.ends_with(']'))
    };
    if !match &metadata {
        Value::Array(docs) => docs.iter().all(valid),
        _ => valid(&metadata),
    } {
        return Err(crate::i18n::text(BAD).into());
    }
    Ok(Some(Envelope {
        format,
        metadata,
        ambiguous_json,
    }))
}

fn yaml_value(yaml: &Yaml) -> Result<Value, String> {
    Ok(match yaml {
        Yaml::String(s) | Yaml::Real(s) => Value::String(s.clone()),
        Yaml::Integer(n) => Value::Number((*n).into()),
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Null => Value::Null,
        Yaml::Array(items) => Value::Array(items.iter().map(yaml_value).collect::<Result<_, _>>()?),
        Yaml::Hash(map) => {
            let mut out = Map::new();
            for (key, value) in map {
                out.insert(key.as_str().ok_or(crate::i18n::text(BAD))?.to_owned(), yaml_value(value)?);
            }
            Value::Object(out)
        }
        _ => return Err(crate::i18n::text(BAD).into()),
    })
}

pub(super) fn recipients(metadata: Option<&Envelope>) -> Value {
    fn keys(meta: &Value) -> Value {
        match meta {
            Value::Array(docs) => Value::Array(docs.iter().map(keys).collect()),
            Value::Object(map) => Value::Object(
                map.iter()
                    .filter(|(key, _)| {
                        let base = key.split("__").next().unwrap_or(key);
                        [
                            "age",
                            "pgp",
                            "kms",
                            "gcp_kms",
                            "azure_kv",
                            "hc_vault",
                            "key_groups",
                            "shamir_threshold",
                        ]
                        .contains(&base)
                    })
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
            _ => Value::Null,
        }
    }
    metadata.map_or(Value::Null, |envelope| keys(&envelope.metadata))
}
