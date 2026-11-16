use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt, ptr::null_mut};
use windows_sys::Win32::{
    Globalization::GetUserDefaultUILanguage, System::Environment::ExpandEnvironmentStringsW,
};

pub(crate) fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}

pub fn italian() -> bool {
    // LANGID stores the primary language in the low ten bits.
    unsafe { GetUserDefaultUILanguage() & 0x03ff == 0x10 }
}

pub fn expand(input: &str) -> io::Result<String> {
    let input = wide(OsStr::new(input));
    let required = unsafe { ExpandEnvironmentStringsW(input.as_ptr(), null_mut(), 0) };
    if required == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0; required as usize];
    let written =
        unsafe { ExpandEnvironmentStringsW(input.as_ptr(), buffer.as_mut_ptr(), required) };
    if written == 0 || written > required {
        return Err(io::Error::other("Unable to expand environment variables"));
    }
    Ok(String::from_utf16_lossy(&buffer[..written as usize - 1]))
}

pub(crate) fn component(value: &str) -> Option<&str> {
    let value = value.trim();
    (value.chars().filter(|c| c.is_alphanumeric()).count() >= 3
        && !value.ends_with(['.', ' '])
        && !value.contains(['/', '\\', ':', '<', '>', '|', '?', '*', '"', '\0']))
    .then_some(value)
}

pub fn matches(query: &str, search_text: &str) -> bool {
    query
        .split_whitespace()
        .all(|word| search_text.contains(word))
}

pub(crate) fn localized(italian: bool, it: &'static str, en: &'static str) -> &'static str {
    if italian { it } else { en }
}

pub(crate) fn size(bytes: u64, italian: bool) -> String {
    let formatted = if bytes >= 1 << 30 {
        format!("{:.1} GiB", bytes as f64 / (1_u64 << 30) as f64)
    } else if bytes >= 1 << 20 {
        format!("{:.1} MiB", bytes as f64 / (1_u64 << 20) as f64)
    } else {
        format!("{} KiB", bytes.div_ceil(1024))
    };
    if italian {
        formatted.replace('.', ",")
    } else {
        formatted
    }
}
