use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows::core::PWSTR;

/// Speaks the first of the user's display languages, in Settings' order, that Snowhush has.
pub fn choose() {
    let (mut count, mut size) = (0u32, 0u32);
    unsafe {
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut size).is_err() || size == 0 {
            return;
        }
        let mut buffer = vec![0u16; size as usize];
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, Some(PWSTR(buffer.as_mut_ptr())), &mut size).is_err() {
            return;
        }
        let tags: Vec<String> = buffer.split(|&unit| unit == 0).filter(|tag| !tag.is_empty()).map(String::from_utf16_lossy).collect();
        snowhush_core::text::choose(&tags);
    }
}
