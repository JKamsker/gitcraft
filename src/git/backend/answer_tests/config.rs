//! Config.

use super::*;

#[test]
fn ambiguous_config_flags_and_nonportable_patterns_are_left_to_git() -> Result<(), String> {
    let repo = Repo::new("answer-config-fallback")?;
    left_to_git(
        repo.path(),
        &[
            &["config", "--local", "--local", "user.name"],
            &["config", "--null", "-z", "user.name"],
            &["config", "--bool", "--type=int", "user.name"],
            &["config", "--get", "--get-all", "user.name"],
            &["config", "--get-regexp", "!user"],
            &["config", "--get-regexp", "\\w"],
            &["config", "--get-regexp", "["],
        ],
    );
    Ok(())
}

/// Check Git-config reading, includes, key casing, and option compatibility.
#[test]
pub(super) fn config_lookups_are_gits() -> Result<(), String> {
    let repo = Repo::new("answer-config")?;
    let path = repo.path();
    std::fs::write(
        path.join("included.cfg"),
        "[gitcraft]\n\tfromInclude = yes\n[Mixed \"CaSe\"]\n\tKey = included\n",
    )
    .map_err(|e| e.to_string())?;
    let config = path.join(".git/config");
    let mut text = std::fs::read_to_string(&config).map_err(|e| e.to_string())?;
    text.push_str(concat!(
        "[gitcraft]\n\tflag\n\tName = Va\"l\"ue\\tx\n\tempty =\n\tsize = 2k\n",
        "\tfence = one\n\tfence = two\n\tsignOff = off\n",
        "[Mixed \"CaSe\"]\n\tKey = local\n",
        "[remote \"origin\"]\n\turl = https://example.com/r.git\n",
        "[branch \"main\"]\n\tremote = origin\n\tmerge = refs/heads/main\n",
        "[credential \"https://dev.example.com\"]\n\tuseHttpPath = true\n",
        "[include]\n\tpath = ../included.cfg\n",
    ));
    std::fs::write(&config, text).map_err(|e| e.to_string())?;
    answered(
        path,
        &[
            &["config", "--get", "gitcraft.name"],
            &["config", "gitcraft.name"],
            &["config", "--get", "GitCraft.NAME"],
            &["config", "--get", "gitcraft.empty"],
            &["config", "--get", "gitcraft.flag"],
            &["config", "--bool", "--get", "gitcraft.flag"],
            &["config", "--type=bool", "--get", "gitcraft.signOff"],
            &["config", "--local", "--bool", "--get", "gitcraft.signOff"],
            &["config", "--type=int", "--get", "gitcraft.size"],
            &["config", "--get", "gitcraft.missing"],
            &["config", "--get", "mixed.CaSe.key"],
            &["config", "--get", "mixed.case.key"],
            &["config", "--get-all", "mixed.CaSe.key"],
            &["config", "--local", "--get-all", "mixed.CaSe.key"],
            &["config", "--local", "--get-all", "gitcraft.fence"],
            &["config", "--get", "gitcraft.fence"],
            &["config", "--get", "gitcraft.fromInclude"],
            &["config", "--local", "--get", "gitcraft.fromInclude"],
            &["config", "--get-regexp", "^gitcraft\\."],
            &["config", "-z", "--get-regexp", "^gitcraft\\."],
            &[
                "config",
                "--null",
                "--get-regexp",
                "^(branch\\..*\\.(remote|merge|pushremote)|remote\\..*\\.gitcraftfork)$",
            ],
            &[
                "config",
                "--local",
                "-z",
                "--get-regexp",
                "^branch\\..*\\.gitcraft-issue",
            ],
            &["config", "--get-regexp", "MIXED"],
            &["config", "--get-regexp", "^credential\\..*\\.usehttppath$"],
            &[
                "config",
                "--type=bool",
                "--get-regexp",
                "^remote\\..*\\.promisor$",
            ],
            &[
                "config",
                "--get",
                "credential.https://dev.example.com.useHttpPath",
            ],
            &["config", "--get", "user.name"],
            &[
                "config",
                "-z",
                "--local",
                "--get-regexp",
                "^user\\.(name|email)$",
            ],
        ],
    );
    left_to_git(
        path,
        &[
            &["config", "gitcraft.name", "value"],
            &["config", "--global", "--get", "user.name"],
            &["config", "--get-regexp", "!x"],
            &["config", "--get-regexp", "\\d"],
            &["config", "--get", "gitcraft.name", "value-pattern"],
            &["config", "--list"],
            &["config", "--type=int", "--get", "gitcraft.flag"],
        ],
    );
    Ok(())
}
