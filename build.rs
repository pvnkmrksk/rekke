fn main() {
    println!("cargo:rerun-if-changed=assets/rekkefly.ico");
    #[cfg(target_os = "windows")]
    {
        winresource::WindowsResource::new()
            .set_icon("assets/rekkefly.ico")
            .set("ProductName", "Rekkefly")
            .set("FileDescription", "Rekkefly insect wing tracker")
            .set("OriginalFilename", "rekkefly.exe")
            .compile()
            .expect("Compile Rekkefly Windows icon resources");
    }
}
