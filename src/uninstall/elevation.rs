use super::{processes::Process, runner::Launch};
use crate::utils::strings::wide;
use std::{ffi::OsStr, io, path::Path, ptr::null};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::{
        Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
        Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
    },
    UI::{
        Shell::{
            SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            ShellExecuteExW,
        },
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

pub(crate) struct ComApartment;

impl ComApartment {
    pub(crate) fn open() -> io::Result<Self> {
        let status = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) };
        if status < 0 {
            return Err(io::Error::other(format!("COM 0x{:08X}", status as u32)));
        }
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub fn run(launch: &Launch) -> io::Result<u32> {
    let _apartment = ComApartment::open()?;
    let executable = wide(launch.executable.as_os_str());
    let arguments = wide(OsStr::new(&launch.arguments));
    let directory = wide(
        launch
            .executable
            .parent()
            .unwrap_or(Path::new(""))
            .as_os_str(),
    );
    let verb = wide(OsStr::new("open"));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: executable.as_ptr(),
        lpParameters: arguments.as_ptr(),
        lpDirectory: directory.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // Pass the executable separately. ShellExecute only handles the original program's UAC.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.hProcess.is_null() {
        return Err(io::Error::other("Uninstaller returned no process handle"));
    }
    let process = Process(info.hProcess);
    if unsafe { WaitForSingleObject(process.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err(io::Error::last_os_error());
    }
    let mut code = 0;
    if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(code)
}
