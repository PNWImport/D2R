fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set("CompanyName", "Google LLC");
        res.set("FileDescription", "Chrome Helper");
        res.set("ProductName", "Google Chrome");
        res.set("InternalName", "chrome_helper");
        res.set("OriginalFilename", "chrome_helper.exe");
        res.set("FileVersion", "122.0.6261.95");
        res.set("ProductVersion", "122.0.6261.95");
        res.set(
            "LegalCopyright",
            "Copyright 2024 Google LLC. All rights reserved.",
        );
        if let Err(e) = res.compile() {
            eprintln!("winres compile warning: {}", e);
            // Don't fail build — PE metadata is nice-to-have
        }
    }
}
