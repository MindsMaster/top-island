use windows::core::{Interface, HSTRING, PCWSTR};
use windows::Win32::System::Com::{
    CLSIDFromString, CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    ApplicationActivationManager, IApplicationActivationManager, ShellExecuteW, AO_NONE,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn custom_activator(aumid: &str) -> Option<String> {
    crate::appid::registry_value(aumid, "CustomActivator")
}

/// INotificationActivationCallback IID 53E31837-6600-4A81-9395-75CFFE746F94
/// vtable 布局考据自 ToastActivation.cs
/// interface 宏引用 ::windows_core 本 crate 无直接依赖 故手写 vtable
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
    /// key/value 数据对不转发 与 winbridge 一致
    unsafe fn activate(&self, aumid: PCWSTR, invoked_args: PCWSTR) -> windows::core::HRESULT {
        unsafe { (self.vtable().activate)(self.as_raw(), aumid, invoked_args, std::ptr::null(), 0) }
    }
}

/// vtable 占位 数据不转发
#[repr(C)]
struct NOTIFICATION_USER_INPUT_DATA {
    key: PCWSTR,
    value: PCWSTR,
}

/// ShellExecute 返回值 >32 才成功 <=32 是错误码
fn shell_open(file: &str, params: Option<&str>) -> bool {
    let verb = HSTRING::from("open");
    let file = HSTRING::from(file);
    let params = params.map(HSTRING::from);
    let result = unsafe {
        ShellExecuteW(
            None,
            &verb,
            &file,
            params
                .as_ref()
                .map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    result.0 as usize > 32
}

/// 链路与 ToastActivation.cs 一致
pub fn activate_toast(aumid: &str, launch: &str, atype: &str) -> String {
    // 重复 CoInitializeEx 返回 S_FALSE 无害
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
                    Err(e) => {
                        eprintln!("[notify] 创建 toast activator 失败({aumid}, {clsid}): {e}")
                    }
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
                manager.ActivateApplication(&HSTRING::from(aumid), &HSTRING::from(launch), AO_NONE)
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
