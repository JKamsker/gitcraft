//! Small, total helpers over `syn` syntax: attributes, visibility, spans and paths.

use quote::ToTokens;
use syn::spanned::Spanned;
use syn::{Attribute, Meta, Visibility};

use crate::model::VisSyntax;

pub fn line_of(node: &impl Spanned) -> usize {
    node.span().start().line
}

pub fn tokens(node: &impl ToTokens) -> String {
    node.to_token_stream().to_string()
}

/// The predicates of every `#[cfg(...)]` attribute, as normalized token text.
pub fn cfgs(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("cfg"))
        .filter_map(|a| match &a.meta {
            Meta::List(list) => Some(list.tokens.to_string()),
            _ => None,
        })
        .collect()
}

/// Evaluate a predicate with the flags a crate fixes (`flag` or `key="value"`); unknown
/// flags make the result unknown (`None`), so only provably inactive code is skipped.
pub fn eval_cfg(
    predicate: &Meta,
    known: &std::collections::BTreeMap<String, bool>,
) -> Option<bool> {
    let nested = |list: &syn::MetaList| {
        list.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            .map(|n| n.iter().map(|m| eval_cfg(m, known)).collect::<Vec<_>>())
            .ok()
    };
    match predicate {
        Meta::Path(path) => known.get(&tokens(path)).copied(),
        Meta::NameValue(nv) => known.get(&tokens(nv).replace(' ', "")).copied(),
        Meta::List(list) if list.path.is_ident("not") => {
            nested(list)?.first().copied().flatten().map(|b| !b)
        }
        Meta::List(list) if list.path.is_ident("all") => {
            let values = nested(list)?;
            if values.contains(&Some(false)) {
                Some(false)
            } else {
                values.iter().all(Option::is_some).then_some(true)
            }
        }
        Meta::List(list) if list.path.is_ident("any") => {
            let values = nested(list)?;
            if values.contains(&Some(true)) {
                Some(true)
            } else {
                values.iter().all(Option::is_some).then_some(false)
            }
        }
        Meta::List(_) => None,
    }
}

/// Whether every `#[cfg]` of these attributes may hold for the crate's fixed flags.
pub fn cfg_active(attrs: &[Attribute], known: &std::collections::BTreeMap<String, bool>) -> bool {
    attrs.iter().filter(|a| a.path().is_ident("cfg")).all(|a| {
        a.parse_args::<Meta>()
            .map_or(true, |m| eval_cfg(&m, known) != Some(false))
    })
}

/// Whether a predicate can only hold when compiling tests (`test`, or `all(.., test, ..)`).
pub fn requires_test(predicate: &Meta) -> bool {
    match predicate {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            .map(|nested| nested.iter().any(requires_test))
            .unwrap_or(false),
        _ => false,
    }
}

/// Test-only code: `#[cfg(test)]`-like predicates or a test attribute (`#[test]`, `#[tokio::test]`).
pub fn is_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        if a.path().is_ident("cfg") {
            return a.parse_args::<Meta>().is_ok_and(|m| requires_test(&m));
        }
        a.path().segments.last().is_some_and(|s| s.ident == "test")
    })
}

/// The string of a `#[path = "..."]` attribute.
pub fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|a| match &a.meta {
        Meta::NameValue(nv) if nv.path.is_ident("path") => match &nv.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => Some(s.value()),
            _ => None,
        },
        _ => None,
    })
}

/// `#[cfg_attr(.., path = ..)]` selects a module file per configuration.
pub fn has_cfg_attr_path(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("cfg_attr") && tokens(&a.meta).contains("path"))
}

pub fn vis(vis: &Visibility) -> VisSyntax {
    match vis {
        Visibility::Public(_) => VisSyntax::Public,
        Visibility::Inherited => VisSyntax::Private,
        Visibility::Restricted(r) => {
            let path = path_idents(&r.path);
            match path.as_slice() {
                [one] if one == "crate" => VisSyntax::Crate,
                [one] if one == "super" => VisSyntax::Super,
                [one] if one == "self" => VisSyntax::SelfModule,
                _ => VisSyntax::InPath(path),
            }
        }
    }
}

pub fn path_idents(path: &syn::Path) -> Vec<String> {
    path.segments.iter().map(|s| s.ident.to_string()).collect()
}

/// Join a relative path onto a directory and normalize `.`/`..`, using `/` separators.
pub fn join(dir: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let relative = relative.replace('\\', "/");
    let start = if relative.starts_with('/') {
        String::new()
    } else {
        dir.to_owned()
    };
    for part in start.split('/').chain(relative.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub fn parent_dir(file: &str) -> String {
    match file.rfind('/') {
        Some(at) => file[..at].to_owned(),
        None => String::new(),
    }
}

/// The identifier a function returns, looking through `&`, `&mut`, and one-argument wrappers.
pub fn return_ident(output: &syn::ReturnType) -> Option<String> {
    match output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, ty) => type_ident(ty),
    }
}

/// The path a type is "about": `Option<RefCell<terminals::Terminals>>` → `terminals::Terminals`.
/// Collections are not looked through: a `Vec<Commit>` is the tab's own list.
pub fn type_head(ty: &syn::Type) -> Vec<String> {
    const WRAPPERS: &[&str] = &[
        "Option", "Box", "RefCell", "Cell", "Rc", "Arc", "Mutex", "RwLock",
    ];
    match ty {
        syn::Type::Reference(r) => type_head(&r.elem),
        syn::Type::Paren(p) => type_head(&p.elem),
        syn::Type::Group(g) => type_head(&g.elem),
        syn::Type::Path(p) if p.qself.is_none() => {
            let Some(last) = p.path.segments.last() else {
                return Vec::new();
            };
            if WRAPPERS.contains(&last.ident.to_string().as_str())
                && let syn::PathArguments::AngleBracketed(args) = &last.arguments
                && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
            {
                return type_head(inner);
            }
            path_idents(&p.path)
        }
        _ => Vec::new(),
    }
}

pub fn type_ident(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Reference(r) => type_ident(&r.elem),
        syn::Type::Paren(p) => type_ident(&p.elem),
        syn::Type::Group(g) => type_ident(&g.elem),
        syn::Type::Path(p) => {
            let last = p.path.segments.last()?;
            let wrapped = ["Option", "Box", "Result", "RefMut", "Ref", "MutexGuard"];
            if wrapped.contains(&last.ident.to_string().as_str())
                && let syn::PathArguments::AngleBracketed(args) = &last.arguments
                && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
            {
                return type_ident(inner);
            }
            Some(last.ident.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_and_normalizes() {
        assert_eq!(
            join("src/repo", "tab_owner/model.rs"),
            "src/repo/tab_owner/model.rs"
        );
        assert_eq!(join("src/repo", "../git/x.rs"), "src/git/x.rs");
        assert_eq!(join("", "./a.rs"), "a.rs");
        assert_eq!(parent_dir("src/lib.rs"), "src");
        assert_eq!(parent_dir("lib.rs"), "");
    }

    #[test]
    fn test_predicates() {
        let parse = |s: &str| syn::parse_str::<Meta>(s).ok();
        assert!(parse("test").is_some_and(|m| requires_test(&m)));
        assert!(parse("all(unix, test)").is_some_and(|m| requires_test(&m)));
        assert!(!parse("any(unix, test)").is_some_and(|m| requires_test(&m)));
        assert!(!parse("not(test)").is_some_and(|m| requires_test(&m)));
    }

    #[test]
    fn evaluates_known_flags_only() {
        let known: std::collections::BTreeMap<String, bool> =
            [("recording".to_owned(), true)].into();
        let eval = |s: &str| {
            syn::parse_str::<Meta>(s)
                .ok()
                .and_then(|m| eval_cfg(&m, &known))
        };
        assert_eq!(eval("recording"), Some(true));
        assert_eq!(eval("all(test, not(recording))"), Some(false));
        assert_eq!(eval("any(unix, recording)"), Some(true));
        assert_eq!(eval("unix"), None);
        assert_eq!(eval("all(unix, recording)"), None);
    }
}
