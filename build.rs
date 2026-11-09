//! Windows only: embed the app icon and version info into `gitcraft.exe`, so Explorer and
//! shortcuts show it. Elsewhere this does nothing; a missing resource compiler is only a warning.

fn main() -> std::io::Result<()> {
    catalogs()?;
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app-icon/gitcraft.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/app-icon/gitcraft.ico")
        .set("ProductName", "gitcraft")
        .set("FileDescription", "gitcraft")
        .set("LegalCopyright", "MIT OR Apache-2.0")
        .set("OriginalFilename", "gitcraft.exe")
        .set("InternalName", "gitcraft");
    if let Err(e) = res.compile() {
        println!("cargo:warning=gitcraft.exe built without its icon: {e}");
    }
    Ok(())
}

/// Discover disjoint worker-owned catalog files without editing a shared source index.
/// Generated Rust stays in OUT_DIR; only human-readable Fluent source is versioned.
fn catalogs() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=locales");
    let mut generated = String::new();
    for (locale, name) in [("en-US", "ENGLISH"), ("de", "GERMAN")] {
        let folder = std::path::Path::new("locales").join(locale);
        let mut paths: Vec<_> = std::fs::read_dir(&folder)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|p| p.extension().is_some_and(|s| s == "ftl"))
            .collect();
        paths.sort();
        generated.push_str(&format!("const {name}: &[(&str, &str)] = &[\n"));
        for path in paths {
            let absolute = std::fs::canonicalize(&path)?;
            println!("cargo:rerun-if-changed={}", path.display());
            generated.push_str(&format!(
                "({:?}, include_str!({:?})),\n",
                path.to_string_lossy(),
                absolute
            ));
        }
        generated.push_str("];\n");
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Cargo OUT_DIR is missing")
    })?);
    std::fs::write(out.join("gitcraft_catalogs.rs"), generated)
}
