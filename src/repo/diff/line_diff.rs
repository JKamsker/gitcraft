//! A line diff of two texts gitcraft made itself (text taken out of a document, the cells of a
//! notebook), which git never saw: Myers' algorithm, shortest edit script first.

/// What a line of the diff is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Same,
    Del,
    Add,
}

/// How many trace entries the search may keep (each edit keeps a row as long as both texts):
/// past that the texts are too unlike to diff line by line, and everything before is removed
/// and everything after added.
const TRACE_BUDGET: usize = 20_000_000;

/// The lines of `old` and `new` as a diff: each with what it is, and its index in its text.
pub fn diff_lines<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<(Edit, &'a str)> {
    // The lines both start and end with stay out of the search.
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a, b) = (&old[head..old.len() - tail], &new[head..new.len() - tail]);
    let mut out: Vec<(Edit, &str)> = old[..head].iter().map(|l| (Edit::Same, *l)).collect();
    match myers(a, b) {
        Some(script) => out.extend(script),
        None => {
            out.extend(a.iter().map(|l| (Edit::Del, *l)));
            out.extend(b.iter().map(|l| (Edit::Add, *l)));
        }
    }
    out.extend(old[old.len() - tail..].iter().map(|l| (Edit::Same, *l)));
    out
}

/// Myers' shortest edit script, or `None` past [`TRACE_BUDGET`].
fn myers<'a>(a: &[&'a str], b: &[&'a str]) -> Option<Vec<(Edit, &'a str)>> {
    let (n, m) = (a.len(), b.len());
    let max = n + m;
    if max == 0 {
        return Some(Vec::new());
    }
    let offset = max as isize;
    let mut v = vec![0usize; 2 * max + 2];
    let mut trace: Vec<Vec<usize>> = Vec::new();
    let limit = max.min(TRACE_BUDGET / (2 * max + 2));
    let mut found = None;
    'outer: for d in 0..=limit {
        trace.push(v.clone());
        let d = d as isize;
        let mut k = -d;
        while k <= d {
            let at = |k: isize| (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = (x as isize - k) as usize;
            while x < n && y < m && a[x] == b[y] {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x >= n && y >= m {
                found = Some(d as usize);
                break 'outer;
            }
            k += 2;
        }
    }
    found?;
    // Back through the trace, from the end to the start.
    let mut script = Vec::with_capacity(max);
    let (mut x, mut y) = (n as isize, m as isize);
    for (d, v) in trace.iter().enumerate().rev() {
        let d = d as isize;
        let k = x - y;
        let at = |k: isize| (k + offset) as usize;
        let prev_k = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = v[at(prev_k)] as isize;
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            script.push((Edit::Same, a[x as usize]));
        }
        if d > 0 {
            if x == prev_x {
                y -= 1;
                script.push((Edit::Add, b[y as usize]));
            } else {
                x -= 1;
                script.push((Edit::Del, a[x as usize]));
            }
        }
    }
    script.reverse();
    Some(script)
}

#[cfg(test)]
mod tests {
    use super::{Edit, diff_lines};

    fn show(old: &str, new: &str) -> String {
        let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
        diff_lines(&a, &b)
            .iter()
            .map(|(e, l)| {
                let mark = match e {
                    Edit::Same => ' ',
                    Edit::Del => '-',
                    Edit::Add => '+',
                };
                format!("{mark}{l}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn small_diffs() {
        assert_eq!(show("a\nb\nc", "a\nx\nc"), " a\n-b\n+x\n c");
        assert_eq!(show("", "a"), "+a");
        assert_eq!(show("a", ""), "-a");
        assert_eq!(show("", ""), "");
        assert_eq!(show("a\nb", "a\nb"), " a\n b");
        assert_eq!(show("ä\nb\nc\nd", "b\nc\nd\nü"), "-ä\n b\n c\n d\n+ü");
    }

    #[test]
    fn every_line_is_kept_once() {
        let old: Vec<String> = (0..300).map(|i| format!("l{}", i % 7)).collect();
        let new: Vec<String> = (0..280).map(|i| format!("l{}", (i * 3) % 11)).collect();
        let (a, b): (Vec<&str>, Vec<&str>) = (
            old.iter().map(String::as_str).collect(),
            new.iter().map(String::as_str).collect(),
        );
        let d = diff_lines(&a, &b);
        let back_old: Vec<&str> = d
            .iter()
            .filter(|(e, _)| *e != Edit::Add)
            .map(|(_, l)| *l)
            .collect();
        let back_new: Vec<&str> = d
            .iter()
            .filter(|(e, _)| *e != Edit::Del)
            .map(|(_, l)| *l)
            .collect();
        assert_eq!(back_old, a);
        assert_eq!(back_new, b);
    }
}
