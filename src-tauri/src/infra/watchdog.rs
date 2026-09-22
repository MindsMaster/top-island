use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use tauri::AppHandle;

#[derive(Debug)]
pub struct Heartbeat {
    pub name: &'static str,
    /// busy 超此才算卡住 闲着不跳正常
    pub expected_period: Duration,
    last_beat_ms: AtomicI64,
    busy: AtomicBool,
    busy_since_ms: AtomicI64,
}

impl Heartbeat {
    pub fn beat(&self) {
        self.last_beat_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub fn busy(&self, on: bool) {
        self.busy.store(on, Ordering::Relaxed);
        if on {
            self.busy_since_ms.store(now_ms(), Ordering::Relaxed);
        }
    }

    fn stuck(&self, now: i64) -> Option<i64> {
        if !self.busy.load(Ordering::Relaxed) {
            return None;
        }
        let since = self.busy_since_ms.load(Ordering::Relaxed);
        let dur = now - since;
        (dur > self.expected_period.as_millis() as i64).then_some(dur)
    }
}

static REGISTRY: Mutex<Vec<&'static Heartbeat>> = Mutex::new(Vec::new());

pub fn register(name: &'static str, expected_period: Duration) -> &'static Heartbeat {
    let hb: &'static Heartbeat = Box::leak(Box::new(Heartbeat {
        name,
        expected_period,
        last_beat_ms: AtomicI64::new(now_ms()),
        busy: AtomicBool::new(false),
        busy_since_ms: AtomicI64::new(0),
    }));
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).push(hb);
    hb
}

static MAIN_PONG_MS: AtomicI64 = AtomicI64::new(0);
static MAIN_LATENCY_MS: AtomicI64 = AtomicI64::new(0);
static MAIN_PING_SENT_MS: AtomicI64 = AtomicI64::new(0);

const PROBE_EVERY: Duration = Duration::from_secs(1);
const MAIN_HUNG_AFTER_MS: i64 = 5_000;
const DUMP_COOLDOWN: Duration = Duration::from_secs(120);

pub fn start(app: AppHandle) {
    static START: Once = Once::new();
    START.call_once(|| {
        MAIN_PONG_MS.store(now_ms(), Ordering::Relaxed);
        if let Err(e) = std::thread::Builder::new()
            .name("watchdog".into())
            .spawn(move || run(app))
        {
            eprintln!("[watchdog] 启动失败: {e}");
        }
    });
}

fn run(app: AppHandle) {
    let mut last_dump: Option<Instant> = None;
    let mut hung_reported = false;
    loop {
        std::thread::sleep(PROBE_EVERY);

        // 探针回来才再投 防队列堆积
        let sent = MAIN_PING_SENT_MS.load(Ordering::Relaxed);
        let pong = MAIN_PONG_MS.load(Ordering::Relaxed);
        if pong >= sent {
            let t = now_ms();
            MAIN_PING_SENT_MS.store(t, Ordering::Relaxed);
            let _ = app.run_on_main_thread(move || {
                let now = now_ms();
                MAIN_PONG_MS.store(now, Ordering::Relaxed);
                MAIN_LATENCY_MS.store(now - t, Ordering::Relaxed);
            });
        }

        let now = now_ms();
        let main_silent = now - MAIN_PONG_MS.load(Ordering::Relaxed);
        let main_hung = main_silent > MAIN_HUNG_AFTER_MS;

        let stuck: Vec<(&'static str, i64)> = REGISTRY
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter_map(|hb| hb.stuck(now).map(|d| (hb.name, d)))
            .collect();
        for (name, d) in &stuck {
            eprintln!("[watchdog] 线程 {name} 已忙 {d}ms 未回来");
        }

        if main_hung {
            if !hung_reported {
                eprintln!("[watchdog] 主线程 {main_silent}ms 未响应探针，判定卡死");
                hung_reported = true;
            }
            let cooled = last_dump.map_or(true, |t| t.elapsed() > DUMP_COOLDOWN);
            if cooled {
                last_dump = Some(Instant::now());
                capture_scene(main_silent, &stuck);
            }
        } else if hung_reported {
            eprintln!(
                "[watchdog] 主线程已恢复（延迟 {}ms）",
                MAIN_LATENCY_MS.load(Ordering::Relaxed)
            );
            hung_reported = false;
        }
    }
}

fn capture_scene(main_silent_ms: i64, stuck: &[(&'static str, i64)]) {
    let Ok(dir) = crate::infra::paths::data_dir() else {
        return;
    };
    let dir = dir.join("watchdog");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let stamp = chrono_stamp();

    let mut report = String::new();
    report.push_str(&format!("top-island watchdog report {stamp}\n"));
    report.push_str(&format!(
        "main thread silent for {main_silent_ms}ms (last latency {}ms)\n",
        MAIN_LATENCY_MS.load(Ordering::Relaxed)
    ));
    report.push_str("\nthreads:\n");
    let now = now_ms();
    for hb in REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        let idle = now - hb.last_beat_ms.load(Ordering::Relaxed);
        let busy = hb.busy.load(Ordering::Relaxed);
        report.push_str(&format!(
            "  {:<20} busy={:<5} last_beat={}ms ago\n",
            hb.name, busy, idle
        ));
    }
    if !stuck.is_empty() {
        report.push_str("\nstuck:\n");
        for (n, d) in stuck {
            report.push_str(&format!("  {n} for {d}ms\n"));
        }
    }
    let _ = std::fs::write(dir.join(format!("hang-{stamp}.txt")), &report);

    match write_minidump(&dir.join(format!("hang-{stamp}.dmp"))) {
        Ok(()) => eprintln!("[watchdog] 已写 minidump 到 {}", dir.display()),
        Err(e) => eprintln!("[watchdog] 写 minidump 失败: {e}"),
    }
}

/// 含线程栈与线程名
fn write_minidump(path: &std::path::Path) -> std::io::Result<()> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Diagnostics::Debug::{
        MiniDumpWithIndirectlyReferencedMemory, MiniDumpWithThreadInfo, MiniDumpWriteDump,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId};

    let file = std::fs::File::create(path)?;
    let handle = HANDLE(file.as_raw_handle());
    let kind = MiniDumpWithThreadInfo | MiniDumpWithIndirectlyReferencedMemory;
    unsafe {
        MiniDumpWriteDump(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            handle,
            kind,
            None,
            None,
            None,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn chrono_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
