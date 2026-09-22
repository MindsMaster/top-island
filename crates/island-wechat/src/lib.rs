pub mod decrypt;
pub mod discover;
pub mod dpapi;
pub mod error;
pub mod foreground;
pub mod key;
pub mod reader;
pub mod watch;

pub use discover::{discover_account, Account};
pub use error::{Result, WeChatError};
pub use foreground::foreground_process_stem;
pub use key::{recover_key, RecoveredKey};
pub use reader::{
    body_for, load_contacts, placeholder_for, read_new_messages, ContactInfo, Contacts, RawMessage,
};
pub use zeroize::Zeroizing;

pub fn key_to_hex(key: &[u8; decrypt::KEY_SIZE]) -> Zeroizing<String> {
    Zeroizing::new(hex::encode(key))
}

pub fn key_from_hex(s: &str) -> Option<Zeroizing<[u8; decrypt::KEY_SIZE]>> {
    let bytes = Zeroizing::new(hex::decode(s).ok()?);
    let mut key = Zeroizing::new([0u8; decrypt::KEY_SIZE]);
    if bytes.len() != decrypt::KEY_SIZE {
        return None;
    }
    key.copy_from_slice(&bytes);
    Some(key)
}

#[cfg(test)]
mod tests {
    #[test]
    fn key_hex_roundtrip_restores_key_bytes() {
        let key = [0xABu8; 32];
        let hex = super::key_to_hex(&key);
        let back = super::key_from_hex(&hex).expect("合法 hex 必须能解回密钥");
        assert_eq!(*back, key);
    }

    #[test]
    fn key_from_hex_rejects_wrong_length_and_garbage() {
        assert!(super::key_from_hex("abcd").is_none());
        assert!(super::key_from_hex("不是hex").is_none());
    }
}
