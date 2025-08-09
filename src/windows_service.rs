use windows::core::PWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Security::*;
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, OpenInputDesktop, SetThreadDesktop, DESKTOP_ACCESS_FLAGS,
    DF_ALLOWOTHERACCOUNTHOOK, HDESK,
};
use windows::Win32::System::Threading::{
    CreateProcessAsUserW, GetCurrentProcess, OpenProcessToken, CREATE_NO_WINDOW,
    CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

use anyhow::Result;

use core::ffi::c_void;
use std::cell::Cell;
use std::ffi::{OsStr, OsString};
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::time::Duration;

const SERVICE_NAME: &str = "Tenebra";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

struct RAIIGuard<F: FnOnce()> {
    cleanup: Option<F>,
}

impl<F: FnOnce()> RAIIGuard<F> {
    fn new(cleanup: F) -> Self {
        RAIIGuard {
            cleanup: Some(cleanup),
        }
    }
}

impl<F: FnOnce()> Drop for RAIIGuard<F> {
    fn drop(&mut self) {
        if let Some(f) = self.cleanup.take() {
            f();
        }
    }
}

pub fn run() -> Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

define_windows_service!(ffi_service_main, service_main);

pub fn service_main(_arguments: Vec<OsString>) {
    create_dir_all("C:\\tenebra").unwrap();
    let mut file = File::create("C:\\tenebra\\tenebra_log.txt").unwrap();
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "1");
    }
    if let Err(e) = run_service() {
        writeln!(&mut file, "Error: {:?}", e).unwrap();
    }
}

pub fn run_service() -> Result<()> {
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            // There's no need to handle the stop event because the service exists immediately
            ServiceControl::Stop => ServiceControlHandlerResult::NoError,
            ServiceControl::UserEvent(_code) => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;
    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    // Launch the process
    unsafe {
        let mut token_handle = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_DUPLICATE | TOKEN_QUERY,
            &mut token_handle,
        )?;
        let _token_guard = RAIIGuard::new(|| {
            let _ = CloseHandle(token_handle);
        });

        // Give ourselves SeTcbPrivilege
        let mut luid = LUID::default();
        LookupPrivilegeValueW(None, SE_TCB_NAME, &mut luid)?;
        let mut new_privs = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            ..Default::default()
        };
        new_privs.Privileges[0].Luid = luid;
        new_privs.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;

        AdjustTokenPrivileges(
            token_handle,
            false, // Do not disable all other privileges
            Some(&new_privs),
            0,    // Buffer length for previous state (not needed)
            None, // Pointer to previous state (not needed)
            None, // Return length (not needed)
        )?;

        let mut new_token_handle = HANDLE::default();
        DuplicateTokenEx(
            token_handle,
            TOKEN_ACCESS_MASK(windows::Win32::System::SystemServices::MAXIMUM_ALLOWED),
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut new_token_handle,
        )?;
        let _new_token_guard = RAIIGuard::new(|| {
            let _ = CloseHandle(new_token_handle);
        });

        // Get the session ID of the active user's session
        let sid = WTSGetActiveConsoleSessionId();
        if sid == 0xFFFFFFFF {
            // error, abort
            anyhow::bail!("Bad session ID from WTSGetActiveConsoleSessionId");
        }
        SetTokenInformation(
            new_token_handle,
            TokenSessionId,
            &sid as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        )?;

        let mut env_block: *mut c_void = std::ptr::null_mut();
        CreateEnvironmentBlock(&mut env_block, Some(new_token_handle), false)?; // Use default env
        let _env_block_guard = RAIIGuard::new(|| {
            let _ = DestroyEnvironmentBlock(env_block);
        });

        let mut si: STARTUPINFOW = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut desktop_name: Vec<u16> = OsStr::new("winsta0\\default")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        si.lpDesktop = PWSTR(desktop_name.as_mut_ptr());

        // Relying on the output of current_exe is NOT a security risk, because an attacker
        // cannot swap this executable out for a new executable while the service is running.
        // Windows prevents users from deleting the executable of a running service.
        let command = format!("{} --console", std::env::current_exe()?.display());
        let mut command_wide: Vec<u16> = OsStr::new(&command)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        CreateProcessAsUserW(
            Some(new_token_handle),
            None,
            Some(PWSTR(command_wide.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            Some(env_block),
            None,
            &si,
            &mut pi,
        )?;

        let _ = CloseHandle(pi.hProcess);
        let _ = CloseHandle(pi.hThread);
    }

    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

thread_local! {
    static CURRENT_DESKTOP: Cell<Option<HDESK>> = const { Cell::new(None) };
}

pub fn sync_thread_desktop() -> Result<()> {
    unsafe {
        let new_desktop = OpenInputDesktop(
            DF_ALLOWOTHERACCOUNTHOOK,
            false,
            DESKTOP_ACCESS_FLAGS(0x10000000),
        )?;

        CURRENT_DESKTOP.with(|cell| {
            let current = cell.get();

            let should_switch = match current {
                Some(current) if current == new_desktop => {
                    CloseDesktop(new_desktop).ok(); // Already using it; discard duplicate handle
                    return Ok(());
                }
                Some(old) => {
                    SetThreadDesktop(new_desktop)?; // Switch first
                    CloseDesktop(old).ok(); // Then safely close old
                    cell.set(Some(new_desktop));
                    Ok(())
                }
                None => {
                    SetThreadDesktop(new_desktop)?;
                    cell.set(Some(new_desktop));
                    Ok(())
                }
            };

            should_switch
        })
    }
}
