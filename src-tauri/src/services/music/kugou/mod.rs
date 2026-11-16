mod bridge;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use island_core::MusicConfig;

use super::integration::{GrantResult, Integration, IntegrationStatus};
use super::provider::MusicProvider;
use super::sideload::{self, Deployer, Target};
use super::smtc;
use bridge::KugouBridgeProvider;

const EXE: &str = "KuGou.exe";

/// 根目录不随版本号变 升级免重部署
static DEPLOYER: Deployer = Deployer::new(Target {
    tag: "kugou-deploy",
    exe: EXE,
    proxy: "version.dll",
    backup: None,
    staged: "version.dll.new",
    marker: "version.dll.topisland",
    payload: include_bytes!(env!("KUGOU_BRIDGE_DLL")),
    locate,
});

#[derive(Debug)]
pub struct Kugou {
    bridge: OnceLock<KugouBridgeProvider>,
}

pub static KUGOU: Kugou = Kugou {
    bridge: OnceLock::new(),
};

impl Integration for Kugou {
    fn id(&self) -> &'static str {
        "kugou"
    }

    fn enabled(&self, music: &MusicConfig) -> bool {
        music.kugou_bridge
    }

    fn provider(&'static self, on_change: fn()) -> &'static dyn MusicProvider {
        self.bridge
            .get_or_init(|| KugouBridgeProvider::start(on_change, &smtc::SMTC))
    }

    fn set_wanted(&self, want: bool) {
        DEPLOYER.set_wanted(want);
    }

    fn status(&self) -> IntegrationStatus {
        let connected = self
            .bridge
            .get()
            .is_some_and(KugouBridgeProvider::is_connected);
        DEPLOYER.status(connected)
    }

    fn grant(&self) -> GrantResult {
        DEPLOYER.grant_access()
    }
}

/// 版本子目录也有同名 exe 候选连同上级一起试
fn locate(running: Option<&Path>) -> Option<PathBuf> {
    let from_running = running.and_then(Path::parent).map(Path::to_path_buf);
    let sources: [fn() -> Vec<PathBuf>; 3] = [
        || sideload::app_path_dirs(EXE),
        || {
            sideload::uninstall_dirs(|name| {
                name.contains("酷狗音乐") || name.to_lowercase().contains("kugou")
            })
        },
        || sideload::program_dirs(&["KuGou", "KGMusic"]),
    ];
    from_running
        .into_iter()
        .chain(sources.iter().flat_map(|f| f()))
        .flat_map(|dir| {
            let parent = dir.parent().map(Path::to_path_buf);
            std::iter::once(dir).chain(parent)
        })
        .find(|d| is_root(d))
}

/// 根目录只有启动器 kugou.dll 在版本子目录
fn is_root(dir: &Path) -> bool {
    dir.join(EXE).is_file()
        && !dir.join("kugou.dll").exists()
        && std::fs::read_dir(dir).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|e| e.path().join("kugou.dll").is_file())
        })
}
