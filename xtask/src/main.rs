use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

// 用法：
//   cargo xtask keygen            生成 updater 签名密钥（~/.tauri/topisland.key），打印 conf 用 pubkey
//   cargo xtask build             tauri build（带签名私钥），产物和 feed json 收集到 release-feed/
//   cargo xtask publish           把 release-feed/ PUT 到 Nexus。发布动作，只由人手动跑，不自动化。

const NEXUS_BASE: &str = "https://repo.azuramc.cc/repository/raw-public/top-island";
// 签名私钥的密码：不追求保密（私钥本身才是秘密），只为绕开 bundler 的空密码兼容问题
const KEY_PASSWORD: &str = "topisland-updater";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("keygen") => keygen(),
        Some("keycheck") => keycheck(),
        Some("build") => build(),
        Some("publish") => publish(),
        _ => {
            eprintln!("usage: cargo xtask <keygen|build|publish>");
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

fn keygen() -> Result<()> {
    let dir = key_dir();
    std::fs::create_dir_all(&dir).context("创建 ~/.tauri")?;
    let sk = dir.join("topisland.key");
    let pk = dir.join("topisland.key.pub");
    if sk.exists() {
        anyhow::bail!("{} 已存在，覆盖请先手动删除", sk.display());
    }
    // 真实密码加密：bundler 内嵌的 minisign 对空密码的 Some("") 语义有版本差异
    // （本地 0.7.9 解得开、bundler 报 Wrong password），用真实密码绕开
    let keypair = minisign::KeyPair::generate_encrypted_keypair(Some(KEY_PASSWORD.into()))
        .context("生成密钥对")?;
    let pk_text = keypair.pk.to_box()?.to_string();
    let sk_text = keypair.sk.to_box(Some("topisland updater secret key"))?.to_string();
    std::fs::write(&pk, &pk_text).context("写公钥文件")?;
    std::fs::write(&sk, &sk_text).context("写私钥文件")?;
    let pubkey_conf =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pk_text.trim_end());
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
    let text = std::fs::read_to_string("src-tauri/tauri.conf.json").context("读 tauri.conf.json")?;
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
    let key_path = key_dir().join("topisland.key");
    let key_file = std::fs::read_to_string(&key_path)
        .context("读签名私钥失败（先 cargo xtask keygen）")?;
    // TAURI_SIGNING_PRIVATE_KEY 是 base64（文件全文，含 untrusted comment 行），
    // bundler 会先解 base64 再按 minisign SecretKeyBox 文本解析
    let key = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        key_file.trim_end(),
    );

    // 绕开 pnpm（本机 pnpm 的供应链检查会在 run 前跑 install 并失败）：
    // CLI 本体是 napi 模块，由 node 驱动 tauri.js
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
    std::fs::write(out.join(format!("{channel}.json")), serde_json::to_string_pretty(&feed)?)
        .context("写 feed json")?;

    println!("渠道: {channel}，产物在 release-feed/");
    println!("检查无误后手动执行 cargo xtask publish 上传 Nexus");
    Ok(())
}

fn publish() -> Result<()> {
    let user = std::env::var("AZURA_REPO_USERNAME").context("缺 AZURA_REPO_USERNAME")?;
    let pass = std::env::var("AZURA_REPO_PASSWORD").context("缺 AZURA_REPO_PASSWORD")?;
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

    let token = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{user}:{pass}"),
    );
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let url = format!("{NEXUS_BASE}/{name}");
        println!("PUT {url}");
        let body = std::fs::read(&path).with_context(|| format!("读 {}", path.display()))?;
        let resp = ureq::put(&url)
            .header("Authorization", format!("Basic {token}"))
            .content_type("application/octet-stream")
            .send(&body[..])
            .with_context(|| format!("上传 {name}"))?;
        anyhow::ensure!(resp.status().is_success(), "上传 {name} 失败: HTTP {}", resp.status());
    }
    println!("全部上传完成");
    Ok(())
}
