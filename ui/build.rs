fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/main.slint", config).expect("slint build failed");
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.starts_with("armv7") {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sysroot/usr/lib/arm-linux-gnueabihf");
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-arg=-Wl,--allow-shlib-undefined");
    }
}
