fn main() {
    println!("cargo:rerun-if-changed=src/group_anchor.c");
    if std::env::var("CARGO_CFG_TARGET_FAMILY").as_deref() != Ok("unix") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default())
        .join("gitcraft-group-anchor");
    let mut command = cc::Build::new().get_compiler().to_command();
    let result = command
        .args([
            "-std=c99",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "src/group_anchor.c",
            "-o",
        ])
        .arg(&out)
        .status();
    match result {
        Ok(status) if status.success() => {}
        other => panic!("could not build packaged group anchor: {other:?}"),
    }
}
