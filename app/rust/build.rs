fn main() -> Result<(), Box<dyn std::error::Error>> {
    let major = std::env::var("CARGO_PKG_VERSION_MAJOR")?;
    let minor = std::env::var("CARGO_PKG_VERSION_MINOR")?;
    let patch = std::env::var("CARGO_PKG_VERSION_PATCH")?;
    let macros = [
        format!("HWID_VERSION_MAJOR={major}"),
        format!("HWID_VERSION_MINOR={minor}"),
        format!("HWID_VERSION_PATCH={patch}"),
        format!("HWID_VERSION_STRING=\"{major}.{minor}.{patch}\\0\""),
    ];
    // embed-resource links resources to binaries, excluding the library test harness.
    embed_resource::compile("app.rc", macros).manifest_required()?;
    Ok(())
}
