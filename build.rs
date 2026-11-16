//! Bettet Icon, Versionsinfos und Anwendungsmanifest (assets/app.rc) in die EXE ein.
//! Funktioniert mit MSVC (rc.exe) und GNU/MinGW (windres).
//!
//! Die Versionsnummer wird nicht als Makro übergeben (Anführungszeichen in `/D` werden von
//! verschiedenen rc.exe-Versionen unterschiedlich behandelt), sondern direkt in eine Kopie der
//! Ressourcendatei im Build-Ordner geschrieben – ebenso die absoluten Pfade zu Icon und Manifest.

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let nums: Vec<u32> = version.split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect();
    let numeric = format!("{},{},{},0", nums.first().unwrap_or(&0), nums.get(1).unwrap_or(&0), nums.get(2).unwrap_or(&0));
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()).replace('\\', "/");
    let src = std::fs::read_to_string("assets/app.rc").expect("assets/app.rc fehlt");
    let rc = src
        .replace("APP_VERSION", &format!("\"{version}\""))
        .replace("APP_FILEVERSION", &numeric)
        .replace("\"assets/", &format!("\"{root}/assets/"));
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR fehlt")).join("app.rc");
    std::fs::write(&out, rc).expect("app.rc konnte nicht geschrieben werden");
    let mut macros: Vec<String> = Vec::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        macros.push("APP_MANIFEST".into());
    }
    embed_resource::compile(&out, macros);
}
