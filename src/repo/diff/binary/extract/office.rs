use super::*;

pub(super) fn slides(
    members: &[Member],
    reader: &mut Reader<'_>,
    gaps: &mut Vec<String>,
) -> String {
    let order = slide_order(members, reader).unwrap_or_else(|| {
        let mut slides: Vec<(u32, &Member)> = members
            .iter()
            .filter_map(|m| {
                let n = m
                    .name
                    .strip_prefix("ppt/slides/slide")?
                    .strip_suffix(".xml")?;
                Some((n.parse().ok()?, m))
            })
            .collect();
        slides.sort_by_key(|(n, _)| *n);
        slides.into_iter().map(|(_, m)| Some(m)).collect()
    });
    let mut out = String::new();
    for (i, m) in order.into_iter().enumerate() {
        let n = i + 1;
        let Some(xml) = m.and_then(|m| reader.read(m)) else {
            gaps.push(crate::i18n::message("repo-core-3833", &[("n", (n).into())]));
            continue;
        };
        let text = xml_text(&String::from_utf8_lossy(&xml), &Rules::SLIDE);
        out.push_str(&crate::i18n::message(
            "repo-core-3834",
            &[("n", (n).into()), ("text", text.to_string().into())],
        ));
        if out.len() > MAX_TEXT {
            break;
        }
    }
    out
}

/// The slides in the order the presentation shows them: the `sldId`s of its
/// `presentation.xml`, through its relationships to their members (`None` for one missing).
/// `None` when the presentation doesn't say.
fn slide_order<'m>(
    members: &'m [Member],
    reader: &mut Reader<'_>,
) -> Option<Vec<Option<&'m Member>>> {
    let presentation =
        String::from_utf8_lossy(&reader.read_named(members, "ppt/presentation.xml")?).into_owned();
    let rels =
        String::from_utf8_lossy(&reader.read_named(members, "ppt/_rels/presentation.xml.rels")?)
            .into_owned();
    let list = presentation.split("<p:sldIdLst").nth(1)?;
    let list = list.split("</p:sldIdLst>").next().unwrap_or(list);
    let ids: Vec<&str> = list
        .split("<p:sldId ")
        .skip(1)
        .filter_map(|tag| attr_value(tag.split('>').next()?, "r:id"))
        .take(100_000)
        .collect();
    if ids.is_empty() {
        return None;
    }
    // Looked up by maps: a hostile presentation may list many slides.
    let targets: std::collections::HashMap<&str, &str> = rels
        .split("<Relationship ")
        .skip(1)
        .filter_map(|tag| {
            let tag = tag.split('>').next().unwrap_or(tag);
            Some((attr_value(tag, "Id")?, attr_value(tag, "Target")?))
        })
        .collect();
    let mut by_name: std::collections::HashMap<&str, &Member> = std::collections::HashMap::new();
    for m in members {
        by_name.entry(m.name.as_str()).or_insert(m);
    }
    Some(
        ids.into_iter()
            .map(|id| {
                let target = decode_entities(targets.get(id)?);
                // Relative to `ppt/`, or from the package's root.
                let name = match target.strip_prefix('/') {
                    Some(root) => root.to_owned(),
                    None => format!("ppt/{target}"),
                };
                by_name.get(name.as_str()).copied()
            })
            .collect(),
    )
}

/// The value of attribute `name` in the inside of a tag (` name="value"`).
fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let padded = tag
        .find(&key)
        .map(|i| i + key.len())
        .or_else(|| tag.strip_prefix(&key[1..]).map(|_| key.len() - 1))?;
    tag[padded..].split('"').next()
}

/// The cells of a workbook, sheet by sheet: `A1: value` a line, shared strings looked up,
/// inline strings and numbers as they are.
pub(super) fn workbook(
    members: &[Member],
    reader: &mut Reader<'_>,
    gaps: &mut Vec<String>,
) -> String {
    let has_strings = members.iter().any(|m| m.name == "xl/sharedStrings.xml");
    let strings: Vec<String> = match reader.read_named(members, "xl/sharedStrings.xml") {
        Some(x) => shared_strings(&String::from_utf8_lossy(&x)),
        None => {
            if has_strings {
                gaps.push(crate::i18n::text("repo-core-3847").to_owned());
            }
            Vec::new()
        }
    };
    let mut sheets: Vec<(u32, &Member)> = members
        .iter()
        .filter_map(|m| {
            let n = m
                .name
                .strip_prefix("xl/worksheets/sheet")?
                .strip_suffix(".xml")?;
            Some((n.parse().ok()?, m))
        })
        .collect();
    sheets.sort_by_key(|(n, _)| *n);
    let mut out = String::new();
    for (n, m) in sheets {
        let Some(xml) = reader.read(m) else {
            gaps.push(crate::i18n::message("repo-core-3849", &[("n", (n).into())]));
            continue;
        };
        out.push_str(&crate::i18n::message(
            "repo-core-3850",
            &[("n", (n).into())],
        ));
        for (cell, value) in cells(&String::from_utf8_lossy(&xml), &strings) {
            out.push_str(&format!("{cell}: {value}\n"));
            if out.len() > MAX_TEXT {
                return out;
            }
        }
    }
    out
}

/// The shared strings of a workbook, in order (one may span lines).
pub fn shared_strings(xml: &str) -> Vec<String> {
    xml.split("<si")
        .skip(1)
        .map(|si| {
            let si = si.split("</si>").next().unwrap_or(si);
            xml_text(&format!("<si{si}</si>"), &Rules::STRINGS)
        })
        .collect()
}

/// The cells of a worksheet with a value: (reference, value). A formula's cell shows its
/// cached value and the formula (`84 (=B2*2)`), so a formula changed to one giving the same
/// value still shows.
pub fn cells(xml: &str, strings: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<c ") {
        rest = &rest[at..];
        let head_end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let end = if rest[..head_end].ends_with("/>") {
            head_end
        } else {
            rest.find("</c>").map_or(rest.len(), |e| e + 4)
        };
        let cell = &rest[..end];
        rest = &rest[end..];
        let head = cell.split('>').next().unwrap_or("");
        let attr = |name: &str| {
            let key = format!(" {name}=\"");
            head.find(&key)
                .and_then(|i| head[i + key.len()..].split('"').next())
        };
        let reference = attr("r").unwrap_or("?").to_owned();
        // The contents of `<tag>` or `<tag attributes…>`; none for `<tag … />`.
        let inner = |tag: &str| {
            let open = format!("<{tag}");
            let mut from = 0;
            let start = loop {
                let i = from + cell[from..].find(&open)? + open.len();
                match cell[i..].chars().next() {
                    Some('>') => break i + 1,
                    Some(c) if c.is_whitespace() => {
                        let gt = i + cell[i..].find('>')?;
                        if cell[..gt].ends_with('/') {
                            return None;
                        }
                        break gt + 1;
                    }
                    _ => from = i,
                }
            };
            let j = cell[start..].find(&format!("</{tag}>"))?;
            Some(decode_entities(&cell[start..start + j]))
        };
        let value = match attr("t") {
            Some("s") => inner("v")
                .and_then(|v| v.trim().parse::<usize>().ok())
                .and_then(|i| strings.get(i).cloned()),
            Some("inlineStr") => Some(xml_text(
                cell,
                &Rules {
                    runs: &["t"],
                    blocks: &[],
                    breaks: &[],
                    spaces: None,
                    skip: &[],
                },
            )),
            _ => inner("v"),
        };
        let formula = inner("f").filter(|f| !f.is_empty());
        let value = match (value.filter(|v| !v.is_empty()), formula) {
            (Some(v), Some(f)) => Some(format!("{v} (={f})")),
            (None, Some(f)) => Some(format!("={f}")),
            (v, None) => v,
        };
        if let Some(v) = value.filter(|v| !v.is_empty()) {
            out.push((reference, v));
        }
    }
    out
}
