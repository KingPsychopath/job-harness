//! Windows lifetime guard for the official ACP process transport.
//!
//! The SDK kills its direct child on Windows. That child is this short-lived
//! launcher, which joins a non-inheritable kill-on-close Job Object before it
//! creates the real agent. Agent and MCP descendants inherit job membership;
//! there is no spawn-then-assign race and no breakaway permission.
//! https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::Path,
    process::{Command, Stdio},
};
use windows_sys::Win32::System::{
    JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    },
    Threading::GetCurrentProcess,
};

/// Internal launcher only: inherits ACP stdio and exits with the real agent's status.
/// Its owning Job handle is deliberately closed by Windows during process exit.
#[doc(hidden)]
pub fn launch(agent: &Path, arguments: &[String]) -> anyhow::Result<()> {
    anyhow::ensure!(
        agent.is_absolute() && agent.is_file(),
        "ACP needs an absolute existing agent executable."
    );
    // SAFETY: null attributes/name create a uniquely owned, unnamed, non-inheritable handle.
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: CreateJobObjectW returned a valid unique kernel handle, now owned by this scope.
    let job = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: pointer and length name a live correctly sized Job Object information struct.
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: owned job handle and documented current-process pseudo handle are valid.
    // Failure happens before an agent is launched. Windows 8+ supports nested jobs.
    if unsafe { AssignProcessToJobObject(job.as_raw_handle(), GetCurrentProcess()) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let status = Command::new(agent)
        .args(arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    let code = match status {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => {
            eprintln!("ACP agent could not start or finish; inspect the configured executable.");
            1
        }
    };
    // Do not drop the Job handle while this process is still running: doing so
    // would terminate the launcher before it could preserve the agent exit code.
    // ExitProcess closes this sole job handle and terminates all remaining descendants.
    std::process::exit(code)
}
