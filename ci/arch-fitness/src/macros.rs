//! Parsers for macro bodies that are not plain expression lists.

use syn::parse::ParseStream;
use syn::visit::Visit;
use syn::{Expr, Pat, Token};

use crate::visit::Visitor;

/// `matches!(expr, pattern [if guard])`.
pub fn parse_matches(input: ParseStream) -> syn::Result<(Expr, Pat, Option<Expr>)> {
    let expr: Expr = input.parse()?;
    input.parse::<Token![,]>()?;
    let pat = Pat::parse_multi_with_leading_vert(input)?;
    let guard = if input.peek(Token![if]) {
        input.parse::<Token![if]>()?;
        Some(input.parse()?)
    } else {
        None
    };
    input.parse::<Option<Token![,]>>()?;
    Ok((expr, pat, guard))
}

impl Visitor<'_, '_> {
    pub(crate) fn visit_macro_body(&mut self, mac: &syn::Macro) {
        use syn::punctuated::Punctuated;
        let name = mac
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if name == "matches"
            && let Ok((expr, pat, guard)) = mac.parse_body_with(parse_matches)
        {
            {
                self.out.stats.macro_bodies_parsed += 1;
                self.visit_expr(&expr);
                self.frame(|v| {
                    v.bind_pat(&pat, None);
                    v.visit_pat(&pat);
                    if let Some(guard) = &guard {
                        v.visit_expr(guard);
                    }
                });
                return;
            }
        }
        let comma = mac
            .parse_body_with(Punctuated::<Expr, syn::Token![,]>::parse_terminated)
            .map(|p| p.into_iter().collect::<Vec<_>>());
        let semi = || {
            mac.parse_body_with(Punctuated::<Expr, syn::Token![;]>::parse_terminated)
                .map(|p| p.into_iter().collect::<Vec<_>>())
        };
        match comma.or_else(|_| semi()) {
            Ok(exprs) => {
                self.out.stats.macro_bodies_parsed += 1;
                exprs.iter().for_each(|e| self.visit_expr(e));
            }
            Err(_) if mac.tokens.is_empty() => {}
            Err(_) => {
                self.out.stats.macro_bodies_unparsed += 1;
                *self.out.unparsed_macros.entry(name).or_default() += 1;
            }
        }
    }
}
