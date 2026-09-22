use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::infra::watchdog::{self, Heartbeat};

#[derive(Debug)]
pub struct PushSignal {
    flag: Mutex<bool>,
    cv: Condvar,
    enabled: AtomicBool,
}

impl PushSignal {
    pub const fn new() -> Self {
        Self {
            flag: Mutex::new(false),
            cv: Condvar::new(),
            enabled: AtomicBool::new(false),
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn request(&self) {
        if !self.enabled() {
            return;
        }
        let mut flag = self.flag.lock().unwrap_or_else(|e| e.into_inner());
        *flag = true;
        self.cv.notify_one();
    }

    /// 先清位 计算期新请求再置位
    fn wait(&self) {
        let mut flag = self.flag.lock().unwrap_or_else(|e| e.into_inner());
        while !*flag {
            flag = self.cv.wait(flag).unwrap_or_else(|e| e.into_inner());
        }
        *flag = false;
    }
}

pub fn start<T>(
    app: AppHandle,
    signal: &'static PushSignal,
    event: &'static str,
    compute: impl Fn() -> T + Send + 'static,
) where
    T: serde::Serialize + Clone + Send + 'static,
{
    let hb: &'static Heartbeat = watchdog::register("music-push", Duration::from_secs(10));
    let spawned = std::thread::Builder::new()
        .name("music-push".into())
        .spawn(move || {
            let mut last_json: Option<String> = None;
            loop {
                signal.wait();
                if !signal.enabled() {
                    hb.beat();
                    continue;
                }
                hb.busy(true);
                let state = compute();
                hb.busy(false);
                hb.beat();

                let json = serde_json::to_string(&state).unwrap_or_default();
                if last_json.as_deref() == Some(json.as_str()) {
                    continue;
                }
                last_json = Some(json);
                if let Err(e) = app.emit(event, &state) {
                    eprintln!("[push] emit {event} 失败: {e}");
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("[push] 推送线程启动失败: {e}");
    }
}
