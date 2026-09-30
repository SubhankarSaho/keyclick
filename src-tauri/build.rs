// The Common-Controls v6 manifest is linked into *every* target (not just the
// app binary, as tauri-build does) so `cargo test` executables can start too.
fn main() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let mut attrs = tauri_build::Attributes::new();
    if windows {
        attrs = attrs.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        embed_manifest();
    }
    tauri_build::try_build(attrs).expect("tauri-build failed");
}

fn embed_manifest() {
    use std::path::PathBuf;
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        return;
    }
    // GNU: compile a manifest-only resource with windres (inside OUT_DIR to avoid spaces in paths).
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::copy(&manifest, out.join("app.manifest")).unwrap();
    std::fs::write(out.join("manifest.rc"), "1 24 \"app.manifest\"\n").unwrap();
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| "windres".into());
    let status = std::process::Command::new(windres)
        .current_dir(&out)
        .args(["-i", "manifest.rc", "-o", "manifest.o", "-O", "coff"])
        .status()
        .expect("windres not found (install MinGW-w64 or set WINDRES)");
    assert!(status.success(), "windres failed");
    println!("cargo:rustc-link-arg={}", out.join("manifest.o").display());
}
