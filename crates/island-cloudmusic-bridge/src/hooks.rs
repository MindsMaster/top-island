use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use retour::GenericDetour;
use windows::core::{s, w, PCSTR};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};

use crate::cef::{
    BorrowedCefString, CefApp, CefFrame, CefRenderProcessHandler, GetRenderProcessHandlerFn,
    OnContextCreatedFn,
};

const BOOTSTRAP_JS: &str = include_str!("../js/bridge.js");

type ExecuteProcessFn = unsafe extern "system" fn(*const c_void, *mut CefApp, *const c_void) -> i32;

static EXECUTE_PROCESS_DETOUR: OnceLock<GenericDetour<ExecuteProcessFn>> = OnceLock::new();
static ORIG_GET_RPH: AtomicUsize = AtomicUsize::new(0);
static ORIG_ON_CONTEXT_CREATED: AtomicUsize = AtomicUsize::new(0);

pub fn install() -> InstallOutcome {
    if let Some(module) = libcef_if_loaded() {
        return do_install(module);
    }
    std::thread::spawn(|| {
        for _ in 0..6000 {
            if let Some(module) = libcef_if_loaded() {
                let _ = do_install(module);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if let Some(module) = load_libcef() {
            let _ = do_install(module);
        }
    });
    InstallOutcome::Deferred
}

#[derive(Debug, Clone, Copy)]
pub enum InstallOutcome {
    Hooked,
    Deferred,
    Failed,
    /// 布局钉在 CEF 91 非白名单版本会崩宿主
    Incompatible {
        #[allow(dead_code)]
        major: i32,
    },
}

const COMPATIBLE_MAJORS: &[i32] = &[91];

type VersionInfoFn = unsafe extern "system" fn(i32) -> i32;

/// cef_version_info(0)=CEF 主版本 (4)=Chromium 主版本
fn cef_version_verdict(module: HMODULE) -> (bool, i32) {
    let Some(f) = export(module, s!("cef_version_info")) else {
        return (false, -1);
    };
    let version_info: VersionInfoFn = unsafe { std::mem::transmute(f) };
    let cef_major = unsafe { version_info(0) };
    let chrome_major = unsafe { version_info(4) };
    let reported = if chrome_major > 0 {
        chrome_major
    } else {
        cef_major
    };
    (is_compatible_major(cef_major, chrome_major), reported)
}

fn is_compatible_major(cef_major: i32, chrome_major: i32) -> bool {
    COMPATIBLE_MAJORS.contains(&cef_major) || COMPATIBLE_MAJORS.contains(&chrome_major)
}

fn libcef_if_loaded() -> Option<HMODULE> {
    match unsafe { GetModuleHandleW(w!("libcef.dll")) } {
        Ok(m) if !m.is_invalid() => Some(m),
        _ => None,
    }
}

fn load_libcef() -> Option<HMODULE> {
    match unsafe { LoadLibraryW(w!("libcef.dll")) } {
        Ok(m) if !m.is_invalid() => Some(m),
        _ => None,
    }
}

fn export(module: HMODULE, name: PCSTR) -> Option<*const c_void> {
    unsafe { GetProcAddress(module, name) }.map(|f| f as *const c_void)
}

fn do_install(module: HMODULE) -> InstallOutcome {
    if EXECUTE_PROCESS_DETOUR.get().is_some() {
        return InstallOutcome::Hooked;
    }

    let (compatible, major) = cef_version_verdict(module);
    if !compatible {
        return InstallOutcome::Incompatible { major };
    }

    let Some(exec) = export(module, s!("cef_execute_process")) else {
        return InstallOutcome::Failed;
    };

    unsafe {
        let exec_target: ExecuteProcessFn = std::mem::transmute(exec);
        let Ok(detour) = GenericDetour::new(exec_target, hook_execute_process) else {
            return InstallOutcome::Failed;
        };
        // enable 后 hook 随时被调 须先存好 trampoline
        if EXECUTE_PROCESS_DETOUR.set(detour).is_err() {
            return InstallOutcome::Hooked;
        }
        let Some(d) = EXECUTE_PROCESS_DETOUR.get() else {
            return InstallOutcome::Failed;
        };
        if d.enable().is_err() {
            return InstallOutcome::Failed;
        }
    }
    InstallOutcome::Hooked
}

unsafe extern "system" fn hook_execute_process(
    args: *const c_void,
    app: *mut CefApp,
    sandbox: *const c_void,
) -> i32 {
    wrap_app(app);
    let d = EXECUTE_PROCESS_DETOUR
        .get()
        .expect("detour published before enable");
    unsafe { d.call(args, app, sandbox) }
}

/// panic 展开进 C 回调是 UB 一律 catch_unwind
fn wrap_app(app: *mut CefApp) {
    if app.is_null() {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        let app = &mut *app;
        let current = app.get_render_process_handler;
        let ours = hook_get_render_process_handler as *const () as usize;
        if current.map_or(0, |f| f as usize) == ours {
            return;
        }
        ORIG_GET_RPH.store(current.map_or(0, |f| f as usize), Ordering::Release);
        app.get_render_process_handler = Some(hook_get_render_process_handler);
    }));
}

unsafe extern "system" fn hook_get_render_process_handler(
    app: *mut CefApp,
) -> *mut CefRenderProcessHandler {
    let orig = ORIG_GET_RPH.load(Ordering::Acquire);
    if orig == 0 {
        return std::ptr::null_mut();
    }
    let handler = unsafe {
        let f: GetRenderProcessHandlerFn = std::mem::transmute(orig);
        f(app)
    };
    if !handler.is_null() {
        unsafe {
            let h = &mut *handler;
            let ours = hook_on_context_created as *const () as usize;
            if h.on_context_created.map_or(0, |f| f as usize) != ours {
                ORIG_ON_CONTEXT_CREATED.store(
                    h.on_context_created.map_or(0, |f| f as usize),
                    Ordering::Release,
                );
                h.on_context_created = Some(hook_on_context_created);
            }
        }
    }
    handler
}

unsafe extern "system" fn hook_on_context_created(
    handler: *mut CefRenderProcessHandler,
    browser: *mut c_void,
    frame: *mut CefFrame,
    context: *mut c_void,
) {
    // 让 NCM 先初始化完自己的上下文
    let orig = ORIG_ON_CONTEXT_CREATED.load(Ordering::Acquire);
    if orig != 0 {
        unsafe {
            let f: OnContextCreatedFn = std::mem::transmute(orig);
            f(handler, browser, frame, context);
        }
    }

    inject_bootstrap(frame);
}

/// 只注入主帧 非 orpheus 页面脚本自返
fn inject_bootstrap(frame: *mut CefFrame) {
    if frame.is_null() {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        let f = &*frame;
        let Some(is_main) = f.is_main else { return };
        if is_main(frame) == 0 {
            return;
        }
        let Some(execute) = f.execute_java_script else {
            return;
        };

        let code = BorrowedCefString::new(BOOTSTRAP_JS);
        let url = BorrowedCefString::new("topisland://bridge/bootstrap.js");
        let code_cef = code.as_cef();
        let url_cef = url.as_cef();
        execute(frame, &code_cef, &url_cef, 0);
    }));
}

#[cfg(test)]
mod tests {
    use super::is_compatible_major;

    #[test]
    fn only_whitelisted_majors_are_compatible() {
        assert!(is_compatible_major(91, 91));
        assert!(!is_compatible_major(120, 120));
        assert!(!is_compatible_major(-1, -1));
        assert!(is_compatible_major(91, 0));
    }
}
