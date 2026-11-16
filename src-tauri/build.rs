use std::path::{Path, PathBuf};

/// 无 cargo 依赖边 故从 target 找产物
fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let target = manifest.join("..").join("target");

    embed_bridge(
        &target,
        &out_dir,
        "island-cloudmusic-bridge",
        "NCM_BRIDGE_DLL",
    );
    embed_bridge(&target, &out_dir, "island-kugou-bridge", "KUGOU_BRIDGE_DLL");

    tauri_build::build();
}

/// 未构建时嵌入空文件 对应部署不生效
fn embed_bridge(target: &Path, out_dir: &Path, package: &str, env: &str) {
    let file = format!("{}.dll", package.replace('-', "_"));
    let dest = out_dir.join(&file);
    let found = ["release", "debug"]
        .iter()
        .map(|profile| target.join(profile).join(&file))
        .find(|candidate| candidate.exists());
    match found {
        Some(candidate) => {
            std::fs::copy(&candidate, &dest).expect("copy bridge dll into OUT_DIR");
            println!("cargo:rerun-if-changed={}", candidate.display());
        }
        None => {
            std::fs::write(&dest, b"").expect("write empty bridge dll placeholder");
            println!(
                "cargo:warning={package} DLL not found under target/{{release,debug}}; \
                 its auto-deploy will be inert in this build. Run: cargo xtask bridge"
            );
        }
    }
    println!("cargo:rustc-env={env}={}", dest.display());
}
