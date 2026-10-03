fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Cargo's bin-specific resource linking excludes the test harness.
    embed_resource::compile_for("app.rc", ["HWIDChecker"], embed_resource::NONE)
        .manifest_required()?;
    Ok(())
}
