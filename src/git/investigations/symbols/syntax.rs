use super::Symbol;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use tree_sitter::{Language, Node, ParseOptions, Parser};
fn language(path: &str) -> Option<Language> {
    Some(match Path::new(path).extension()?.to_str()? {
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        "py" => tree_sitter_python::LANGUAGE.into(),
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "c" | "h" => tree_sitter_c::LANGUAGE.into(),
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" => tree_sitter_cpp::LANGUAGE.into(),
        _ => return None,
    })
}
pub(super) fn supported(path: &str) -> bool {
    language(path).is_some()
}
fn source<'a>(node: Node, text: &'a str) -> &'a str {
    text.get(node.byte_range()).unwrap_or("")
}
fn function(node: Node) -> bool {
    matches!(
        node.kind(),
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "method_declaration"
    )
}
fn declarator_name(node: Node) -> Option<Node> {
    if matches!(
        node.kind(),
        "identifier"
            | "field_identifier"
            | "qualified_identifier"
            | "operator_name"
            | "destructor_name"
    ) {
        return Some(node);
    }
    node.child_by_field_name("declarator")
        .and_then(declarator_name)
        .or_else(|| node.child_by_field_name("name").and_then(declarator_name))
}
fn name(node: Node) -> Option<Node> {
    node.child_by_field_name("name")
        .or_else(|| {
            node.child_by_field_name("declarator")
                .and_then(declarator_name)
        })
        .or_else(|| {
            node.parent()
                .filter(|n| {
                    matches!(
                        n.kind(),
                        "variable_declarator" | "pair" | "assignment" | "short_var_declaration"
                    )
                })
                .and_then(|n| {
                    n.child_by_field_name("name")
                        .or_else(|| n.child_by_field_name("key"))
                        .or_else(|| n.child_by_field_name("left"))
                })
        })
}
fn tokens(node: Node, text: &str) -> Result<Vec<String>, String> {
    let mut stack = vec![node];
    let mut out = Vec::new();
    let mut visited = 0;
    while let Some(n) = stack.pop() {
        visited += 1;
        if visited > 100_000 {
            return Err("Function exceeds structural node budget".into());
        }
        if n.kind().contains("comment") {
            continue;
        }
        if n.child_count() == 0
            || matches!(
                n.kind(),
                "string" | "string_literal" | "raw_string_literal" | "interpreted_string_literal"
            )
        {
            let s = source(n, text);
            if s.len() > 8192 || out.len() > 50_000 {
                return Err("Function exceeds structural token budget".into());
            }
            if !s.is_empty() {
                out.push(s.into());
            }
        } else {
            let mut cursor = n.walk();
            let children = n.children(&mut cursor).collect::<Vec<_>>();
            stack.extend(children.into_iter().rev());
        }
    }
    Ok(out)
}
pub(super) fn parse(
    path: &str,
    text: &str,
    blob: &str,
    active: &dyn Fn() -> bool,
) -> Result<Vec<Symbol>, String> {
    if !active() {
        return Err("Symbol parsing cancelled".into());
    }
    let mut parser = Parser::new();
    parser
        .set_language(&language(path).ok_or("Unsupported source language")?)
        .map_err(|e| e.to_string())?;
    let began = Instant::now();
    let mut stop =
        |_: &tree_sitter::ParseState| !active() || began.elapsed() > Duration::from_secs(2);
    let tree = parser
        .parse_with_options(
            &mut |offset, _| &text.as_bytes()[offset..],
            None,
            Some(ParseOptions::new().progress_callback(&mut stop)),
        )
        .ok_or("Symbol parsing cancelled or exceeded two seconds")?;
    if tree.root_node().has_error() {
        return Err(format!(
            "Cannot prove symbol lineage: {path} has syntax errors for its grammar"
        ));
    }
    let mut stack = vec![tree.root_node()];
    let mut out = Vec::new();
    let mut visited = 0;
    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > 200_000 || out.len() > 5000 || !active() {
            return Err("Symbol parsing cancelled or exceeded structural budget".into());
        }
        if function(node) {
            if let (Some(n), Some(body)) = (name(node), node.child_by_field_name("body")) {
                let own = source(n, text).trim().to_owned();
                let mut scopes = Vec::new();
                let mut parent = node.parent();
                while let Some(p) = parent {
                    if function(p)
                        || matches!(
                            p.kind(),
                            "impl_item"
                                | "trait_item"
                                | "mod_item"
                                | "class_definition"
                                | "class_declaration"
                                | "class_specifier"
                                | "struct_specifier"
                                | "namespace_definition"
                        )
                    {
                        if let Some(scope) = name(p).or_else(|| p.child_by_field_name("type")) {
                            scopes.push(source(scope, text).trim().to_owned());
                        }
                    }
                    parent = p.parent();
                }
                scopes.reverse();
                scopes.push(own.clone());
                out.push(Symbol {
                    name: own,
                    qualified: scopes.join("::"),
                    path: path.into(),
                    first: node.start_position().row + 1,
                    last: node.end_position().row + 1,
                    body: source(node, text).into(),
                    blob: blob.into(),
                    tokens: tokens(body, text)?,
                });
            }
        }
        let mut cursor = node.walk();
        let children = node.named_children(&mut cursor).collect::<Vec<_>>();
        stack.extend(children.into_iter().rev());
    }
    Ok(out)
}
