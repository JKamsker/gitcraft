//! The stacks the recorded layers make: trees rooted at a branch that is no layer itself
//! (`main`), each layer under its parent.

use crate::git::stack::Layer;

/// One stack: its root (the branch at the bottom, not a layer) and its layers, each with how
/// deep it is (1 for those based on the root), parents before the branches on them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tree {
    pub root: String,
    pub rows: Vec<(usize, Layer)>,
}

impl Tree {
    /// `main → schema → api → ui` for a stack that doesn't fork; for one that does, its root and
    /// how many branches it has.
    pub fn title(&self) -> String {
        let linear = self
            .rows
            .iter()
            .enumerate()
            .all(|(i, (depth, _))| *depth == i + 1);
        if linear {
            std::iter::once(self.root.as_str())
                .chain(self.rows.iter().map(|(_, l)| l.branch.as_str()))
                .collect::<Vec<_>>()
                .join(" → ")
        } else {
            format!("{} → {} branches", self.root, self.rows.len())
        }
    }

    /// How many of its layers need restacking, and how many have lost their parent.
    pub fn stale(&self) -> (usize, usize) {
        let count = |f: fn(&Layer) -> bool| self.rows.iter().filter(|(_, l)| f(l)).count();
        (count(Layer::needs_restack), count(|l| l.parent_missing))
    }
}

/// The stacks `layers` make, by root name; a layer's children by name. A loop in the
/// config (edited by hand) has no root, and its layers are left out.
pub(crate) fn trees(layers: &[Layer]) -> Vec<Tree> {
    let is_layer = |name: &str| layers.iter().any(|l| l.branch == name);
    let mut roots: Vec<&str> = layers
        .iter()
        .filter(|l| !is_layer(&l.parent))
        .map(|l| l.parent.as_str())
        .collect();
    roots.sort_unstable();
    roots.dedup();
    roots
        .into_iter()
        .map(|root| {
            let mut rows = Vec::new();
            push_children(layers, root, 1, &mut rows);
            Tree {
                root: root.to_owned(),
                rows,
            }
        })
        .collect()
}

/// The layers based on `parent`, each followed by those based on it, depth first.
fn push_children(layers: &[Layer], parent: &str, depth: usize, rows: &mut Vec<(usize, Layer)>) {
    let mut children: Vec<&Layer> = layers.iter().filter(|l| l.parent == parent).collect();
    children.sort_by(|a, b| a.branch.cmp(&b.branch));
    for child in children {
        // A branch already placed (a loop through the root) isn't placed again.
        if rows.iter().any(|(_, l)| l.branch == child.branch) || depth > layers.len() {
            continue;
        }
        rows.push((depth, child.clone()));
        push_children(layers, &child.branch, depth + 1, rows);
    }
}

/// The layers `trees` leaves out: those in a loop (edited into the config by hand), or on
/// one.
pub(crate) fn looped(layers: &[Layer]) -> Vec<String> {
    let placed: Vec<String> = trees(layers)
        .into_iter()
        .flat_map(|t| t.rows.into_iter().map(|(_, l)| l.branch))
        .collect();
    layers
        .iter()
        .filter(|l| !placed.contains(&l.branch))
        .map(|l| l.branch.clone())
        .collect()
}

/// The branches from the bottom of `branch`'s stack up to it: `[main, schema, api]` for `api`.
/// Just `[branch]` for a branch that is no layer.
pub(crate) fn chain(layers: &[Layer], branch: &str) -> Vec<String> {
    let mut out = vec![branch.to_owned()];
    let mut at = branch;
    while let Some(layer) = layers.iter().find(|l| l.branch == at) {
        if out.contains(&layer.parent) {
            break;
        }
        out.push(layer.parent.clone());
        at = &layer.parent;
    }
    out.reverse();
    out
}

/// Every layer based on `branch`, directly or through others, parents before the branches on
/// them: the order they are restacked in after `branch` changed.
pub(crate) fn dependents(layers: &[Layer], branch: &str) -> Vec<Layer> {
    let mut rows = Vec::new();
    push_children(layers, branch, 1, &mut rows);
    // Nor the branch itself, through a loop.
    rows.into_iter()
        .map(|(_, l)| l)
        .filter(|l| l.branch != branch)
        .collect()
}

/// The layers to restack so that every stack is on its parents' tips again: each that needs
/// it and every layer above one of those, parents first.
pub(crate) fn to_restack(layers: &[Layer]) -> Vec<Layer> {
    let mut out: Vec<Layer> = Vec::new();
    for tree in trees(layers) {
        for (_, layer) in tree.rows {
            let parent_moves = out.iter().any(|l| l.branch == layer.parent);
            if layer.needs_restack() || parent_moves {
                out.push(layer);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(branch: &str, parent: &str, behind: u32) -> Layer {
        Layer {
            branch: branch.to_owned(),
            parent: parent.to_owned(),
            base: None,
            own: 1,
            behind,
            parent_missing: false,
        }
    }

    fn names(rows: &[(usize, Layer)]) -> Vec<(usize, &str)> {
        rows.iter().map(|(d, l)| (*d, l.branch.as_str())).collect()
    }

    #[test]
    fn a_linear_stack_reads_bottom_to_top() {
        let layers = [
            layer("ui", "api", 1),
            layer("schema", "main", 0),
            layer("api", "schema", 0),
        ];
        let got = trees(&layers);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].root, "main");
        assert_eq!(names(&got[0].rows), [(1, "schema"), (2, "api"), (3, "ui")]);
        assert_eq!(got[0].title(), "main → schema → api → ui");
        assert_eq!(got[0].stale(), (1, 0));
        assert_eq!(chain(&layers, "api"), ["main", "schema", "api"]);
        assert_eq!(chain(&layers, "main"), ["main"]);
        let up: Vec<String> = dependents(&layers, "schema")
            .into_iter()
            .map(|l| l.branch)
            .collect();
        assert_eq!(up, ["api", "ui"]);
        assert!(dependents(&layers, "ui").is_empty());
        let all: Vec<String> = to_restack(&layers).into_iter().map(|l| l.branch).collect();
        assert_eq!(all, ["ui"]);
        // The bottom moved on: everything above it goes too, in order.
        let layers = [
            layer("ui", "api", 0),
            layer("schema", "main", 2),
            layer("api", "schema", 0),
        ];
        let all: Vec<String> = to_restack(&layers).into_iter().map(|l| l.branch).collect();
        assert_eq!(all, ["schema", "api", "ui"]);
    }

    #[test]
    fn a_forked_stack_and_two_roots() {
        let layers = [
            layer("b", "a", 0),
            layer("a", "main", 0),
            layer("c", "a", 0),
            layer("x", "dev", 0),
        ];
        let got = trees(&layers);
        assert_eq!(
            got.iter().map(|t| t.root.as_str()).collect::<Vec<_>>(),
            ["dev", "main"]
        );
        assert_eq!(names(&got[1].rows), [(1, "a"), (2, "b"), (2, "c")]);
        assert_eq!(got[1].title(), "main → 3 branches");
        assert_eq!(got[0].title(), "dev → x");
        assert!(trees(&[]).is_empty());
    }

    #[test]
    fn a_loop_in_the_config_ends() {
        let layers = [
            layer("a", "b", 0),
            layer("b", "a", 0),
            layer("c", "main", 0),
        ];
        let got = trees(&layers);
        assert_eq!(got.len(), 1);
        assert_eq!(names(&got[0].rows), [(1, "c")]);
        assert_eq!(looped(&layers), ["a", "b"]);
        let up: Vec<String> = dependents(&layers, "a")
            .into_iter()
            .map(|l| l.branch)
            .collect();
        assert_eq!(up, ["b"]);
        assert!(looped(&layers[2..]).is_empty());
        assert_eq!(chain(&layers, "a"), ["b", "a"]);
    }
}
