//! 挂 ui.dll 按名导出的界面方法 版本更新后名字不变即可用

use std::ffi::c_void;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use retour::GenericDetour;
use windows::core::{s, w, PCSTR};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

use crate::lyric::{self, Api, GetIntFn, GetRefFn, Lyric};
use crate::slider::Sliders;

type IntArgFn = unsafe extern "system" fn(*mut c_void, i32);
type TwoIntFn = unsafe extern "system" fn(*mut c_void, i32, i32);
/// shared_ptr 按值传参 实为指向临时对象的指针 由被调方析构
type SetLyricFn = unsafe extern "system" fn(*mut c_void, *mut [usize; 2]);

/// 歌词控件取进度的入口 约 80Hz 暂停即停
static PROGRESS: OnceLock<GenericDetour<IntArgFn>> = OnceLock::new();
static SET_RANGE: OnceLock<GenericDetour<TwoIntFn>> = OnceLock::new();
static SET_POS: OnceLock<GenericDetour<IntArgFn>> = OnceLock::new();
static SET_LYRIC: OnceLock<GenericDetour<SetLyricFn>> = OnceLock::new();
static LYRIC_API: OnceLock<Api> = OnceLock::new();

pub static POSITION_MS: AtomicI64 = AtomicI64::new(0);
/// 0 为尚无进度
pub static TICK_EPOCH_MS: AtomicI64 = AtomicI64::new(0);
pub static SLIDERS: Mutex<Sliders> = Mutex::new(Sliders::new());
/// 代号随内容变 多个歌词控件会各收一份
pub static LYRIC: Mutex<Option<(u64, Lyric)>> = Mutex::new(None);

const UI_DLL_WAIT: Duration = Duration::from_secs(120);

pub fn epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

unsafe extern "system" fn hook_progress(this: *mut c_void, position_ms: i32) {
    if let Some(d) = PROGRESS.get() {
        unsafe { d.call(this, position_ms) };
    }
    POSITION_MS.store(i64::from(position_ms), Ordering::Relaxed);
    TICK_EPOCH_MS.store(epoch_ms(), Ordering::Release);
}

unsafe extern "system" fn hook_set_range(this: *mut c_void, min: i32, max: i32) {
    if let Some(d) = SET_RANGE.get() {
        unsafe { d.call(this, min, max) };
    }
    SLIDERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set_range(this as usize, max);
}

unsafe extern "system" fn hook_set_pos(this: *mut c_void, pos: i32) {
    if let Some(d) = SET_POS.get() {
        unsafe { d.call(this, pos) };
    }
    SLIDERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set_pos(this as usize, pos);
}

/// 在界面线程 对象由参数持有 须在转调前读
unsafe extern "system" fn hook_set_lyric(this: *mut c_void, sp: *mut [usize; 2]) {
    let lt = if sp.is_null() { 0 } else { unsafe { (*sp)[0] } };
    if lt != 0 {
        if let Some(api) = LYRIC_API.get() {
            if let Some(l) = unsafe { lyric::read(api, lt as *const c_void) } {
                let mut cur = LYRIC.lock().unwrap_or_else(|e| e.into_inner());
                if !l.lines.is_empty() && cur.as_ref().is_none_or(|(_, c)| *c != l) {
                    let generation = cur.as_ref().map_or(1, |(g, _)| g + 1);
                    *cur = Some((generation, l));
                }
            }
        }
    }
    if let Some(d) = SET_LYRIC.get() {
        unsafe { d.call(this, sp) };
    }
}

fn lyric_api(ui: HMODULE) -> Option<Api> {
    let f = |name: PCSTR| unsafe { GetProcAddress(ui, name) }.map(|p| p as usize);
    unsafe {
        Some(Api {
            lines: std::mem::transmute::<usize, GetRefFn>(f(s!("?GetLinesText@LyricText@kugou_ui@@QEAAAEAV?$vector@ULyricTextLine@kugou_ui@@V?$allocator@ULyricTextLine@kugou_ui@@@std@@@std@@XZ"))?),
            title: std::mem::transmute::<usize, GetRefFn>(f(s!("?GetTitle@LyricText@kugou_ui@@QEAAAEBV?$basic_string@_WU?$char_traits@_W@std@@V?$allocator@_W@2@@std@@XZ"))?),
            artist: std::mem::transmute::<usize, GetRefFn>(f(s!("?GetArtist@LyricText@kugou_ui@@QEAAAEBV?$basic_string@_WU?$char_traits@_W@std@@V?$allocator@_W@2@@std@@XZ"))?),
            length: std::mem::transmute::<usize, GetIntFn>(f(s!("?GetLength@LyricText@kugou_ui@@QEAAHXZ"))?),
        })
    }
}

/// 进度钩子装上才算成功
pub fn install() -> bool {
    let Some(ui) = wait_ui_dll() else {
        return false;
    };
    unsafe {
        let progress = detour(
            &PROGRESS,
            ui,
            s!("?OnPlaybackProgressUpdated@LyricViewForPlaying@kugou_ui@@UEAAXH@Z"),
            hook_progress as IntArgFn,
        );
        let _ = detour(
            &SET_RANGE,
            ui,
            s!("?SetRange@TrackBar@kugou_ui@@QEAAXHH@Z"),
            hook_set_range as TwoIntFn,
        );
        let _ = detour(
            &SET_POS,
            ui,
            s!("?SetTrackBarPosNoNotify@TrackBar@kugou_ui@@QEAAXH@Z"),
            hook_set_pos as IntArgFn,
        );
        if let Some(api) = lyric_api(ui) {
            let _ = LYRIC_API.set(api);
            let _ = detour(
                &SET_LYRIC,
                ui,
                s!("?SetLyricText@LyricViewForPlaying@kugou_ui@@UEAAXV?$shared_ptr@VLyricText@kugou_ui@@@std@@@Z"),
                hook_set_lyric as SetLyricFn,
            );
        }
        progress
    }
}

/// kugou.dll 由主程序动态加载 ui.dll 随后才在
fn wait_ui_dll() -> Option<HMODULE> {
    let started = Instant::now();
    while started.elapsed() < UI_DLL_WAIT {
        if let Ok(m) = unsafe { GetModuleHandleW(w!("ui.dll")) } {
            if !m.is_invalid() {
                return Some(m);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// enable 后钩子随时被调 须先存好 trampoline
unsafe fn detour<F: retour::Function>(
    slot: &'static OnceLock<GenericDetour<F>>,
    module: HMODULE,
    name: PCSTR,
    hook: F,
) -> bool {
    let Some(addr) = (unsafe { GetProcAddress(module, name) }) else {
        return false;
    };
    let target: F = unsafe { std::mem::transmute_copy(&addr) };
    let Ok(d) = (unsafe { GenericDetour::new(target, hook) }) else {
        return false;
    };
    if slot.set(d).is_err() {
        return false;
    }
    slot.get().is_some_and(|d| unsafe { d.enable() }.is_ok())
}
