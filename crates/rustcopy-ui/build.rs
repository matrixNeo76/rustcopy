fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The interface tests find elements by their accessible label, which needs the compiler's debug
    // information. It costs size, so only the non-release builds (dev and test) carry it: the shipped
    // binary does not.
    let release = std::env::var("PROFILE").is_ok_and(|profile| profile == "release");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".to_string())
        .with_debug_info(!release);
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
    println!("cargo:rerun-if-env-changed=PROFILE");
    Ok(())
}
