use windows::core::{HSTRING, Interface, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSIDFromString, CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Shell::{
    ApplicationActivationManager, IApplicationActivationManager, ShellExecuteW, AO_NONE,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::error::{Result, WinError};

/// 应用注册的 toast activator CLSID（HKCU 优先，其次 HKLM）
fn custom_activator(aumid: &str) -> Option<String> {
    let subkey = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{aumid}"));
    let value = HSTRING::from("CustomActivator");
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut size: u32 = 0;
        let ok = unsafe {
            RegGetValueW(root, &subkey, &value, RRF_RT_REG_SZ, None, None, Some(&mut size))
        };
        if ok.is_err() || size == 0 {
            continue;
        }
        let mut buf = vec![0u16; (size / 2) as usize];
        let ok = unsafe {
            RegGetValueW(
                root,
                &subkey,
                &value,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut size),
            )
        };
        if ok.is_ok() {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            return Some(String::from_utf16_lossy(&buf[..end]));
        }
    }
    None
}

// INotificationActivationCallback：应用注册的 toast 点击回调接口
// （IID 53E31837-6600-4A81-9395-75CFFE746F94，vtable 布局考据自 ToastActivation.cs）。
// windows::core::interface 宏生成的代码按 ::windows_core 绝对路径引用，本 crate 不直接
// 依赖 windows-core（Cargo.toml 不在本域清单内），改为手写 vtable + 手动实现 Interface。
#[repr(transparent)]
#[derive(Clone)]
struct NotificationActivationCallback(windows::core::IUnknown);

#[repr(C)]
struct NotificationActivationCallbackVtbl {
    base: windows::core::IUnknown_Vtbl,
    activate: unsafe extern "system" fn(
        this: *mut core::ffi::c_void,
        app_user_model_id: PCWSTR,
        invoked_args: PCWSTR,
        data: *const NOTIFICATION_USER_INPUT_DATA,
        count: u32,
    ) -> windows::core::HRESULT,
}

unsafe impl Interface for NotificationActivationCallback {
    type Vtable = NotificationActivationCallbackVtbl;
    const IID: windows::core::GUID =
        windows::core::GUID::from_u128(0x53E31837_6600_4A81_9395_75CFFE746F94);
}

impl NotificationActivationCallback {
    /// 复现系统点击回调；key/value 数据对不转发（winbridge 也没传）
    unsafe fn activate(&self, aumid: PCWSTR, invoked_args: PCWSTR) -> windows::core::HRESULT {
        unsafe { (self.vtable().activate)(self.as_raw(), aumid, invoked_args, std::ptr::null(), 0) }
    }
}

/// toast <action> 的 key/value 对。我们复现点击时不转发它们（winbridge 也没传），
/// 但 vtable 签名里这个指针类型要摆在正确的位置上。
#[repr(C)]
struct NOTIFICATION_USER_INPUT_DATA {
    key: PCWSTR,
    value: PCWSTR,
}

/// ShellExecute 返回值 > 32 才是成功（<=32 是历史遗留的错误码，不是 HINSTANCE）
fn shell_open(file: &str, params: Option<&str>) -> bool {
    let verb = HSTRING::from("open");
    let file = HSTRING::from(file);
    let params = params.map(HSTRING::from);
    let result = unsafe {
        ShellExecuteW(
            HWND::default(),
            &verb,
            &file,
            params.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    result.0 as usize > 32
}

/// 复现一次 toast 点击。链路与系统行为一致（ToastActivation.cs）：
/// protocol 直开 → 应用注册的 COM activator（深链，能定位到会话）→
/// IApplicationActivationManager（带参前台启动）→ shell:AppsFolder 兜底（仅拉起应用）。
/// 返回实际生效的方式：protocol/com/aam/shell/failed。
pub fn activate_toast(aumid: &str, launch: &str, atype: &str) -> String {
    // COM 回调要求套间线程；重复初始化返回 S_FALSE，无害
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    if atype.eq_ignore_ascii_case("protocol") && !launch.is_empty() {
        if shell_open(launch, None) {
            return "protocol".into();
        }
        eprintln!("[notify] protocol 直开失败，继续走激活链: {launch}");
    }

    if let Some(clsid) = custom_activator(aumid) {
        match unsafe { CLSIDFromString(&HSTRING::from(clsid.trim())) } {
            Ok(guid) => {
                let created = unsafe {
                    CoCreateInstance::<_, NotificationActivationCallback>(&guid, None, CLSCTX_ALL)
                };
                match created {
                    Ok(callback) => {
                        let hr = unsafe {
                            callback.activate(
                                PCWSTR(HSTRING::from(aumid).as_ptr()),
                                PCWSTR(HSTRING::from(launch).as_ptr()),
                            )
                        };
                        if hr.is_ok() {
                            return "com".into();
                        }
                        eprintln!("[notify] COM 激活回调返回失败({aumid}): {hr:?}");
                    }
                    Err(e) => eprintln!("[notify] 创建 toast activator 失败({aumid}, {clsid}): {e}"),
                }
            }
            Err(e) => eprintln!("[notify] CustomActivator CLSID 非法({aumid}): {clsid}: {e}"),
        }
    }

    let manager = unsafe {
        CoCreateInstance::<_, IApplicationActivationManager>(
            &ApplicationActivationManager,
            None,
            CLSCTX_ALL,
        )
    };
    match manager {
        Ok(manager) => {
            let activated = unsafe {
                manager.ActivateApplication(
                    &HSTRING::from(aumid),
                    &HSTRING::from(launch),
                    AO_NONE,
                )
            };
            match activated {
                Ok(_) => return "aam".into(),
                Err(e) => eprintln!("[notify] AAM 激活失败({aumid}): {e}"),
            }
        }
        Err(e) => eprintln!("[notify] 创建应用激活管理器失败: {e}"),
    }

    if shell_open("explorer.exe", Some(&format!(r"shell:AppsFolder\{aumid}"))) {
        return "shell".into();
    }
    eprintln!("[notify] shell:AppsFolder 兜底失败({aumid})");
    "failed".into()
}

/// spike 期保留的一参版本：不带深链参数直接激活应用。
/// 新代码用 activate_toast；本函数等 spike 命令退役后删除。
pub fn activate(aumid: &str) -> Result<String> {
    if aumid.is_empty() {
        return Err(WinError::api("激活应用", "aumid 为空"));
    }
    Ok(activate_toast(aumid, "", ""))
}
