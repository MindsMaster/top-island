//! 实为模拟点击播放进度条

use std::ffi::c_void;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::OnceLock;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostThreadMessageW, SetWindowsHookExW, WH_GETMESSAGE, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_NULL,
};

use crate::hooks::{POSITION_MS, SLIDERS};
use crate::slider::TICK_MS;

const MK_LBUTTON: usize = 1;

/// 返回结构体走隐藏指针
pub type RectFn = unsafe extern "system" fn(*const c_void, *mut [i32; 4]) -> *mut [i32; 4];
/// POINT 按值传 x 在低位
pub type PointPosFn = unsafe extern "system" fn(*mut c_void, u64) -> i32;
/// Win32 消息式 `(msg, wparam, lparam, &mut lresult)`
pub type MsgFn = unsafe extern "system" fn(*mut c_void, u32, usize, isize, *mut isize) -> bool;

pub struct Api {
    pub thumb_rect: RectFn,
    pub pos_from_point: PointPosFn,
    pub mouse_down: MsgFn,
    pub mouse_up: MsgFn,
}

pub static API: OnceLock<Api> = OnceLock::new();
/// 由界面线程上的钩子回填
pub static UI_THREAD: AtomicU32 = AtomicU32::new(0);
static HOOKED: OnceLock<bool> = OnceLock::new();
static PENDING_MS: AtomicI64 = AtomicI64::new(-1);

pub fn request(position_ms: i64) {
    let tid = UI_THREAD.load(Ordering::Acquire);
    if tid == 0 {
        return;
    }
    let hooked = *HOOKED.get_or_init(|| {
        unsafe { SetWindowsHookExW(WH_GETMESSAGE, Some(on_message), None, tid) }.is_ok()
    });
    if !hooked {
        return;
    }
    PENDING_MS.store(position_ms.max(0), Ordering::Release);
    let _ = unsafe { PostThreadMessageW(tid, WM_NULL, WPARAM(0), LPARAM(0)) };
}

/// 界面方法须在界面线程调用
unsafe extern "system" fn on_message(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let ms = PENDING_MS.swap(-1, Ordering::AcqRel);
        if ms >= 0 {
            unsafe { click(ms) };
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 点击会重入进度条钩子 须先放锁
unsafe fn click(target_ms: i64) {
    let Some(api) = API.get() else {
        return;
    };
    let slider = SLIDERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .play_slider(POSITION_MS.load(Ordering::Relaxed));
    let Some((this, max)) = slider else {
        return;
    };
    let this = this as *mut c_void;
    let target = (target_ms / TICK_MS).clamp(0, i64::from(max)) as i32;
    let mut thumb = [0i32; 4];
    unsafe { (api.thumb_rect)(this, &mut thumb) };
    let y = (thumb[1] + thumb[3]) / 2;
    let x = x_for(target, |x| unsafe {
        (api.pos_from_point)(this, point(x, y))
    });
    let mut result = 0isize;
    unsafe {
        (api.mouse_down)(this, WM_LBUTTONDOWN, MK_LBUTTON, lparam(x, y), &mut result);
        (api.mouse_up)(this, WM_LBUTTONUP, 0, lparam(x, y), &mut result);
    }
    // 暂停时无进度回调 先行回写
    POSITION_MS.store(i64::from(target) * TICK_MS, Ordering::Relaxed);
}

/// 越界坐标按两端算 单调不减
fn x_for(target: i32, pos_at: impl Fn(i32) -> i32) -> i32 {
    let (mut lo, mut hi) = (-1_024, 16_384);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pos_at(mid) < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

fn point(x: i32, y: i32) -> u64 {
    u64::from(x as u32) | (u64::from(y as u32) << 32)
}

fn lparam(x: i32, y: i32) -> isize {
    (((y & 0xffff) << 16) | (x & 0xffff)) as isize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(x: i32) -> i32 {
        (x.clamp(0, 1060) as i64 * 19_600 / 1060) as i32
    }

    #[test]
    fn finds_first_x_reaching_target() {
        let x = x_for(3_500, track);
        assert!(track(x) >= 3_500);
        assert!(track(x - 1) < 3_500);
    }

    #[test]
    fn clamps_to_track_ends() {
        assert_eq!(track(x_for(0, track)), 0);
        assert_eq!(track(x_for(19_600, track)), 19_600);
    }

    #[test]
    fn packs_point_and_lparam() {
        assert_eq!(point(193, 5), (5u64 << 32) | 193);
        assert_eq!(lparam(193, 5), (5 << 16) | 193);
    }
}
