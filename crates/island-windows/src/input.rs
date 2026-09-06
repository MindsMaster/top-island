//! 悬停热区与剪贴板监听。
//!
//! 悬停用 Raw Input（`RIDEV_INPUTSINK`）：窗口穿透态下收不到 DOM 事件，只能在系统层知道
//! 鼠标动了。不用 `WH_MOUSE_LL`：低级钩子是同步回调，超时会被系统静默卸载。

use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

use windows::core::w;
use windows::Win32::Devices::HumanInterfaceDevice::{HID_USAGE_GENERIC_MOUSE, HID_USAGE_PAGE_GENERIC};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::DataExchange::AddClipboardFormatListener;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{RegisterRawInputDevices, RAWINPUTDEVICE, RIDEV_INPUTSINK};
use windows::Win32::UI::WindowsAndMessaging::*;

/// 物理像素，屏幕坐标
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    fn contains(&self, pt: POINT) -> bool {
        pt.x >= self.left && pt.x < self.right && pt.y >= self.top && pt.y < self.bottom
    }
}

#[derive(Default)]
pub struct InputHandlers {
    pub hover_rect: Option<Rect>,
    pub on_hover: Option<Box<dyn Fn(bool) + Send>>,
    pub on_clipboard: Option<Box<dyn Fn() + Send>>,
}

impl std::fmt::Debug for InputHandlers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputHandlers")
            .field("hover_rect", &self.hover_rect)
            .field("on_hover", &self.on_hover.is_some())
            .field("on_clipboard", &self.on_clipboard.is_some())
            .finish()
    }
}

struct HoverState {
    /// None 表示全程可交互（拖动等手势期间）
    rect: Option<Rect>,
    inside: bool,
    tx: Sender<bool>,
}

static HOVER: Mutex<Option<HoverState>> = Mutex::new(None);
static CLIPBOARD_TX: Mutex<Option<Sender<()>>> = Mutex::new(None);

pub fn set_hover_rect(rect: Option<Rect>) {
    let mut guard = HOVER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(st) = guard.as_mut() {
        st.rect = rect;
        evaluate(st);
    }
}

fn evaluate(st: &mut HoverState) {
    let inside = match &st.rect {
        None => true,
        Some(r) => {
            let (x, y) = cursor_position();
            r.contains(POINT { x, y })
        }
    };
    if inside != st.inside {
        st.inside = inside;
        let _ = st.tx.send(inside);
    }
}

pub fn cursor_position() -> (i32, i32) {
    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_ok() {
        (pt.x, pt.y)
    } else {
        (0, 0)
    }
}

/// 回调会切窗口穿透并 emit 到 webview，放在独立线程上执行，突发时只取最后一个状态
fn start_hover_dispatch(rx: std::sync::mpsc::Receiver<bool>, on_change: Box<dyn Fn(bool) + Send>) {
    std::thread::Builder::new()
        .name("island-hover".into())
        .spawn(move || {
            while let Ok(mut inside) = rx.recv() {
                while let Ok(later) = rx.try_recv() {
                    inside = later;
                }
                on_change(inside);
            }
        })
        .expect("spawn island-hover");
}

fn start_clip_dispatch(rx: std::sync::mpsc::Receiver<()>, on_change: Box<dyn Fn() + Send>) {
    std::thread::Builder::new()
        .name("island-clip".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {}
                on_change();
            }
        })
        .expect("spawn island-clip");
}

unsafe extern "system" fn input_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_INPUT => {
                // Raw Input 给的是相对位移，命中判定直接读 GetCursorPos
                if let Ok(mut guard) = HOVER.try_lock() {
                    if let Some(st) = guard.as_mut() {
                        evaluate(st);
                    }
                }
                // WM_INPUT 必须交给 DefWindowProc 清理
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CLIPBOARDUPDATE => {
                if let Ok(guard) = CLIPBOARD_TX.try_lock() {
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.send(());
                    }
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub fn start_input(handlers: InputHandlers) {
    let want_hover = if let (Some(rect), Some(on_hover)) = (handlers.hover_rect, handlers.on_hover) {
        let (tx, rx) = channel::<bool>();
        start_hover_dispatch(rx, on_hover);
        *HOVER.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(HoverState { rect: Some(rect), inside: false, tx });
        true
    } else {
        false
    };
    let want_clip = if let Some(cb) = handlers.on_clipboard {
        let (tx, rx) = channel::<()>();
        start_clip_dispatch(rx, cb);
        *CLIPBOARD_TX.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        true
    } else {
        false
    };
    if !want_hover && !want_clip {
        return;
    }

    std::thread::Builder::new()
        .name("island-input".into())
        .spawn(move || unsafe {
            let hmod = GetModuleHandleW(None).expect("module handle");
            let hinst = HINSTANCE::from(hmod);
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(input_wnd_proc),
                hInstance: hinst,
                lpszClassName: w!("TopIslandInput"),
                ..Default::default()
            };
            RegisterClassExW(&class);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("TopIslandInput"),
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                hinst,
                None,
            )
            .expect("message window");

            if want_hover {
                let device = RAWINPUTDEVICE {
                    usUsagePage: HID_USAGE_PAGE_GENERIC,
                    usUsage: HID_USAGE_GENERIC_MOUSE,
                    dwFlags: RIDEV_INPUTSINK,
                    hwndTarget: hwnd,
                };
                if let Err(e) = RegisterRawInputDevices(&[device], std::mem::size_of::<RAWINPUTDEVICE>() as u32) {
                    eprintln!("[input] 注册 Raw Input 鼠标失败，悬停不可用: {e}");
                }
            }
            if want_clip {
                if let Err(e) = AddClipboardFormatListener(hwnd) {
                    eprintln!("[input] 注册剪贴板监听失败: {e}");
                }
            }

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("spawn island-input");
}
