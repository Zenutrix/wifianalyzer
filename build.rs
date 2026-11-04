//! Bettet Icon, Versionsinfos und Anwendungsmanifest (assets/app.rc) in die EXE ein.
//! Funktioniert mit MSVC (rc.exe) und GNU/MinGW (windres).

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
        let mut macros = vec![format!("APP_VERSION=\\\"{version}\\\"")];
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            macros.push("APP_MANIFEST".into());
        }
        embed_resource::compile("assets/app.rc", macros);
    }
}
