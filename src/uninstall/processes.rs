use crate::utils::paths::{comparable, is_local_absolute};
use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
    },
};

pub(crate) struct Process(pub(crate) HANDLE);

impl Drop for Process {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn running_under(location: Option<&Path>) -> Vec<u32> {
    let Some(path) = location.and_then(|path| path.canonicalize().ok()) else {
        return Vec::new();
    };
    if !is_local_absolute(&path) || path.parent().and_then(Path::parent).is_none() {
        return Vec::new();
    }
    let parent = comparable(&path);
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let snapshot = Process(handle);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut next = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    let mut matching = Vec::new();
    let mut buffer = vec![0; 32768];
    while next != 0 {
        let pid = entry.th32ProcessID;
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if !handle.is_null() {
            let process = Process(handle);
            let mut size = buffer.len() as u32;
            if unsafe { QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut size) }
                != 0
            {
                let image = PathBuf::from(OsString::from_wide(&buffer[..size as usize]));
                if comparable(&image).starts_with(&parent) {
                    matching.push(pid);
                }
            }
        }
        next = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    matching
}

pub fn close_visible_windows(location: &Path) -> usize {
    use windows_sys::{
        Win32::Foundation::{HWND, LPARAM},
        core::BOOL,
    };

    struct Close {
        processes: Vec<u32>,
        windows: usize,
    }

    unsafe extern "system" fn send(hwnd: HWND, context: LPARAM) -> BOOL {
        // EnumWindows calls this synchronously while the stack-owned context is alive.
        let state = unsafe { &mut *(context as *mut Close) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if state.processes.contains(&pid)
            && unsafe { IsWindowVisible(hwnd) } != 0
            && unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) } != 0
        {
            state.windows += 1;
        }
        1
    }

    let mut state = Close {
        processes: running_under(Some(location))
            .into_iter()
            .filter(|pid| *pid != std::process::id())
            .collect(),
        windows: 0,
    };
    if !state.processes.is_empty() {
        unsafe { EnumWindows(Some(send), (&raw mut state) as LPARAM) };
    }
    state.windows
}
