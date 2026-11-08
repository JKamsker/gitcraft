//! The impact graph built from what was read, as the filters say: the changed symbols on the
//! left (with the changed files no analyzer reads), the code referring to them in the middle,
//! the tests referring to them on the right.

use std::collections::BTreeMap;

use egui::{Color32, Pos2};

use super::super::graph::{self, Arrange, Edge, Graph, Node, NodeShape};
use crate::insights::Evidence;
use crate::insights::impact::{Change, Impact};
use crate::theme::Tokens;

/// At most so many referring sites are drawn; the tables list them all.
pub(super) const MAX_SITES: usize = 40;
/// At most so many changed symbols are drawn (those referred to most first), and so many files
/// in other languages: a column stays readable; the tables list them all.
pub(super) const MAX_SYMBOLS: usize = 24;
pub(super) const MAX_OTHER: usize = 8;

/// Which references are shown, by where the code referring is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Scope {
    #[default]
    All,
    /// From files the change changed.
    InDiff,
    /// From files it left alone: affected but unchanged.
    OutsideDiff,
}

impl Scope {
    pub(super) const ALL: [Scope; 3] = [Scope::All, Scope::InDiff, Scope::OutsideDiff];

    pub(super) fn label(self) -> &'static str {
        match self {
            Scope::All => "All references",
            Scope::InDiff => "From changed files",
            Scope::OutsideDiff => "From unchanged files (affected)",
        }
    }
}

/// What the graph and the tables show of what was read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Filter {
    /// Only symbols declared `pub`.
    pub(crate) public_only: bool,
    /// Only this symbol.
    pub(crate) symbol: Option<usize>,
    pub(crate) scope: Scope,
}

impl Filter {
    /// The symbols shown.
    pub(super) fn symbols(&self, data: &Impact) -> Vec<usize> {
        (0..data.symbols.len())
            .filter(|&i| self.symbol.is_none_or(|s| s == i))
            .filter(|&i| !self.public_only || data.symbols[i].public)
            .collect()
    }

    /// The references shown.
    pub(super) fn refs(&self, data: &Impact) -> Vec<usize> {
        let symbols = self.symbols(data);
        (0..data.refs.len())
            .filter(|&r| symbols.contains(&data.refs[r].symbol))
            .filter(|&r| {
                let site = &data.sites[data.refs[r].site];
                match self.scope {
                    Scope::All => true,
                    Scope::InDiff => site.in_diff,
                    Scope::OutsideDiff => !site.in_diff,
                }
            })
            .collect()
    }
}

/// What a node of the graph stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeRef {
    Symbol(usize),
    Site(usize),
    Other(usize),
}

/// What an edge stands for: the references of a site to a symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EdgeRef {
    pub(crate) symbol: usize,
    pub(crate) site: usize,
    pub(crate) refs: Vec<usize>,
}

/// The graph, where its nodes go, and what each node and edge stands for.
#[derive(Clone, Debug, Default)]
pub(crate) struct Laid {
    pub(crate) graph: Graph,
    pub(crate) positions: Vec<Pos2>,
    pub(crate) nodes: Vec<NodeRef>,
    pub(crate) edges: Vec<EdgeRef>,
    /// Sites, symbols and files in other languages left out of the picture for room (listed
    /// in the tables).
    pub(crate) hidden: usize,
    pub(crate) hidden_symbols: usize,
    pub(crate) hidden_other: usize,
    /// The most nodes in one column: how tall the picture has to be.
    pub(crate) tallest: usize,
}

pub(super) fn change_color(t: &Tokens, change: Change) -> Color32 {
    match change {
        Change::Added => t.success,
        Change::Removed => t.danger,
        Change::Modified => t.accent,
    }
}

pub(super) fn change_badge(change: Change) -> &'static str {
    match change {
        Change::Added => "+",
        Change::Removed => "-",
        Change::Modified => "~",
    }
}

/// Builds the graph of `data` as `filter` says, laid out in columns or by force.
pub(crate) fn build(data: &Impact, filter: &Filter, force: bool, t: &Tokens) -> Laid {
    let mut laid = Laid::default();
    let mut symbols = filter.symbols(data);
    let mut refs = filter.refs(data);
    // The symbols referred to most, as many as there is room for, in their order.
    if symbols.len() > MAX_SYMBOLS {
        let mut by_refs: BTreeMap<usize, usize> = BTreeMap::new();
        for &r in &refs {
            *by_refs.entry(data.refs[r].symbol).or_default() += 1;
        }
        let mut ranked = symbols.clone();
        ranked.sort_by_key(|i| (std::cmp::Reverse(by_refs.get(i).copied().unwrap_or(0)), *i));
        ranked.truncate(MAX_SYMBOLS);
        laid.hidden_symbols = symbols.len() - MAX_SYMBOLS;
        symbols.retain(|i| ranked.contains(i));
        refs.retain(|&r| symbols.contains(&data.refs[r].symbol));
    }
    let mut index: BTreeMap<usize, usize> = BTreeMap::new();
    for &i in &symbols {
        let s = &data.symbols[i];
        let mut node = Node::new(s.path.clone(), change_color(t, s.change));
        node.shape = NodeShape::Pill;
        node.badge = Some(change_badge(s.change).to_owned());
        node.column = 0;
        let at = s
            .at()
            .map_or(String::new(), |sp| format!("{}:{}", sp.path, sp.line));
        node.tip = format!(
            "{} {}{} · {} · {} changed line{} · {at}{}",
            s.kind.word(),
            s.name,
            if s.public { " (pub)" } else { "" },
            s.change.word(),
            s.lines,
            if s.lines == 1 { "" } else { "s" },
            if s.test { " · a test" } else { "" },
        );
        index.insert(i, laid.graph.nodes.len());
        laid.graph.nodes.push(node);
        laid.nodes.push(NodeRef::Symbol(i));
    }
    if filter.symbol.is_none() && !filter.public_only {
        laid.hidden_other = data.other.len().saturating_sub(MAX_OTHER);
        for (i, o) in data.other.iter().enumerate().take(MAX_OTHER) {
            let mut node = Node::new(o.path.clone(), t.text_faint);
            node.shape = NodeShape::Pill;
            node.badge = Some("?".to_owned());
            node.evidence = Evidence::Unknown;
            node.tip = format!("{} ({}): its symbols are unknown", o.why, o.status);
            laid.graph.nodes.push(node);
            laid.nodes.push(NodeRef::Other(i));
        }
    }
    // Sites, the most referred to first, as many as there is room for.
    let mut per_site: BTreeMap<usize, usize> = BTreeMap::new();
    for &r in &refs {
        *per_site.entry(data.refs[r].site).or_default() += 1;
    }
    let mut sites: Vec<(usize, usize)> = per_site.into_iter().collect();
    sites.sort_by_key(|&(site, n)| (std::cmp::Reverse(n), site));
    laid.hidden = sites.len().saturating_sub(MAX_SITES);
    sites.truncate(MAX_SITES);
    // Code first, then tests: each column in the order of its files.
    sites.sort_by(|a, b| {
        let (x, y) = (&data.sites[a.0], &data.sites[b.0]);
        (x.test, &x.path, x.line).cmp(&(y.test, &y.path, y.line))
    });
    let mut site_node: BTreeMap<usize, usize> = BTreeMap::new();
    for &(i, n) in &sites {
        let site = &data.sites[i];
        let color = if site.in_diff { t.text_dim } else { t.warning };
        let mut node = Node::new(site.label.clone(), color);
        node.shape = if site.in_diff {
            NodeShape::Circle
        } else {
            NodeShape::Diamond
        };
        node.column = if site.test { 2 } else { 1 };
        node.badge = site.test.then(|| "T".to_owned());
        node.evidence = Evidence::Inferred;
        node.size = (0.9 + (n as f32).ln() * 0.25).min(1.8);
        node.tip = format!(
            "{}:{}{} · {} name match{} · {}{}",
            site.path,
            site.line,
            if site.label != site.path {
                format!(" in {}", site.label)
            } else {
                String::new()
            },
            n,
            if n == 1 { "" } else { "es" },
            if site.in_diff {
                "in a changed file"
            } else {
                "in a file the change left alone (affected, unchanged)"
            },
            if site.read {
                ""
            } else {
                " · test or not told by its path only"
            },
        );
        site_node.insert(i, laid.graph.nodes.len());
        laid.graph.nodes.push(node);
        laid.nodes.push(NodeRef::Site(i));
    }
    let mut edges: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for &r in &refs {
        let reference = &data.refs[r];
        if site_node.contains_key(&reference.site) {
            edges
                .entry((reference.symbol, reference.site))
                .or_default()
                .push(r);
        }
    }
    for ((symbol, site), list) in edges {
        let (Some(&from), Some(&to)) = (site_node.get(&site), index.get(&symbol)) else {
            continue;
        };
        let mut edge = Edge::new(from, to);
        edge.evidence = Evidence::Inferred;
        edge.directed = true;
        edge.weight = 1.0 + (list.len() as f32).ln();
        edge.tip = format!(
            "{} refers to {} by name on {} line{} (click for them)",
            data.sites[site].label,
            data.symbols[symbol].name,
            list.len(),
            if list.len() == 1 { "" } else { "s" },
        );
        laid.graph.edges.push(edge);
        laid.edges.push(EdgeRef {
            symbol,
            site,
            refs: list,
        });
    }
    let mut per_column = [0usize; 3];
    for node in &laid.graph.nodes {
        per_column[node.column.min(2)] += 1;
    }
    laid.tallest = per_column.into_iter().max().unwrap_or(0);
    let how = if force {
        Arrange::Force
    } else {
        Arrange::Columns
    };
    laid.positions = graph::arrange(&laid.graph, how);
    laid
}
