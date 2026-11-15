//! Git's protected NTFS short-name/stream and HFS ignorable-character aliases.
//! Independent of host filesystem and core.protectNTFS/core.protectHFS settings.
pub(super) fn git_alias(component: &str) -> bool {
    let folded: String = component
        .chars()
        .filter(|c| {
            !matches!(
                *c as u32, 0x200c..=0x200f | 0x202a..=0x202e | 0x206a..=0x206f | 0xfeff
            )
        })
        .collect();
    let name = folded
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_end_matches(['.', ' ']);
    name.eq_ignore_ascii_case(".git") || name.eq_ignore_ascii_case("git~1")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_aliases_match_on_every_host() {
        for name in [
            ".git",
            "GIT~1",
            "git~1 .",
            ".git::$INDEX_ALLOCATION",
            "GIT~1:stream",
        ] {
            assert!(git_alias(name), "{name}");
        }
        for ignored in (0x200c..=0x200f)
            .chain(0x202a..=0x202e)
            .chain(0x206a..=0x206f)
            .chain([0xfeff])
        {
            let c = char::from_u32(ignored).expect("constant Unicode scalar");
            for name in [format!("{c}.GiT"), format!(".g{c}it"), format!(".git{c}")] {
                assert!(git_alias(&name), "{name:?}");
            }
        }
        for name in ["git", "git~2", ".github", "git~1.txt", ".gitkeep"] {
            assert!(!git_alias(name), "{name}");
        }
    }
}
