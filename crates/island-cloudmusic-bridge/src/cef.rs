//! CEF 91（Chromium 91.0.4472，网易云自带 libcef 91.2.2）C API 的最小手写绑定，
//! 只覆盖从 `cef_execute_process` 到注入 JS 这条路径上用到的结构体。
//!
//! 布局是 ABI 关键：CEF C API 就是一张函数指针表，每个结构体以 `CefBaseRefCounted` 开头，
//! 后面按声明顺序每个方法一个指针槽。字段顺序必须和 CEF 91 头文件逐字节一致，
//! 用不到的方法保留为 `usize` 占位以维持偏移。

use std::ffi::c_void;
use std::os::raw::c_int;

/// Windows 上 `cef_string_t` 是 UTF-16 版本。`dtor` 为 None 表示不拥有缓冲区。
#[repr(C)]
pub struct CefStringUtf16 {
    pub str_: *mut u16,
    pub length: usize,
    pub dtor: Option<unsafe extern "system" fn(*mut u16)>,
}

#[repr(C)]
pub struct CefBaseRefCounted {
    pub size: usize,
    pub add_ref: Option<unsafe extern "system" fn(*mut CefBaseRefCounted)>,
    pub release: Option<unsafe extern "system" fn(*mut CefBaseRefCounted) -> c_int>,
    pub has_one_ref: Option<unsafe extern "system" fn(*mut CefBaseRefCounted) -> c_int>,
    pub has_at_least_one_ref: Option<unsafe extern "system" fn(*mut CefBaseRefCounted) -> c_int>,
}

pub type GetRenderProcessHandlerFn =
    unsafe extern "system" fn(*mut CefApp) -> *mut CefRenderProcessHandler;

#[repr(C)]
pub struct CefApp {
    pub base: CefBaseRefCounted,
    pub on_before_command_line_processing: usize,
    pub on_register_custom_schemes: usize,
    pub get_resource_bundle_handler: usize,
    pub get_browser_process_handler: usize,
    pub get_render_process_handler: Option<GetRenderProcessHandlerFn>,
}

pub type OnContextCreatedFn = unsafe extern "system" fn(
    *mut CefRenderProcessHandler,
    *mut c_void, // cef_browser_t*
    *mut CefFrame,
    *mut c_void, // cef_v8context_t*
);

#[repr(C)]
pub struct CefRenderProcessHandler {
    pub base: CefBaseRefCounted,
    pub on_web_kit_initialized: usize,
    pub on_browser_created: usize,
    pub on_browser_destroyed: usize,
    pub get_load_handler: usize,
    pub on_context_created: Option<OnContextCreatedFn>,
    pub on_context_released: usize,
    pub on_uncaught_exception: usize,
    pub on_focused_node_changed: usize,
    pub on_process_message_received: usize,
}

pub type ExecuteJavaScriptFn = unsafe extern "system" fn(
    *mut CefFrame,
    *const CefStringUtf16, // code
    *const CefStringUtf16, // script_url
    c_int,                 // start_line
);
pub type IsMainFn = unsafe extern "system" fn(*mut CefFrame) -> c_int;

#[repr(C)]
pub struct CefFrame {
    pub base: CefBaseRefCounted,
    pub is_valid: usize,
    pub undo: usize,
    pub redo: usize,
    pub cut: usize,
    pub copy: usize,
    pub paste: usize,
    pub del: usize,
    pub select_all: usize,
    pub view_source: usize,
    pub get_source: usize,
    pub get_text: usize,
    pub load_request: usize,
    pub load_url: usize,
    pub execute_java_script: Option<ExecuteJavaScriptFn>,
    pub is_main: Option<IsMainFn>,
    pub is_focused: usize,
    pub get_name: usize,
    pub get_identifier: usize,
    pub get_parent: usize,
    pub get_url: usize,
    pub get_browser: usize,
    pub get_v8context: usize,
    pub visit_dom: usize,
    pub create_urlrequest: usize,
    pub send_process_message: usize,
}

/// 借用调用方持有的 UTF-16 缓冲区。CEF 会同步拷贝输入字符串，缓冲区只需活过调用。
pub struct BorrowedCefString {
    buf: Vec<u16>,
}

impl BorrowedCefString {
    pub fn new(s: &str) -> Self {
        Self { buf: s.encode_utf16().collect() }
    }

    pub fn as_cef(&self) -> CefStringUtf16 {
        CefStringUtf16 {
            str_: self.buf.as_ptr() as *mut u16,
            length: self.buf.len(),
            dtor: None,
        }
    }
}
