//! Native minimize/maximize for the frameless main window.
//!
//! Slint's `minimized` property is not synchronized back by the windowing layer,
//! so the titlebar asks the operating system directly instead.

#[cfg(windows)]
fn with_main_window(action: impl FnOnce(*mut core::ffi::c_void)) {
    use windows_sys::{
        Win32::{
            Foundation::{HWND, LPARAM},
            System::Threading::GetCurrentProcessId,
            UI::WindowsAndMessaging::{
                EnumWindows, GW_OWNER, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId,
                IsWindowVisible,
            },
        },
        core::BOOL,
    };

    struct Search {
        pid: u32,
        found: HWND,
    }

    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid != search.pid {
            return 1;
        }
        let titled = unsafe { GetWindowTextLengthW(hwnd) } > 0;
        if titled
            && unsafe { IsWindowVisible(hwnd) } != 0
            && unsafe { GetWindow(hwnd, GW_OWNER) }.is_null()
        {
            search.found = hwnd;
            return 0;
        }
        1
    }

    let mut search = Search {
        pid: unsafe { GetCurrentProcessId() },
        found: core::ptr::null_mut(),
    };
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    if !search.found.is_null() {
        action(search.found);
    }
}

#[cfg(windows)]
pub(crate) fn minimize() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_MINIMIZE, ShowWindow};

    with_main_window(|hwnd| unsafe {
        ShowWindow(hwnd, SW_MINIMIZE);
    });
}

/// Maximizes or restores the window and reports the resulting state.
#[cfg(windows)]
pub(crate) fn toggle_maximize() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsZoomed, SW_MAXIMIZE, SW_RESTORE, ShowWindow,
    };

    let mut maximized = false;
    with_main_window(|hwnd| unsafe {
        let current = IsZoomed(hwnd) != 0;
        ShowWindow(hwnd, if current { SW_RESTORE } else { SW_MAXIMIZE });
        maximized = IsZoomed(hwnd) != 0;
    });
    maximized
}

#[cfg(not(windows))]
pub(crate) fn minimize() {}

#[cfg(not(windows))]
pub(crate) fn toggle_maximize() -> bool {
    false
}
