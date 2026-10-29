use std::ffi::c_void;

use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle,
    WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use snowhush_core::version::{field, newer};

const REPOSITORY: &str = "mbezruchko/Snowhush";
const INSTALLER: &str = "Snowhush-Setup.exe";

/// A release newer than this build that carries an installer for Windows.
pub struct Release {
    pub version: String,
    pub url: String,
}

/// Asks GitHub for the latest release. Releases with nothing for Windows don't count as updates.
pub fn check() -> Result<Option<Release>, String> {
    let body = fetch(&format!("/repos/{REPOSITORY}/releases/latest"))?;
    let tag = field(&body, "tag_name").ok_or("the latest release has no tag")?;
    let version = tag.trim_start_matches('v');
    if !body.contains(&format!("\"{INSTALLER}\"")) || !newer(version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    Ok(Some(Release { version: version.to_string(), url: format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{INSTALLER}") }))
}

pub fn open(url: &str) {
    let url: Vec<u16> = url.encode_utf16().chain([0]).collect();
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(url.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn fetch(path: &str) -> Result<String, String> {
    let failed = |what: &str| format!("{what} failed: {}", windows::core::Error::from_thread().message());
    let path: Vec<u16> = path.encode_utf16().chain([0]).collect();
    unsafe {
        let session = Handle(WinHttpOpen(w!("Snowhush"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err(failed("Opening a connection"));
        }
        let _ = WinHttpSetTimeouts(session.0, 10_000, 10_000, 10_000, 10_000);
        let connection = Handle(WinHttpConnect(session.0, w!("api.github.com"), 443, 0));
        if connection.0.is_null() {
            return Err(failed("Connecting to GitHub"));
        }
        let request = Handle(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err(failed("Preparing the request"));
        }
        let headers: Vec<u16> = "Accept: application/vnd.github+json\r\n".encode_utf16().collect();
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(|error| error.message())?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|error| error.message())?;

        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|error| error.message())?;
        if status != 200 {
            return Err(format!("GitHub answered {status}"));
        }

        let mut body = Vec::new();
        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read).map_err(|error| error.message())?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        Ok(String::from_utf8_lossy(&body).into_owned())
    }
}
