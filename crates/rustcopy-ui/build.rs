fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".to_string());
    slint_build::compile_with_config("ui/app.slint", config)?;

    // The icon and the version information (FileVersion/ProductVersion come from Cargo's own version).
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icon.ico");
        resource.set("ProductName", "rustcopy");
        resource.set("FileDescription", "rustcopy console");
        resource.compile()?;
    }
    println!("cargo:rerun-if-changed=icon.ico");
    Ok(())
}
