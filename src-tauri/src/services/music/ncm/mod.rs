mod bridge;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use island_core::MusicConfig;

use super::integration::{GrantResult, Integration, IntegrationStatus};
use super::provider::MusicProvider;
use super::sideload::{self, Deployer, Target};
use bridge::NcmBridgeProvider;

const EXE: &str = "cloudmusic.exe";

static DEPLOYER: Deployer = Deployer::new(Target {
    tag: "ncm-deploy",
    exe: EXE,
    proxy: "msimg32.dll",
    backup: Some("msimg32_original.dll"),
    staged: "msimg32.dll.new",
    marker: "msimg32.dll.topisland",
    payload: include_bytes!(env!("NCM_BRIDGE_DLL")),
    locate,
});

#[derive(Debug)]
pub struct Netease {
    bridge: OnceLock<NcmBridgeProvider>,
}

pub static NETEASE: Netease = Netease {
    bridge: OnceLock::new(),
};

impl Integration for Netease {
    fn id(&self) -> &'static str {
        "netease"
    }

    fn enabled(&self, music: &MusicConfig) -> bool {
        music.netease_bridge
    }

    fn provider(&'static self, on_change: fn()) -> &'static dyn MusicProvider {
        self.bridge
            .get_or_init(|| NcmBridgeProvider::start(on_change))
    }

    fn set_wanted(&self, want: bool) {
        DEPLOYER.set_wanted(want);
    }

    fn status(&self) -> IntegrationStatus {
        let connected = self
            .bridge
            .get()
            .is_some_and(NcmBridgeProvider::is_connected);
        DEPLOYER.status(connected)
    }

    fn grant(&self) -> GrantResult {
        DEPLOYER.grant_access()
    }
}

fn locate(running: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = running.and_then(Path::parent).filter(|d| is_valid(d)) {
        return Some(dir.to_path_buf());
    }
    let sources: [fn() -> Vec<PathBuf>; 4] = [
        || sideload::app_path_dirs(EXE),
        vendor_dirs,
        || {
            sideload::uninstall_dirs(|name| {
                name.contains("网易云音乐") || name.to_lowercase().contains("cloudmusic")
            })
        },
        || sideload::program_dirs(&["Netease", "CloudMusic"]),
    ];
    sources.iter().flat_map(|f| f()).find(|d| is_valid(d))
}

fn is_valid(dir: &Path) -> bool {
    dir.join(EXE).exists() && dir.join("libcef.dll").exists()
}

fn vendor_dirs() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Netease\CloudMusic",
        ),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Netease\CloudMusic"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Netease\CloudMusic"),
    ]
    .into_iter()
    .filter_map(|(hive, sub)| {
        let dir: String = RegKey::predef(hive)
            .open_subkey(sub)
            .ok()?
            .get_value("install_dir")
            .ok()?;
        Some(PathBuf::from(dir.trim().trim_matches('"')))
    })
    .collect()
}
