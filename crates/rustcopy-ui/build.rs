fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".to_string());
    slint_build::compile_with_config("ui/app.slint", config)?;
    Ok(())
}
