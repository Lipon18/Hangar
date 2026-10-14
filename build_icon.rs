pub fn compile_icon() {
    // Only compile the resource script on Windows targets
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/hangar_x32.ico");
        if let Err(e) = res.compile() {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}