//! Shortening branch and tag names, keeping whichever end the user chose.

/// The longest end of `name` that fits with an ellipsis. Short names stay whole, and cuts
/// always fall between Unicode characters. `fits` measures the actual rendered text.
pub(crate) fn fit(name: &str, keep_end: bool, fits: impl Fn(&str) -> bool) -> String {
    if fits(name) {
        return name.to_owned();
    }
    let chars: Vec<char> = name.chars().collect();
    let cut = |n: usize| {
        if keep_end {
            format!("…{}", chars[chars.len() - n..].iter().collect::<String>())
        } else {
            format!("{}…", chars[..n].iter().collect::<String>())
        }
    };
    // Bisection avoids measuring every prefix or suffix on every frame.
    let (mut lo, mut hi) = (0, chars.len().saturating_sub(1));
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&cut(mid)) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    cut(lo)
}

#[cfg(test)]
mod tests {
    use super::fit;

    #[test]
    fn short_names_stay_whole_in_both_modes() {
        for keep_end in [true, false] {
            assert_eq!(fit("dev", keep_end, |s| s.chars().count() <= 3), "dev");
            assert_eq!(fit("", keep_end, |_| true), "");
        }
    }

    #[test]
    fn suffix_mode_keeps_the_part_that_distinguishes_similar_branches() {
        let fits = |s: &str| s.chars().count() <= 8;
        assert_eq!(fit("feature/team/login", true, fits), "…m/login");
        assert_eq!(fit("feature/team/logout", true, fits), "…/logout");
        assert_eq!(fit("feature/team/login", false, fits), "feature…");
    }

    #[test]
    fn measures_unicode_and_variable_width_names_in_both_directions() {
        let fits = |s: &str| {
            s.chars()
                .map(|c| if c.is_ascii() { 1 } else { 2 })
                .sum::<usize>()
                <= 9
        };
        assert_eq!(fit("機能/検索画面", true, fits), "…索画面");
        assert_eq!(fit("機能/検索画面", false, fits), "機能/検…");
        assert_eq!(fit("anything", true, |_| false), "…");
        assert_eq!(fit("anything", false, |_| false), "…");
    }
}
