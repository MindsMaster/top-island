use std::sync::Mutex;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::DataExchange::AddClipboardFormatListener;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

/// 物理像素矩形（屏幕坐标）
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

/// 输入监听挂接：悬停热区（光标进出时回调）与剪贴板变化。
/// WebView2 没有 Electron 的 setIgnoreMouseEvents(forward:) 等价物，
/// 悬停展开靠 WH_MOUSE_LL 跟踪光标位置，进出热区时通知调用方切换穿透。
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
    rect: Rect,
    inside: bool,
    on_change: Box<dyn Fn(bool) + Send>,
}

static HOVER: Mutex<Option<HoverState>> = Mutex::new(None);
static CLIPBOARD_CB: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

/// 面板展开/收起或窗口移动时更新悬停热区
pub fn set_hover_rect(rect: Rect) {
    let mut guard = HOVER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(st) = guard.as_mut() {
        let was_inside = st.inside;
        st.rect = rect;
        if was_inside && !cursor_inside(&st.rect) {
            st.inside = false;
            (st.on_change)(false);
        }
    }
}

fn cursor_inside(rect: &Rect) -> bool {
    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_ok() {
        rect.contains(pt)
    } else {
        false
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 && wparam.0 as u32 == WM_MOUSEMOVE {
            let pt = (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt;
            let mut guard = HOVER.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(st) = guard.as_mut() {
                let inside = st.rect.contains(pt);
                if inside != st.inside {
                    st.inside = inside;
                    (st.on_change)(inside);
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

unsafe extern "system" fn clip_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_CLIPBOARDUPDATE {
            let guard = CLIPBOARD_CB.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cb) = guard.as_ref() {
                cb();
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// 在 "island-input" 线程上安装鼠标钩子与剪贴板监听，跑消息循环。进程生命周期内调用一次。
pub fn start_input(handlers: InputHandlers) {
    if let (Some(rect), Some(on_hover)) = (handlers.hover_rect, handlers.on_hover) {
        *HOVER.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(HoverState { rect, inside: false, on_change: on_hover });
    }
    if let Some(cb) = handlers.on_clipboard {
        *CLIPBOARD_CB.lock().unwrap_or_else(|e| e.into_inner()) = Some(cb);
    }

    let want_hook = HOVER.lock().unwrap_or_else(|e| e.into_inner()).is_some();
    let want_clip = CLIPBOARD_CB.lock().unwrap_or_else(|e| e.into_inner()).is_some();
    if !want_hook && !want_clip {
        return;
    }

    std::thread::Builder::new()
        .name("island-input".into())
        .spawn(move || unsafe {
            let hmod = GetModuleHandleW(None).expect("module handle");
            let hinst = HINSTANCE::from(hmod);
            if want_hook {
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0).expect("mouse hook");
            }
            if want_clip {
                let class = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(clip_wnd_proc),
                    hInstance: hinst,
                    lpszClassName: w!("TopIslandClip"),
                    ..Default::default()
                };
                RegisterClassExW(&class);
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("TopIslandClip"),
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
            }

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("spawn island-input");
}
