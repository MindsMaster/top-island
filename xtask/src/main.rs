use std::io::ErrorKind;
use std::net::{Ipv4Addr, Ipv6Addr, TcpListener};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

// app 的 build.rs 从 target 嵌入 bridge DLL 二者无依赖边 dev build 须先编 bridge

const NEXUS_BASE: &str = "https://repo.azuramc.cc/repository/raw-public/top-island";
/// raw-public 是聚合组只读 PUT 会 405
const NEXUS_UPLOAD_RELEASES: &str = "https://repo.azuramc.cc/repository/raw-releases/top-island";
const NEXUS_UPLOAD_SNAPSHOTS: &str = "https://repo.azuramc.cc/repository/raw-snapshots/top-island";
/// 硬编码密码只为绕开 bundler 空密码兼容问题
const KEY_PASSWORD: &str = "topisland-updater";
const DEV_PORT: u16 = 1420;
const DEV_PORT_TRIES: u16 = 20;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("bridge") => build_bridge(),
        Some("dev") => dev(),
        Some("keygen") => keygen(),
        Some("keycheck") => keycheck(),
        Some("build") => build(),
        Some("legacy-feed") => legacy_feed(),
        Some("publish") => publish(),
        _ => {
            eprintln!("usage: cargo xtask <bridge|dev|keygen|build|legacy-feed|publish>");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("xtask: {e:#}");
        std::process::exit(1);
    }
}

fn key_dir() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE")).join(".tauri")
}

fn build_bridge() -> Result<()> {
    println!("编译网易云 bridge 代理 DLL (release)…");
    let status = Command::new("cargo")
        .args(["build", "-p", "island-cloudmusic-bridge", "--release"])
        .status()
        .context("cargo build island-cloudmusic-bridge")?;
    anyhow::ensure!(status.success(), "编译 bridge DLL 失败");
    Ok(())
}

fn dev() -> Result<()> {
    build_bridge()?;
    let port = first_free_port(DEV_PORT, DEV_PORT_TRIES)
        .with_context(|| format!("{DEV_PORT} 起连续 {DEV_PORT_TRIES} 个端口都被占用"))?;
    if port != DEV_PORT {
        println!("端口 {DEV_PORT} 被占用，改用 {port}");
    }
    // devUrl 由 CLI 先读 必须在起 vite 前定好
    let config = format!(r#"{{"build":{{"devUrl":"http://localhost:{port}"}}}}"#);
    let status = Command::new("node")
        .args([
            "node_modules/@tauri-apps/cli/tauri.js",
            "dev",
            "--config",
            &config,
        ])
        .env("TOP_ISLAND_DEV_PORT", port.to_string())
        .status()
        .context("tauri dev")?;
    anyhow::ensure!(status.success(), "tauri dev 退出非零");
    Ok(())
}

fn first_free_port(start: u16, tries: u16) -> Option<u16> {
    (start..=start.saturating_add(tries - 1)).find(|&p| port_free(p))
}

/// 通配地址也要探 Windows 下别人占着 0.0.0.0 时 localhost 照样能绑上
fn port_free(port: u16) -> bool {
    let v4 = [Ipv4Addr::UNSPECIFIED, Ipv4Addr::LOCALHOST]
        .into_iter()
        .all(|ip| TcpListener::bind((ip, port)).is_ok());
    // 没开 IPv6 的机器绑不上不算占用
    let v6 = match TcpListener::bind((Ipv6Addr::LOCALHOST, port)) {
        Ok(_) => true,
        Err(e) => e.kind() != ErrorKind::AddrInUse,
    };
    v4 && v6
}

fn keygen() -> Result<()> {
    let dir = key_dir();
    std::fs::create_dir_all(&dir).context("创建 ~/.tauri")?;
    let sk = dir.join("topisland.key");
    let pk = dir.join("topisland.key.pub");
    if sk.exists() {
        anyhow::bail!("{} 已存在，覆盖请先手动删除", sk.display());
    }
    // bundler 内嵌 minisign 的空密码语义有版本差异
    let keypair = minisign::KeyPair::generate_encrypted_keypair(Some(KEY_PASSWORD.into()))
        .context("生成密钥对")?;
    let pk_text = keypair.pk.to_box()?.to_string();
    let sk_text = keypair
        .sk
        .to_box(Some("topisland updater secret key"))?
        .to_string();
    std::fs::write(&pk, &pk_text).context("写公钥文件")?;
    std::fs::write(&sk, &sk_text).context("写私钥文件")?;
    let pubkey_conf = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        pk_text.trim_end(),
    );
    println!("私钥: {}", sk.display());
    println!("tauri.conf.json 的 plugins.updater.pubkey 填：\n{pubkey_conf}");
    Ok(())
}

fn keycheck() -> Result<()> {
    let text = std::fs::read_to_string(key_dir().join("topisland.key")).context("读私钥")?;
    let keybox = minisign::SecretKeyBox::from_string(&text).context("解析 SecretKeyBox")?;
    match keybox.into_secret_key(Some(KEY_PASSWORD.into())) {
        Ok(_) => println!("密码解得开"),
        Err(e) => println!("密码解不开: {e}"),
    }
    Ok(())
}

fn conf_version() -> Result<String> {
    let text =
        std::fs::read_to_string("src-tauri/tauri.conf.json").context("读 tauri.conf.json")?;
    let conf: serde_json::Value = serde_json::from_str(&text).context("解析 tauri.conf.json")?;
    conf.get("version")
        .and_then(|v| v.as_str())
        .map(String::from)
        .context("tauri.conf.json 没有 version")
}

fn channel_of(version: &str) -> &'static str {
    if version.to_uppercase().ends_with("-SNAPSHOT") {
        "snapshot"
    } else {
        "latest"
    }
}

fn build() -> Result<()> {
    build_bridge()?;

    let key_path = key_dir().join("topisland.key");
    let key_file =
        std::fs::read_to_string(&key_path).context("读签名私钥失败（先 cargo xtask keygen）")?;
    // 该变量须为私钥文件全文的 base64
    let key = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        key_file.trim_end(),
    );

    // 直驱 tauri.js 绕开 pnpm run 前置 install
    let status = Command::new("node")
        .args(["node_modules/@tauri-apps/cli/tauri.js", "build"])
        .env("TAURI_SIGNING_PRIVATE_KEY", &key)
        .env("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", KEY_PASSWORD)
        .status()
        .context("tauri build")?;
    anyhow::ensure!(status.success(), "tauri build 失败");

    let version = conf_version()?;
    let name = format!("TopIsland_{version}_x64-setup.exe");
    let nsis_dir = PathBuf::from("target/release/bundle/nsis");
    let exe = nsis_dir.join(&name);
    let sig = nsis_dir.join(format!("{name}.sig"));
    anyhow::ensure!(exe.exists(), "缺少产物 {}", exe.display());
    anyhow::ensure!(sig.exists(), "缺少签名 {}", sig.display());

    let out = PathBuf::from("release-feed");
    std::fs::create_dir_all(&out).context("创建 release-feed/")?;
    std::fs::copy(&exe, out.join(&name)).context("拷贝安装包")?;
    std::fs::copy(&sig, out.join(format!("{name}.sig"))).context("拷贝签名")?;

    let sig_text = std::fs::read_to_string(&sig).context("读签名")?;
    let feed = serde_json::json!({
        "version": version,
        "notes": "",
        "pub_date": time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        "platforms": {
            "windows-x86_64": {
                "signature": sig_text.trim(),
                "url": format!("{NEXUS_BASE}/{name}"),
            }
        }
    });
    let channel = channel_of(&version);
    std::fs::write(
        out.join(format!("{channel}.json")),
        serde_json::to_string_pretty(&feed)?,
    )
    .context("写 feed json")?;

    println!("渠道: {channel}，产物在 release-feed/");
    println!("检查无误后手动执行 cargo xtask publish 上传 Nexus");
    Ok(())
}

/// 凭证链路与老 dist 脚本一致
fn repo_creds() -> Result<(String, String)> {
    if let (Ok(u), Ok(p)) = (
        std::env::var("AZURA_REPO_USERNAME"),
        std::env::var("AZURA_REPO_PASSWORD"),
    ) {
        return Ok((u, p));
    }
    let userprofile = std::env::var("USERPROFILE").context("缺 USERPROFILE")?;
    let mut homes = vec![
        std::env::var("GRADLE_USER_HOME").unwrap_or_default(),
        std::env::var("GRADLE_HOME").unwrap_or_default(),
        format!("{userprofile}/.gradle"),
    ];
    homes.retain(|h| !h.is_empty());
    let (mut user, mut pass) = (None, None);
    for home in homes {
        let Ok(text) = std::fs::read_to_string(format!("{home}/gradle.properties")) else {
            continue;
        };
        for line in text.lines() {
            let t = line.trim();
            if let Some(v) = t.strip_prefix("azuraRepoUsername") {
                user = user.or(Some(v.trim_start_matches('=').trim().to_string()));
            }
            if let Some(v) = t.strip_prefix("azuraRepoPassword") {
                pass = pass.or(Some(v.trim_start_matches('=').trim().to_string()));
            }
        }
    }
    match (user, pass) {
        (Some(u), Some(p)) => Ok((u, p)),
        _ => anyhow::bail!("缺凭证：设 AZURA_REPO_USERNAME/PASSWORD 或写进 gradle.properties"),
    }
}

fn publish() -> Result<()> {
    let (user, pass) = repo_creds()?;
    let dir = PathBuf::from("release-feed");
    anyhow::ensure!(dir.is_dir(), "release-feed/ 不存在，先 cargo xtask build");

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .context("读 release-feed/")?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    anyhow::ensure!(!files.is_empty(), "release-feed/ 是空的");

    let channel = channel_of(&conf_version()?);
    let token = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{user}:{pass}"),
    );
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // snapshot.* 文件一律进 snapshots 仓
        let base = if name.starts_with("snapshot.") || channel == "snapshot" {
            NEXUS_UPLOAD_SNAPSHOTS
        } else {
            NEXUS_UPLOAD_RELEASES
        };
        let url = format!("{base}/{name}");
        println!("PUT {url}");
        let body = std::fs::read(&path).with_context(|| format!("读 {}", path.display()))?;
        let resp = ureq::put(&url)
            .header("Authorization", format!("Basic {token}"))
            .content_type("application/octet-stream")
            .send(&body[..])
            .with_context(|| format!("上传 {name}"))?;
        anyhow::ensure!(
            resp.status().is_success(),
            "上传 {name} 失败: HTTP {}",
            resp.status()
        );
    }
    println!("全部上传完成");
    Ok(())
}

/// 老 electron-updater 只认 yml feed 迁移完可删
fn legacy_feed() -> Result<()> {
    use sha2::Digest;

    let version = conf_version()?;
    let name = format!("TopIsland_{version}_x64-setup.exe");
    let out = PathBuf::from("release-feed");
    let exe = out.join(&name);
    anyhow::ensure!(exe.exists(), "缺少 {}，先 cargo xtask build", exe.display());

    let bytes = std::fs::read(&exe).context("读安装包")?;
    let sha512 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        sha2::Sha512::digest(&bytes),
    );
    let size = bytes.len();
    let date = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();

    let yml = format!(
        "version: {version}\n\
         files:\n\
         \x20 - url: {name}\n\
         \x20   sha512: {sha512}\n\
         \x20   size: {size}\n\
         path: {name}\n\
         sha512: {sha512}\n\
         releaseDate: '{date}'\n"
    );
    std::fs::write(out.join("latest.yml"), &yml).context("写 latest.yml")?;
    std::fs::write(out.join("snapshot.yml"), &yml).context("写 snapshot.yml")?;

    // 0.0.1-SNAPSHOT 老客户端读的是 snapshot.json
    let latest_json = out.join("latest.json");
    anyhow::ensure!(
        latest_json.exists(),
        "缺少 latest.json，先 cargo xtask build"
    );
    std::fs::copy(&latest_json, out.join("snapshot.json")).context("镜像 snapshot.json")?;

    println!("version: {version}");
    println!("sha512: {sha512}");
    println!("size: {size}");
    println!(
        "已写 latest.yml / snapshot.yml / snapshot.json，检查后用 cargo xtask publish 一并上传"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_port_held_on_wildcard() {
        let held = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        assert!(!port_free(port));
        assert_ne!(first_free_port(port, 5), Some(port));
    }

    #[test]
    fn skips_port_held_on_loopback() {
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        assert!(!port_free(port));
    }

    #[test]
    fn keeps_start_when_free() {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(first_free_port(port, 1), Some(port));
    }
}
