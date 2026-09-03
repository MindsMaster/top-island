//! DPAPI 包装：CryptProtectData / CryptUnprotectData 绑当前 Windows 用户的
//! 主密钥加解密。微信密钥落盘前必须过这层，明文只存在于内存
//!（Electron 版把密钥明文直接写进 store.json，任何能读 %APPDATA% 的进程都能拿走）。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

use crate::error::{Result, WeChatError};

pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(&input, PCWSTR::null(), None, None, None, 0, &mut output)
            .map_err(|e| WeChatError::api("CryptProtectData", e))?;
        let blob = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(output.pbData as *mut _));
        Ok(blob)
    }
}

pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: blob.len() as u32,
        pbData: blob.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, 0, &mut output)
            .map_err(|e| WeChatError::api("CryptUnprotectData", e))?;
        let data = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(output.pbData as *mut _));
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_roundtrip_restores_original_bytes() {
        let secret = b"0123456789abcdef0123456789abcdef";
        let blob = protect(secret).expect("当前用户下 CryptProtectData 必须可用");
        assert_ne!(blob.as_slice(), secret, "保护后的字节不能还是明文");
        let back = unprotect(&blob).expect("同一用户必须能解回自己保护的数据");
        assert_eq!(back, secret, "DPAPI 往返必须还原原始字节，否则密钥缓存读写是坏的");
    }

    #[test]
    fn unprotect_rejects_garbage_blob() {
        assert!(unprotect(&[0xDE, 0xAD, 0xBE, 0xEF]).is_err(), "垃圾数据必须报错而不是静默返回空");
    }
}
