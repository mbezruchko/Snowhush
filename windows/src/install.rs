//! Installing for the current user and removing again, shared by the installer and the app's own uninstaller.
//! No administrator rights: everything lives under the user's profile and registry.

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, LPARAM, WAIT_TIMEOUT, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IPersistFile};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_Programs, FOLDERID_UserProgramFiles, IShellLinkW, KF_FLAG_CREATE, SHGetKnownFolderPath, ShellExecuteW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SW_SHOWNORMAL, SendMessageTimeoutW, WM_CLOSE,
};
use windows::core::{GUID, HSTRING, Interface, PCWSTR, w};

const UNINSTALL: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Snowhush");
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const SETTINGS: PCWSTR = w!("Software\\Snowhush");
const NAME: PCWSTR = w!("Snowhush");
/// The app's hidden window, which a running Snowhush always has.
const CONTROLLER: PCWSTR = w!("SnowhushController");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The version the uninstall entry records, if Snowhush is installed.
pub fn installed_version() -> Option<String> {
    read_string(UNINSTALL, w!("DisplayVersion"))
}

/// Puts the app in place and registers it, replacing a running or older copy. Returns where the app went.
pub fn install(app: &[u8], version: &str) -> Result<PathBuf, String> {
    close_running();
    let folder = known_folder(&FOLDERID_UserProgramFiles).ok_or("the programs folder isn't available")?.join("Snowhush");
    std::fs::create_dir_all(&folder).map_err(|error| format!("couldn't create {}: {error}", folder.display()))?;
    let exe = folder.join("Snowhush.exe");
    let staged = folder.join("Snowhush.exe.new");
    std::fs::write(&staged, app).map_err(|error| format!("couldn't write {}: {error}", staged.display()))?;
    // A copy that was just asked to quit may hold on to its file for a moment longer.
    let started = Instant::now();
    while let Err(error) = std::fs::rename(&staged, &exe) {
        if started.elapsed() > Duration::from_secs(5) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("couldn't replace {}: {error}", exe.display()));
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    if let Some(programs) = known_folder(&FOLDERID_Programs) {
        let _ = shortcut(&exe, &programs.join("Snowhush.lnk"));
    }
    let quoted = format!("\"{}\"", exe.display());
    write_string(UNINSTALL, w!("DisplayName"), "Snowhush");
    write_string(UNINSTALL, w!("DisplayVersion"), version);
    write_string(UNINSTALL, w!("Publisher"), "Mykhailo Bezruchko");
    write_string(UNINSTALL, w!("DisplayIcon"), &exe.display().to_string());
    write_string(UNINSTALL, w!("InstallLocation"), &folder.display().to_string());
    write_string(UNINSTALL, w!("URLInfoAbout"), "https://snowhush.app");
    write_string(UNINSTALL, w!("UninstallString"), &format!("{quoted} --uninstall"));
    write_string(UNINSTALL, w!("QuietUninstallString"), &format!("{quoted} --uninstall --silent"));
    write_number(UNINSTALL, w!("NoModify"), 1);
    write_number(UNINSTALL, w!("NoRepair"), 1);
    write_number(UNINSTALL, w!("EstimatedSize"), app.len().div_ceil(1024) as u32);
    // Open at Login follows the app to where it now lives.
    if read_string(RUN, NAME).is_some() {
        write_string(RUN, NAME, &quoted);
    }
    Ok(exe)
}

/// Removes everything Snowhush put on the computer. The app's own folder goes a moment after this process exits.
pub fn uninstall() {
    close_running();
    if let Some(programs) = known_folder(&FOLDERID_Programs) {
        let _ = std::fs::remove_file(programs.join("Snowhush.lnk"));
    }
    if let Some(data) = known_folder(&FOLDERID_LocalAppData) {
        let _ = std::fs::remove_dir_all(data.join("Snowhush"));
    }
    let folder = read_string(UNINSTALL, w!("InstallLocation"));
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, UNINSTALL);
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, SETTINGS);
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME);
    }
    if let Some(folder) = folder.filter(|folder| folder.ends_with("Snowhush")) {
        let _ = std::process::Command::new("cmd.exe")
            .raw_arg(format!("/c ping 127.0.0.1 -n 3 >nul & rmdir /s /q \"{folder}\""))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

pub fn launch(exe: &Path) {
    let path: Vec<u16> = exe.as_os_str().encode_wide_null();
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(path.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Asks a running Snowhush to quit and waits a few seconds for it to go.
fn close_running() {
    unsafe {
        let Ok(window) = FindWindowW(CONTROLLER, PCWSTR::null()) else { return };
        let mut process = 0;
        GetWindowThreadProcessId(window, Some(&mut process));
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, false, process).ok();
        let _ = SendMessageTimeoutW(window, WM_CLOSE, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 5000, None);
        if let Some(handle) = handle {
            if WaitForSingleObject(handle, 5000) == WAIT_TIMEOUT {
                std::thread::sleep(Duration::from_millis(500));
            }
            let _ = CloseHandle(handle);
        }
    }
}

fn shortcut(target: &Path, at: &Path) -> windows::core::Result<()> {
    let target_path = target.as_os_str().encode_wide_null();
    let folder = target.parent().unwrap_or(target).as_os_str().encode_wide_null();
    let at = at.as_os_str().encode_wide_null();
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(PCWSTR(target_path.as_ptr()))?;
        link.SetWorkingDirectory(PCWSTR(folder.as_ptr()))?;
        link.SetDescription(&HSTRING::from(snowhush_core::text::tr("shortcut")))?;
        link.cast::<IPersistFile>()?.Save(PCWSTR(at.as_ptr()), true)
    }
}

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_CREATE, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const c_void));
        text.map(PathBuf::from)
    }
}

fn read_string(key: PCWSTR, name: PCWSTR) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, key, name, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast()), Some(&mut size)) };
    if status != ERROR_SUCCESS {
        return None;
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn write_string(key: PCWSTR, name: PCWSTR, value: &str) {
    let value: Vec<u16> = value.encode_utf16().chain([0]).collect();
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_SZ.0, Some(value.as_ptr().cast()), (value.len() * 2) as u32);
    }
}

fn write_number(key: PCWSTR, name: PCWSTR, value: u32) {
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_DWORD.0, Some((&raw const value).cast()), 4);
    }
}

trait Wide {
    fn encode_wide_null(&self) -> Vec<u16>;
}

impl Wide for std::ffi::OsStr {
    fn encode_wide_null(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain([0]).collect()
    }
}
