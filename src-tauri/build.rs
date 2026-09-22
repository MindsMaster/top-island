use std::path::PathBuf;

/// 无 cargo 依赖边 故从 target 找产物
fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let target = manifest.join("..").join("target");
    let candidates = [
        target.join("release").join("island_cloudmusic_bridge.dll"),
        target.join("debug").join("island_cloudmusic_bridge.dll"),
    ];

    let dest = out_dir.join("ncm-bridge.dll");
    let mut embedded = false;
    for candidate in candidates {
        if candidate.exists() {
            std::fs::copy(&candidate, &dest).expect("copy bridge dll into OUT_DIR");
            println!("cargo:rerun-if-changed={}", candidate.display());
            embedded = true;
            break;
        }
    }
    if !embedded {
        std::fs::write(&dest, b"").expect("write empty bridge dll placeholder");
        println!(
            "cargo:warning=island-cloudmusic-bridge DLL not found under target/{{release,debug}}; \
             NCM bridge auto-deploy will be inert in this build. Build the bridge crate first: \
             cargo build -p island-cloudmusic-bridge --release"
        );
    }
    println!("cargo:rustc-env=NCM_BRIDGE_DLL={}", dest.display());

    tauri_build::build();
}
