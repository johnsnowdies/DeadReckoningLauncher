use winresource::WindowsResource;

fn main() {
    if cfg!(target_os = "windows") {
        let mut res = WindowsResource::new();
        
        // Добавляем иконку
        res.set_icon("assets/icon.ico");
        
        // Добавляем информацию о версии
        res.set("FileVersion", env!("CARGO_PKG_VERSION"))
           .set("ProductVersion", env!("CARGO_PKG_VERSION"))
           .set("FileDescription", "Anomaly Launcher")
           .set("ProductName", "Anomaly Launcher")
           .set("OriginalFilename", "AnomalyLauncher.exe")
           .set("LegalCopyright", "Copyright (C) 2024");
        
        // Компилируем ресурсы
        res.compile()
            .expect("Failed to run the Windows resource compiler (rc.exe)");
    }
}