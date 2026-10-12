fn main() {
    println!("cargo:rerun-if-changed=assets/rekke.ico");
    #[cfg(target_os = "windows")]
    {
        winresource::WindowsResource::new()
            .set_icon("assets/rekke.ico")
            .set("ProductName", "Rekke")
            .set("FileDescription", "Rekke insect wing tracker")
            .set("OriginalFilename", "rekke.exe")
            .compile()
            .expect("Compile Rekke Windows icon resources");
    }
}
