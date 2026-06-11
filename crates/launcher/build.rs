fn main() {
    // Embed the exe icon + version info (Windows only).
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "rokugakun");
        res.set("FileDescription", "rokugakun — game auto-recording launcher");
        res.set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon embedding skipped: {e}");
        }
    }
}
