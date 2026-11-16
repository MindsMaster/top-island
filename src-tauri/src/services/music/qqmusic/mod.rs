mod pipe;
mod qrc;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use island_core::MusicConfig;

use super::integration::{GrantResult, Integration, IntegrationStatus};
use super::provider::MusicProvider;
use super::sideload;
use pipe::QqPipeProvider;

const EXE: &str = "QQMusic.exe";
/// 旧版无本机接口 会一直连不上
const CONNECT_GRACE: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub struct QqMusic {
    pipe: OnceLock<QqPipeProvider>,
    wanted: AtomicBool,
    waiting_since: Mutex<Option<Instant>>,
}

pub static QQMUSIC: QqMusic = QqMusic {
    pipe: OnceLock::new(),
    wanted: AtomicBool::new(false),
    waiting_since: Mutex::new(None),
};

impl Integration for QqMusic {
    fn id(&self) -> &'static str {
        "qqmusic"
    }

    fn enabled(&self, music: &MusicConfig) -> bool {
        music.qqmusic_bridge
    }

    fn provider(&'static self, on_change: fn()) -> &'static dyn MusicProvider {
        self.pipe
            .get_or_init(|| QqPipeProvider::start(&self.wanted, on_change))
    }

    fn set_wanted(&self, want: bool) {
        self.wanted.store(want, Ordering::Release);
    }

    fn status(&self) -> IntegrationStatus {
        let mut since = self.waiting_since.lock().unwrap_or_else(|e| e.into_inner());
        if self.pipe.get().is_some_and(QqPipeProvider::is_connected) {
            *since = None;
            return IntegrationStatus::Connected;
        }
        if !sideload::is_running(EXE) {
            *since = None;
            return IntegrationStatus::NotDetected;
        }
        if since.get_or_insert_with(Instant::now).elapsed() < CONNECT_GRACE {
            IntegrationStatus::Connecting
        } else {
            IntegrationStatus::Unsupported
        }
    }

    /// 不写文件 无需授权
    fn grant(&self) -> GrantResult {
        GrantResult::Granted
    }
}
