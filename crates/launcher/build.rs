fn main() {
    // Embed the exe icon + version info (Windows only).
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "Rokugakun");
        res.set("FileDescription", "ロクガくん — ゲーム自動録画ランチャー");
        res.set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon embedding skipped: {e}");
        }
    }
}
