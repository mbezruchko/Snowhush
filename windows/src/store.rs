//! Running as the Microsoft Store's package: the Store keeps the app up to date, and opening at login goes through the
//! package's startup task, since the package keeps its own copy of the registry's Run key.

use std::sync::OnceLock;

use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::core::h;

/// Whether this copy runs from an installed package, as the Store's does.
pub fn packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut length = 0u32;
        unsafe { GetCurrentPackageFullName(&mut length, None) != APPMODEL_ERROR_NO_PACKAGE }
    })
}

/// The startup task the package's manifest declares.
fn task() -> windows::core::Result<StartupTask> {
    StartupTask::GetAsync(h!("SnowhushStartup"))?.join()
}

pub fn opens_at_login() -> bool {
    task().and_then(|task| task.State()).is_ok_and(|state| state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy)
}

/// Turned off in Task Manager's Startup apps, the task can only be turned back on there.
pub fn set_opens_at_login(on: bool) -> WIN32_ERROR {
    let result = task().and_then(|task| if on { task.RequestEnableAsync()?.join().map(Some) } else { task.Disable().map(|()| None) });
    match result {
        Ok(Some(state)) if state != StartupTaskState::Enabled && state != StartupTaskState::EnabledByPolicy => {
            crate::log!("startup task stays in state {}", state.0);
            WIN32_ERROR(1)
        }
        Ok(_) => ERROR_SUCCESS,
        Err(error) => {
            crate::log!("startup task: {}", error.message());
            WIN32_ERROR(error.code().0 as u32)
        }
    }
}
