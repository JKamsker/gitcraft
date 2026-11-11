//! Records.

use super::*;

#[test]
pub(super) fn month_math_and_recorded_timezone_boundaries_are_validated() -> Result<(), String> {
    let d = Day::parse("2024-02-29").ok_or("leap")?;
    assert_eq!(d.weekday(), 3);
    assert_eq!(d.shift_month(1).to_string(), "2024-03-01");
    assert_eq!(Day::parse("2026-01-01").ok_or("day")?.weekday(), 3);
    assert!(Day::parse("2025-02-29").is_none());
    assert!(Day::parse("2026-13-01").is_none());
    assert!(Day::parse("2026-01-00").is_none());
    assert_eq!(
        Day::timestamp(b"2026-01-01T23:59:59-1200")?.to_string(),
        "2026-01-01"
    );
    assert_eq!(
        Day::timestamp(b"2026-01-02T00:00:00+1400")?.to_string(),
        "2026-01-02"
    );
    Ok(())
}

#[test]
pub(super) fn nul_records_keep_non_utf8_paths_and_reject_incomplete_metadata() -> Result<(), String>
{
    let mut data = format!(
        "\0{}\0A\0a@b\02026-01-01T00:00:00Z\02026-01-01T00:00:00Z\0subject\0\n:000000 100644 {} {} A\0",
        "a".repeat(40), "0".repeat(40), "b".repeat(40)
    )
    .into_bytes();
    data.extend_from_slice(b"odd\xff\t\nname\0");
    let parsed = parse(&data)?;
    assert!(parsed[0].paths.contains(b"odd\xff\t\nname".as_slice()));
    assert!(parse(b"\0abc\0").is_err());
    Ok(())
}

#[test]
pub(super) fn configured_submodule_display_filters_never_hide_committed_gitlink_paths()
-> Result<(), String> {
    let r = Repo::new()?;
    std::fs::write(r.0.join("file"), "base").map_err(|e| e.to_string())?;
    let oid = r.commit("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "base")?;
    r.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{oid},sub"),
    ])?;
    r.git(&["commit", "-qm", "gitlink"])?;
    r.git(&["config", "diff.relative", "true"])?;
    let nested = r.0.join("nested");
    std::fs::create_dir(&nested).map_err(|e| e.to_string())?;
    for value in ["none", "all", "dirty", "untracked"] {
        r.git(&["config", "diff.ignoreSubmodules", value])?;
        r.git(&["config", "submodule.sub.ignore", value])?;
        let h = read(&nested, 100, &|| true, &AtomicUsize::new(0))?;
        assert_eq!(h.commits.len(), 2);
        assert!(
            h.commits
                .iter()
                .any(|c| c.paths.contains(b"sub".as_slice()))
        );
        assert_eq!(
            summarize(&h, Query::default(), &|| true)?.contributors[0].files,
            2
        );
    }
    Ok(())
}
