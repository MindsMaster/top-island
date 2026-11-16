//! 读酷狗已加载的歌词 布局须与 ui.dll 的 LyricText 相符
//! LyricTextLine 32 字节 `{i32 起始, i32 时长, vector<字>}`
//! 字 48 字节 `{i32 偏移, i32 时长, wstring 文字, 8 字节保留}`

use std::ffi::c_void;

const LINE_SIZE: usize = 32;
const WORD_SIZE: usize = 48;
const WORD_TEXT_AT: usize = 8;
const MAX_LINES: usize = 10_000;
const MAX_WORDS: usize = 2_000;
const MAX_CHARS: usize = 4_096;
/// 翻译为 2 行数时间轴同原文
const TRANSLATE_NONE: i32 = 0;
/// MSVC basic_string<wchar_t> 内联容量
const WSTR_INLINE_CAP: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lyric {
    pub title: String,
    pub artist: String,
    pub length_ms: i64,
    pub lines: Vec<(i64, String)>,
}

unsafe fn rd<T: Copy>(p: *const u8, off: usize) -> T {
    unsafe { std::ptr::read_unaligned(p.add(off).cast::<T>()) }
}

/// MSVC 布局 `{union{buf[8], ptr}, size, cap}`
unsafe fn wstr(p: *const u8) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let size: usize = unsafe { rd(p, 16) };
    let cap: usize = unsafe { rd(p, 24) };
    if cap < WSTR_INLINE_CAP || size > cap || size > MAX_CHARS {
        return None;
    }
    let data: *const u16 = if cap > WSTR_INLINE_CAP {
        unsafe { rd(p, 0) }
    } else {
        p.cast()
    };
    if data.is_null() {
        return None;
    }
    Some(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(data, size)
    }))
}

/// vector 布局 `{first, last, end}` 返回首址与元素数
unsafe fn span(v: *const u8, elem: usize, max: usize) -> Option<(*const u8, usize)> {
    let first: usize = unsafe { rd(v, 0) };
    let last: usize = unsafe { rd(v, 8) };
    let bytes = last.checked_sub(first)?;
    if bytes % elem != 0 || bytes / elem > max || (first == 0 && bytes != 0) {
        return None;
    }
    Some((first as *const u8, bytes / elem))
}

/// 参数为 LyricTextLine 的 vector
pub unsafe fn read_lines(v: *const u8) -> Option<Vec<(i64, String)>> {
    let (first, count) = unsafe { span(v, LINE_SIZE, MAX_LINES) }?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let line = unsafe { first.add(i * LINE_SIZE) };
        let start: i32 = unsafe { rd(line, 0) };
        let (words, n) = unsafe { span(line.add(8), WORD_SIZE, MAX_WORDS) }?;
        let mut text = String::new();
        for k in 0..n {
            text.push_str(&unsafe { wstr(words.add(k * WORD_SIZE + WORD_TEXT_AT)) }?);
        }
        out.push((i64::from(start.max(0)), text));
    }
    Some(out)
}

/// 返回成员引用 无所有权
pub type GetRefFn = unsafe extern "system" fn(*const c_void) -> *const u8;
pub type GetIntFn = unsafe extern "system" fn(*const c_void) -> i32;

pub struct Api {
    pub lines: GetRefFn,
    pub title: GetRefFn,
    pub artist: GetRefFn,
    pub length: GetIntFn,
    pub translate_type: GetIntFn,
}

/// 每个歌词控件各收原文与翻译 只取原文
pub unsafe fn read(api: &Api, lyric_text: *const c_void) -> Option<Lyric> {
    unsafe {
        if (api.translate_type)(lyric_text) != TRANSLATE_NONE {
            return None;
        }
        Some(Lyric {
            title: wstr((api.title)(lyric_text))?,
            artist: wstr((api.artist)(lyric_text)).unwrap_or_default(),
            length_ms: i64::from((api.length)(lyric_text).max(0)),
            lines: read_lines((api.lines)(lyric_text))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按 MSVC 布局造 wstring 长串放堆上
    fn make_wstr(s: &str, heap: &mut Vec<Vec<u16>>) -> [u8; 32] {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut out = [0u8; 32];
        let cap = if units.len() <= WSTR_INLINE_CAP {
            for (i, u) in units.iter().enumerate() {
                out[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes());
            }
            WSTR_INLINE_CAP
        } else {
            heap.push(units.clone());
            let ptr = heap.last().unwrap().as_ptr() as usize;
            out[..8].copy_from_slice(&ptr.to_le_bytes());
            units.len()
        };
        out[16..24].copy_from_slice(&units.len().to_le_bytes());
        out[24..32].copy_from_slice(&cap.to_le_bytes());
        out
    }

    fn make_vec(buf: &[u8]) -> [u8; 24] {
        let first = buf.as_ptr() as usize;
        let mut v = [0u8; 24];
        v[..8].copy_from_slice(&first.to_le_bytes());
        v[8..16].copy_from_slice(&(first + buf.len()).to_le_bytes());
        v[16..24].copy_from_slice(&(first + buf.len()).to_le_bytes());
        v
    }

    fn make_words(words: &[&str], heap: &mut Vec<Vec<u16>>) -> Vec<u8> {
        let mut buf = Vec::new();
        for (i, w) in words.iter().enumerate() {
            let mut word = [0u8; WORD_SIZE];
            word[..4].copy_from_slice(&(i as i32 * 100).to_le_bytes());
            word[4..8].copy_from_slice(&100i32.to_le_bytes());
            word[8..40].copy_from_slice(&make_wstr(w, heap));
            buf.extend_from_slice(&word);
        }
        buf
    }

    fn make_line(start: i32, words: &[u8]) -> [u8; LINE_SIZE] {
        let mut line = [0u8; LINE_SIZE];
        line[..4].copy_from_slice(&start.to_le_bytes());
        line[8..32].copy_from_slice(&make_vec(words));
        line
    }

    #[test]
    fn joins_words_into_lines() {
        let mut heap = Vec::new();
        let w0 = make_words(&["洗", "牌", " (", "Live) - "], &mut heap);
        let w1 = make_words(&["有多烫就多烫"], &mut heap);
        let mut lines = Vec::new();
        lines.extend_from_slice(&make_line(892, &w0));
        lines.extend_from_slice(&make_line(35_504, &w1));
        let v = make_vec(&lines);
        let got = unsafe { read_lines(v.as_ptr()) }.unwrap();
        assert_eq!(
            got,
            vec![
                (892, "洗牌 (Live) - ".to_string()),
                (35_504, "有多烫就多烫".to_string())
            ]
        );
    }

    #[test]
    fn empty_vector_is_empty_lyric() {
        let v = [0u8; 24];
        assert_eq!(unsafe { read_lines(v.as_ptr()) }, Some(Vec::new()));
    }

    #[test]
    fn rejects_unexpected_layout() {
        let buf = [0u8; 40];
        let v = make_vec(&buf);
        assert_eq!(unsafe { read_lines(v.as_ptr()) }, None);

        let mut heap = Vec::new();
        let mut bad = make_wstr("abc", &mut heap);
        bad[24..32].copy_from_slice(&2usize.to_le_bytes());
        assert_eq!(unsafe { wstr(bad.as_ptr()) }, None);
    }

    #[test]
    fn long_text_reads_from_heap() {
        let mut heap = Vec::new();
        let s = make_wstr("Live) - SHarK/Rapeter", &mut heap);
        assert_eq!(
            unsafe { wstr(s.as_ptr()) }.as_deref(),
            Some("Live) - SHarK/Rapeter")
        );
    }
}
