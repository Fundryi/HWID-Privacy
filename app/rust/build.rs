// The same typed parser validates the source inventory before any executable is built.
#[allow(dead_code)] // Matcher data is consumed by the runtime engine, not the resource build.
#[path = "src/win/anticheat/database.rs"]
mod anticheat_database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "cargo::rerun-if-changed={}",
        anticheat_database::SOURCE_PATH
    );
    println!("cargo::rerun-if-changed=src/win/anticheat/database.rs");
    anticheat_database::Database::parse(anticheat_database::EMBEDDED)
        .map_err(|error| format!("anti-cheat detection database: {error}"))?;
    // Avoid recompiling the app for unrelated docs/scripts or private capture changes.
    for path in ["app.rc", "app.manifest", "assets/app.ico", "Cargo.toml"] {
        println!("cargo::rerun-if-changed={path}");
    }
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
