//! Human-readable renderings of an analysis: Markdown for reviewers and Graphviz for the graph.

use std::fmt::Write;

use crate::analysis::Analysis;
use crate::rules::Outcome;

fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

pub fn render(a: &Analysis, outcome: Option<&Outcome>) -> String {
    let mut md = String::new();
    let s = &a.summary;
    let _ = writeln!(md, "# Architecture fitness report\n");
    let source = a.source.revision.as_deref().unwrap_or("working tree");
    let _ = writeln!(
        md,
        "Source: `{source}` · policy `{}` · schema {}\n",
        a.policy_version, a.schema
    );
    let _ = writeln!(
        md,
        "{} crates, {} files, {} modules, {} items in {} domains. {} domain edges ({} forbidden by policy), {} cycles, {} `RepoTab` fields.\n",
        s.crates,
        s.files,
        s.modules,
        s.items,
        s.domains,
        s.domain_edges,
        s.forbidden_edges,
        s.cycles,
        s.repo_tab_fields
    );
    let st = &s.stats;
    let _ = writeln!(
        md,
        "Resolution: {} local paths, {} external, {} unresolved qualified paths, {} unresolved simple names; {} method calls (not resolved: no type inference); {} macro bodies parsed, {} unparsed; {} tab-field accesses with an unknown receiver.\n",
        st.paths_local,
        st.paths_external,
        st.paths_unresolved,
        st.names_unresolved,
        st.method_calls,
        st.macro_bodies_parsed,
        st.macro_bodies_unparsed,
        st.tab_field_unattributed
    );
    if let Some(outcome) = outcome {
        violations(&mut md, outcome);
    }
    domains(&mut md, a);
    edges(&mut md, a);
    tab(&mut md, a);
    gaps(&mut md, a);
    md
}

fn violations(md: &mut String, outcome: &Outcome) {
    let _ = writeln!(md, "## Check result\n");
    if outcome.violations.is_empty() {
        let _ = writeln!(md, "No violations.\n");
    }
    for v in &outcome.violations {
        let _ = writeln!(md, "- **{}**: {}", v.rule, v.message);
        if let Some(location) = &v.location {
            let _ = writeln!(md, "  - at `{location}`");
        }
        if let Some(symbol) = v.symbol.as_deref().filter(|s| !s.is_empty()) {
            let _ = writeln!(md, "  - symbol: {symbol}");
        }
        let _ = writeln!(md, "  - fix: {}", v.fix);
    }
    if !outcome.ratchet.is_empty() {
        let _ = writeln!(
            md,
            "\nBudgets that can be lowered (`ratchet`): {}\n",
            outcome.ratchet.join("; ")
        );
    }
    md.push('\n');
}

fn domains(md: &mut String, a: &Analysis) {
    let _ = writeln!(md, "## Domains\n");
    let _ = writeln!(
        md,
        "| Domain | Layer | Modules | Files | Items | Exported | Fan-out | Fan-in | Foreign symbols used | Own symbols used elsewhere | Test fan-out |"
    );
    let _ = writeln!(
        md,
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
    );
    for (name, d) in a.domains.iter().filter(|(_, d)| d.modules > 0) {
        let _ = writeln!(
            md,
            "| {name} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            d.layer,
            d.modules,
            d.files,
            d.items,
            d.exported,
            d.fan_out,
            d.fan_in,
            d.fan_out_symbols,
            d.fan_in_symbols,
            d.test_fan_out
        );
    }
    md.push('\n');
    if !a.cycles.is_empty() {
        let _ = writeln!(md, "### Cycles (strongly connected domains)\n");
        for c in &a.cycles {
            let _ = writeln!(md, "- {}", c.join(" ⇄ "));
        }
        md.push('\n');
    }
    let _ = writeln!(md, "### Modules with the most foreign dependencies\n");
    let _ = writeln!(
        md,
        "| Module | Domain | Foreign domains | Foreign symbols |\n| --- | --- | ---: | ---: |"
    );
    for m in a.module_fan_out.iter().take(15) {
        let _ = writeln!(
            md,
            "| `{}` | {} | {} | {} |",
            m.module, m.domain, m.foreign_domains, m.foreign_symbols
        );
    }
    md.push('\n');
}

fn edges(md: &mut String, a: &Analysis) {
    let _ = writeln!(md, "## Forbidden-by-policy edges\n");
    let _ = writeln!(
        md,
        "| From | To | References | Symbols | First site |\n| --- | --- | ---: | ---: | --- |"
    );
    for e in a.edges.iter().filter(|e| e.policy == "forbidden") {
        let site = e
            .samples
            .first()
            .map(|s| format!("`{}:{}` {}", s.file, s.line, cell(&s.symbol)))
            .unwrap_or_default();
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} | {site} |",
            e.from,
            e.to,
            e.references,
            e.symbols.len()
        );
    }
    let _ = writeln!(
        md,
        "\n### Cross-domain re-exports ({})\n",
        a.reexports.len()
    );
    for r in a.reexports.iter().take(40) {
        let _ = writeln!(
            md,
            "- `{}` re-exports `{}` ({}) at `{}:{}`",
            r.module, r.target, r.target_domain, r.file, r.line
        );
    }
    let _ = writeln!(
        md,
        "\n<details><summary>All {} domain edges</summary>\n",
        a.edges.len()
    );
    let _ = writeln!(
        md,
        "| From | To | Policy | References | Symbols |\n| --- | --- | --- | ---: | ---: |"
    );
    for e in &a.edges {
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} | {} |",
            e.from,
            e.to,
            e.policy,
            e.references,
            e.symbols.len()
        );
    }
    let _ = writeln!(md, "\n</details>\n");
}

fn tab(md: &mut String, a: &Analysis) {
    let t = &a.repo_tab;
    let _ = writeln!(
        md,
        "## `RepoTab` state ({} fields, `{}`)\n",
        t.fields.len(),
        t.file
    );
    let _ = writeln!(
        md,
        "A field count is not a finding; mutation by a domain that does not own the field is.\n"
    );
    let _ = writeln!(
        md,
        "### Field hotspots\n\n| Field | Owner | Domains mutating | Domains reading |\n| --- | --- | ---: | ---: |"
    );
    for h in t.field_hotspots.iter().take(25) {
        let owner = t
            .fields
            .iter()
            .find(|f| f.name == h.name)
            .map_or("", |f| f.owner.as_str());
        let _ = writeln!(
            md,
            "| `{}` | {owner} | {} | {} |",
            h.name, h.domains_mutating, h.domains_reading
        );
    }
    let _ = writeln!(
        md,
        "\n### Domains by tab state touched\n\n| Domain | Fields read | Fields mutated | Foreign fields mutated |\n| --- | ---: | ---: | ---: |"
    );
    for d in t.domain_use.iter().take(25) {
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} |",
            d.domain, d.fields_read, d.fields_mutated, d.foreign_fields_mutated
        );
    }
    let _ = writeln!(
        md,
        "\n### Cross-feature mutations ({})\n",
        t.cross_mutations.len()
    );
    let _ = writeln!(
        md,
        "| Domain | Field | Owner | Sites | First site |\n| --- | --- | --- | ---: | --- |"
    );
    for m in &t.cross_mutations {
        let site = m
            .samples
            .first()
            .map(|s| format!("`{}:{}` {:?}", s.file, s.line, s.mode))
            .unwrap_or_default();
        let _ = writeln!(
            md,
            "| {} | `{}` | {} | {} | {site} |",
            m.domain, m.field, m.owner, m.count
        );
    }
    md.push('\n');
}

fn gaps(md: &mut String, a: &Analysis) {
    let g = &a.gaps;
    let _ = writeln!(md, "## Analysis gaps\n");
    for (kind, count) in &g.counts {
        let _ = writeln!(md, "- {kind}: {count}");
    }
    for gap in g.items.iter().take(50) {
        let _ = writeln!(
            md,
            "- `{}:{}` {}: {}",
            gap.file,
            gap.line,
            gap.kind,
            cell(&gap.detail)
        );
    }
    if !g.unresolved_paths.is_empty() {
        let _ = writeln!(md, "\nUnresolved paths ({}):\n", g.unresolved_paths.len());
        for gap in g.unresolved_paths.iter().take(50) {
            let _ = writeln!(md, "- `{}:{}` {}", gap.file, gap.line, cell(&gap.detail));
        }
    }
    let mut macros: Vec<_> = g.unparsed_macros.iter().collect();
    macros.sort_by(|x, y| y.1.cmp(x.1).then(x.0.cmp(y.0)));
    let list: Vec<String> = macros
        .iter()
        .take(15)
        .map(|(n, c)| format!("`{n}!` {c}"))
        .collect();
    let _ = writeln!(
        md,
        "\nUnparsed macro bodies (paths inside are unseen): {}\n",
        list.join(", ")
    );
    let _ = writeln!(md, "Ignored files: {}\n", g.ignored_files.len());
}

/// The domain graph in Graphviz syntax; forbidden edges are red.
pub fn dot(a: &Analysis) -> String {
    let mut out = String::from("digraph domains {\n  rankdir=LR;\n  node [shape=box];\n");
    for (name, d) in a.domains.iter().filter(|(_, d)| d.modules > 0) {
        let _ = writeln!(out, "  \"{name}\" [label=\"{name}\\n{}\"];", d.layer);
    }
    for e in &a.edges {
        let color = if e.policy == "forbidden" {
            "red"
        } else {
            "gray40"
        };
        let _ = writeln!(
            out,
            "  \"{}\" -> \"{}\" [color={color}, label=\"{}\"];",
            e.from, e.to, e.references
        );
    }
    out.push_str("}\n");
    out
}
