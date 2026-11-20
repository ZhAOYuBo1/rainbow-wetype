use anyhow::{Context, anyhow};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};

type ScHandle = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct ServiceStatus {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    check_point: u32,
    wait_hint: u32,
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenSCManagerW(machine: *const u16, database: *const u16, access: u32) -> ScHandle;
    fn OpenServiceW(scm: ScHandle, name: *const u16, access: u32) -> ScHandle;
    fn ControlService(service: ScHandle, control: u32, status: *mut ServiceStatus) -> i32;
    fn StartServiceW(service: ScHandle, argc: u32, argv: *const *const u16) -> i32;
    fn QueryServiceStatus(service: ScHandle, status: *mut ServiceStatus) -> i32;
    fn CloseServiceHandle(handle: ScHandle) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn IsUserAnAdmin() -> i32;
}

const SC_MANAGER_CONNECT: u32 = 0x0001;
const SERVICE_QUERY_STATUS: u32 = 0x0004;
const SERVICE_START: u32 = 0x0010;
const SERVICE_STOP: u32 = 0x0020;
const SERVICE_CONTROL_STOP: u32 = 0x00000001;

const STATE_STOPPED: u32 = 1;
const STATE_RUNNING: u32 = 4;

pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn open_scm() -> anyhow::Result<ScHandle> {
    let h = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
    if h.is_null() {
        return Err(anyhow!("OpenSCManagerW failed"));
    }
    Ok(h)
}

fn open_service(scm: ScHandle, name: &str, access: u32) -> anyhow::Result<ScHandle> {
    let wname = wide(name);
    let h = unsafe { OpenServiceW(scm, wname.as_ptr(), access) };
    if h.is_null() {
        return Err(anyhow!("OpenServiceW failed: {name}"));
    }
    Ok(h)
}

fn query_state(service: ScHandle) -> anyhow::Result<u32> {
    let mut status = ServiceStatus::default();
    let ok = unsafe { QueryServiceStatus(service, &mut status) };
    if ok == 0 {
        return Err(anyhow!("QueryServiceStatus failed"));
    }
    Ok(status.current_state)
}

pub fn service_state(name: &str) -> anyhow::Result<u32> {
    let scm = open_scm()?;
    let service = open_service(scm, name, SERVICE_QUERY_STATUS)?;
    let state = query_state(service);
    unsafe {
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
    }
    state
}

pub fn service_state_name(state: u32) -> &'static str {
    match state {
        STATE_STOPPED => "Stopped",
        STATE_RUNNING => "Running",
        2 => "StartPending",
        3 => "StopPending",
        5 => "ContinuePending",
        6 => "PausePending",
        7 => "Paused",
        _ => "Unknown",
    }
}

fn wait_for_state(service: ScHandle, wanted: u32, attempts: u32) -> anyhow::Result<bool> {
    for _ in 0..attempts {
        if query_state(service)? == wanted {
            return Ok(true);
        }
        sleep(Duration::from_millis(500));
    }
    Ok(query_state(service)? == wanted)
}

pub fn stop_service(name: &str) -> anyhow::Result<()> {
    let scm = open_scm()?;
    let service = open_service(
        scm,
        name,
        SERVICE_QUERY_STATUS | SERVICE_START | SERVICE_STOP,
    )
    .context("open service for stop")?;
    let result = (|| {
        let state = query_state(service)?;
        if state == STATE_STOPPED {
            return Ok(());
        }
        let mut status = ServiceStatus::default();
        let ok = unsafe { ControlService(service, SERVICE_CONTROL_STOP, &mut status) };
        if ok == 0 {
            // It may already be stopping; still wait.
        }
        if !wait_for_state(service, STATE_STOPPED, 30)? {
            return Err(anyhow!("service did not reach Stopped: {name}"));
        }
        Ok(())
    })();
    unsafe {
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
    }
    result
}

pub fn start_service(name: &str) -> anyhow::Result<()> {
    let scm = open_scm()?;
    let service = open_service(
        scm,
        name,
        SERVICE_QUERY_STATUS | SERVICE_START | SERVICE_STOP,
    )
    .context("open service for start")?;
    let result = (|| {
        let state = query_state(service)?;
        if state == STATE_RUNNING {
            return Ok(());
        }
        let ok = unsafe { StartServiceW(service, 0, std::ptr::null()) };
        if ok == 0 {
            let state = query_state(service)?;
            if state != STATE_RUNNING && state != 2 {
                return Err(anyhow!("StartServiceW failed: {name}"));
            }
        }
        if !wait_for_state(service, STATE_RUNNING, 30)? {
            return Err(anyhow!("service did not reach Running: {name}"));
        }
        Ok(())
    })();
    unsafe {
        CloseServiceHandle(service);
        CloseServiceHandle(scm);
    }
    result
}

fn path_is_under(file: &Path, dir: &Path) -> bool {
    let file = file
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    let dir = dir
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    file.starts_with(&(dir + "\\"))
}

pub fn kill_installed(dir: &Path, names: &[&str]) -> anyhow::Result<usize> {
    let mut sys = System::new_all()?;
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut killed = 0usize;
    for process in sys.processes().values() {
        let Some(exe) = process.exe() else { continue };
        let fname = exe
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&fname)) {
            continue;
        }
        if path_is_under(exe, dir) {
            if process.kill() {
                killed += 1;
            }
        }
    }
    if killed > 0 {
        sleep(Duration::from_millis(800));
    }
    Ok(killed)
}
