use std::{
    ffi::c_void,
    io,
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::Networking::WinHttp::*;

struct Http(*mut c_void);

impl Http {
    fn new(value: *mut c_void) -> io::Result<Self> {
        if value.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(value))
        }
    }
}

impl Drop for Http {
    fn drop(&mut self) {
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn ok(value: i32) -> io::Result<()> {
    if value == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn get(host: &str, path: &str, limit: usize) -> io::Result<Vec<u8>> {
    let path = wide(path);
    let (_session, connection) = connect(host)?;
    let request = Http::new(unsafe {
        WinHttpOpenRequest(
            connection.0,
            null(),
            path.as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        )
    })?;
    let redirect = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
    ok(unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            (&raw const redirect).cast(),
            size_of::<u32>() as u32,
        )
    })?;
    ok(unsafe { WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) })?;
    ok(unsafe { WinHttpReceiveResponse(request.0, null_mut()) })?;
    let status = status(&request)?;
    if status != 200 {
        return Err(io::Error::other(format!("Release server HTTP {status}")));
    }
    let started = Instant::now();
    let mut bytes = Vec::new();
    let mut buffer = [0; 32 * 1024];
    loop {
        let mut read = 0;
        ok(unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
        })?;
        if read == 0 {
            return Ok(bytes);
        }
        if bytes.len() + read as usize > limit || started.elapsed() > Duration::from_secs(120) {
            return Err(io::Error::other("Release download exceeded its limit"));
        }
        bytes.extend_from_slice(&buffer[..read as usize]);
    }
}

fn connect(host: &str) -> io::Result<(Http, Http)> {
    let agent = wide("Sakura Uninstaller");
    let session = Http::new(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        )
    })?;
    let tls = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2;
    ok(unsafe {
        WinHttpSetOption(
            session.0,
            WINHTTP_OPTION_SECURE_PROTOCOLS,
            (&raw const tls).cast(),
            size_of::<u32>() as u32,
        )
    })?;
    ok(unsafe { WinHttpSetTimeouts(session.0, 5_000, 5_000, 5_000, 10_000) })?;
    let connection = Http::new(unsafe { WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0) })?;
    Ok((session, connection))
}
fn status(request: &Http) -> io::Result<u32> {
    let mut status = 0_u32;
    let mut size = size_of::<u32>() as u32;
    ok(unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&raw mut status).cast(),
            &mut size,
            null_mut(),
        )
    })?;
    Ok(status)
}

pub(crate) fn post_json(host: &str, path: &str, body: &[u8]) -> io::Result<()> {
    let (_session, connection) = connect(host)?;
    let request = Http::new(unsafe {
        WinHttpOpenRequest(
            connection.0,
            wide("POST").as_ptr(),
            wide(path).as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        )
    })?;
    let features =
        WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_AUTHENTICATION;
    ok(unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_DISABLE_FEATURE,
            (&raw const features).cast(),
            size_of::<u32>() as u32,
        )
    })?;
    let length = u32::try_from(body.len()).map_err(io::Error::other)?;
    let headers = wide("Content-Type: application/json");
    ok(unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            u32::MAX,
            body.as_ptr().cast(),
            length,
            length,
            0,
        )
    })?;
    ok(unsafe { WinHttpReceiveResponse(request.0, null_mut()) })?;
    let status = status(&request)?;
    if !(200..300).contains(&status) {
        return Err(io::Error::other(format!("Diagnostic server HTTP {status}")));
    }
    Ok(())
}
