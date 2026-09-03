use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::DataExchange::AddClipboardFormatListener;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

// WebView2 没有 Electron 的 setIgnoreMouseEvents(forward: true) 等价物，
// 悬停展开靠 WH_MOUSE_LL 跟踪光标位置，进出岛窗矩形时切换穿透。
struct HookState {
    app: AppHandle,
    rect: RECT,
    // 胶囊高度与整窗高度（物理像素）：面板收起时交互区只有胶囊这一条
    cap_h: i32,
    full_h: i32,
    inside: bool,
}

static HOOK: Mutex<Option<HookState>> = Mutex::new(None);
static CLIP_APP: Mutex<Option<AppHandle>> = Mutex::new(None);
static CLIP_COUNT: AtomicU64 = AtomicU64::new(0);

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEMOVE {
        let pt = (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt;
        let mut guard = HOOK.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(st) = guard.as_mut() {
            let inside = pt.x >= st.rect.left
                && pt.x < st.rect.right
                && pt.y >= st.rect.top
                && pt.y < st.rect.bottom;
            if inside != st.inside {
                st.inside = inside;
                if let Some(win) = st.app.get_webview_window("island") {
                    let _ = win.set_ignore_cursor_events(!inside);
                    let _ = st.app.emit("island-hover", inside);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

// 面板展开/收起时调整交互区高度；收起后光标若在胶囊外立刻恢复穿透
pub fn set_panel_open(open: bool) {
    let mut guard = HOOK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(st) = guard.as_mut() {
        st.rect.bottom = st.rect.top + if open { st.full_h } else { st.cap_h };
        if !open && st.inside {
            if let Ok(pt) = cursor_pos() {
                let inside = pt.x >= st.rect.left
                    && pt.x < st.rect.right
                    && pt.y >= st.rect.top
                    && pt.y < st.rect.bottom;
                if !inside {
                    st.inside = false;
                    if let Some(win) = st.app.get_webview_window("island") {
                        let _ = win.set_ignore_cursor_events(true);
                        let _ = st.app.emit("island-hover", false);
                    }
                }
            }
        }
    }
}

fn cursor_pos() -> windows::core::Result<windows::Win32::Foundation::POINT> {
    let mut pt = windows::Win32::Foundation::POINT::default();
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt)? };
    Ok(pt)
}

unsafe extern "system" fn clip_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {    if msg == WM_CLIPBOARDUPDATE {
        let n = CLIP_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        let guard = CLIP_APP.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(app) = guard.as_ref() {
            let _ = app.emit("clip-changed", n);
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub fn start(app: AppHandle, rect: RECT, cap_h: i32, full_h: i32) {
    *HOOK.lock().unwrap_or_else(|e| e.into_inner()) = Some(HookState {
        app: app.clone(),
        rect: RECT { bottom: rect.top + cap_h, ..rect },
        cap_h,
        full_h,
        inside: false,
    });
    *CLIP_APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);

    std::thread::Builder::new()
        .name("island-input".into())
        .spawn(move || unsafe {
            let hmod = GetModuleHandleW(None).expect("module handle");
            let hinst = HINSTANCE::from(hmod);
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0).expect("mouse hook");

            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(clip_wnd_proc),
                hInstance: hinst,
                lpszClassName: w!("IslandSpikeClip"),
                ..Default::default()
            };
            RegisterClassExW(&class);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("IslandSpikeClip"),
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
            AddClipboardFormatListener(hwnd).expect("clipboard listener");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("spawn island-input");
}
