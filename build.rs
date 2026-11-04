//! Bettet Icon und Versionsinfos (assets/app.rc) in die EXE ein. Nutzt windres aus MinGW.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("app_res.o");
    let status = Command::new("windres")
        .current_dir(manifest_dir.join("assets"))
        .args(["--input", "app.rc", "--output-format=coff", "--output"])
        .arg(&out)
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", out.display()),
        _ => println!("cargo:warning=windres nicht gefunden - EXE wird ohne eingebettetes Icon gebaut"),
    }
}
