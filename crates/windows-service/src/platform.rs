use std::{
    ffi::c_void,
    iter,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::{null, null_mut},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicPtr, Ordering},
    },
};

use tokio::sync::Notify;
use windows_sys::Win32::{
    Foundation::{ERROR_CALL_NOT_IMPLEMENTED, ERROR_SERVICE_SPECIFIC_ERROR, NO_ERROR},
    System::Services::{
        RegisterServiceCtrlHandlerExW, SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP,
        SERVICE_CONTROL_INTERROGATE, SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP,
        SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STATUS_HANDLE,
        SERVICE_STOP_PENDING, SERVICE_STOPPED, SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
        SetServiceStatus, StartServiceCtrlDispatcherW,
    },
};

use crate::{ServiceError, validate_service_name};

type Handler = Box<dyn FnOnce(ServiceShutdown) -> bool + Send>;

static SERVICE_NAME: OnceLock<Vec<u16>> = OnceLock::new();
static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);
static STATUS_LOCK: Mutex<()> = Mutex::new(());
static STATUS_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
static STOP_NOTIFY: Notify = Notify::const_new();

#[derive(Clone, Copy, Debug)]
pub struct ServiceShutdown;

impl ServiceShutdown {
    pub async fn cancelled(self) {
        if STOP_REQUESTED.load(Ordering::Acquire) {
            return;
        }
        STOP_NOTIFY.notified().await;
    }
}

/// Connects the process to SCM and runs one service handler.
///
/// # Errors
///
/// Returns an error for an invalid name, repeated initialization, or dispatcher failure.
pub fn run_service<F>(name: &str, handler: F) -> Result<(), ServiceError>
where
    F: FnOnce(ServiceShutdown) -> bool + Send + 'static,
{
    validate_service_name(name)?;
    let wide = name.encode_utf16().chain(iter::once(0)).collect::<Vec<_>>();
    SERVICE_NAME
        .set(wide)
        .map_err(|_| ServiceError::AlreadyInitialized)?;
    HANDLER
        .lock()
        .map_err(|_| ServiceError::AlreadyInitialized)?
        .replace(Box::new(handler));
    let name_pointer = SERVICE_NAME
        .get()
        .ok_or(ServiceError::AlreadyInitialized)?
        .as_ptr();
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name_pointer.cast_mut(),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: null_mut(),
            lpServiceProc: None,
        },
    ];
    let started = unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) };
    if started == 0 {
        return Err(ServiceError::DispatcherFailed);
    }
    Ok(())
}

unsafe extern "system" fn service_main(_argument_count: u32, _arguments: *mut *mut u16) {
    let Some(name) = SERVICE_NAME.get() else {
        return;
    };
    let handle =
        unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control_handler), null()) };
    if handle.is_null() {
        return;
    }
    STATUS_HANDLE.store(handle, Ordering::Release);
    if !report_active_status(SERVICE_START_PENDING, 0, 1, 30_000) {
        return;
    }
    if !report_active_status(
        SERVICE_RUNNING,
        SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
        0,
        0,
    ) {
        return;
    }
    let handler = HANDLER.lock().ok().and_then(|mut value| value.take());
    let succeeded = handler.is_some_and(|handler| {
        catch_unwind(AssertUnwindSafe(|| handler(ServiceShutdown))).unwrap_or(false)
    });
    let exit_code = if succeeded {
        NO_ERROR
    } else {
        ERROR_SERVICE_SPECIFIC_ERROR
    };
    let service_code = u32::from(!succeeded);
    let _ = report_stopped(exit_code, service_code);
}

unsafe extern "system" fn control_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut c_void,
    _context: *mut c_void,
) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            let Ok(_status_guard) = STATUS_LOCK.lock() else {
                return ERROR_SERVICE_SPECIFIC_ERROR;
            };
            if !STOP_REQUESTED.swap(true, Ordering::AcqRel) {
                let _ = report_status(SERVICE_STOP_PENDING, 0, 1, 30_000);
                STOP_NOTIFY.notify_one();
            }
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

fn report_active_status(state: u32, accepted: u32, checkpoint: u32, wait_hint: u32) -> bool {
    let Ok(_status_guard) = STATUS_LOCK.lock() else {
        return false;
    };
    if STOP_REQUESTED.load(Ordering::Acquire) {
        report_status(SERVICE_STOP_PENDING, 0, 1, 30_000)
    } else {
        report_status(state, accepted, checkpoint, wait_hint)
    }
}

fn report_status(state: u32, accepted: u32, checkpoint: u32, wait_hint: u32) -> bool {
    let handle = STATUS_HANDLE.load(Ordering::Acquire) as SERVICE_STATUS_HANDLE;
    if handle.is_null() {
        return false;
    }
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: accepted,
        dwWin32ExitCode: NO_ERROR,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: checkpoint,
        dwWaitHint: wait_hint,
    };
    unsafe { SetServiceStatus(handle, &raw const status) != 0 }
}

fn report_stopped(exit_code: u32, service_code: u32) -> bool {
    let Ok(_status_guard) = STATUS_LOCK.lock() else {
        return false;
    };
    let handle = STATUS_HANDLE.load(Ordering::Acquire) as SERVICE_STATUS_HANDLE;
    if handle.is_null() {
        return false;
    }
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: SERVICE_STOPPED,
        dwControlsAccepted: 0,
        dwWin32ExitCode: exit_code,
        dwServiceSpecificExitCode: service_code,
        dwCheckPoint: 0,
        dwWaitHint: 0,
    };
    unsafe { SetServiceStatus(handle, &raw const status) != 0 }
}
