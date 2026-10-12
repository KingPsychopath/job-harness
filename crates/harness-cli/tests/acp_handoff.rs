//! A separate real process peer checks ACP handoff and nonapproval end to end.
#[allow(dead_code)]
#[path = "../../../examples/agent/acp-fixture.rs"]
mod fixture;

#[cfg(unix)]
fn process_running(pid: &str) -> bool {
    let out = std::process::Command::new("ps")
        .args(["-p", pid.trim(), "-o", "stat="])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&out.stdout);
    out.status.success() && !state.trim().is_empty() && !state.trim().starts_with('Z')
}
#[cfg(windows)]
fn process_running(pid: &str) -> bool {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::WAIT_TIMEOUT,
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    // SAFETY: the synthetic peer's own PID is parsed, and only wait/query access is requested.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid.trim().parse().unwrap()) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        assert_eq!(
            error.raw_os_error(),
            Some(87),
            "Unexpected error querying the owned process: {error}"
        );
        return false;
    }
    // SAFETY: valid uniquely owned process handle; zero timeout is a nonblocking lifetime check.
    let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
    unsafe { WaitForSingleObject(owned.as_raw_handle(), 0) == WAIT_TIMEOUT }
}
fn cleanup_owned_process(pid: &str) {
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-KILL", pid.trim()])
        .status();
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
        };
        // SAFETY: this is only a failure cleanup for the PID written by our synthetic subprocess.
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid.trim().parse().unwrap());
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "--lingering-child") {
        let directory = std::env::args().next_back().unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join("child.pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(100));
        return;
    }
    if std::env::args().any(|arg| arg == "--timeout-peer") {
        fixture::run_fixture();
        return;
    }
    if std::env::args().any(|arg| arg == "--synthetic-peer") {
        fixture::run_fixture();
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let temp = tempfile::tempdir().unwrap();
        job_harness_core::workspace::initialize(temp.path()).unwrap();
        let executable = std::path::Path::new(env!("CARGO_BIN_EXE_job-harness"));
        let peer = std::env::current_exe().unwrap();
        let result = job_harness_protocol::acp::run(
            temp.path(),
            executable,
            &peer,
            vec!["--synthetic-peer".into()],
            "Inspect synthetic workspace only.".into(),
            std::time::Duration::from_secs(10),
            false,
        )
        .await
        .unwrap();
        assert_eq!(result["stop_reason"], "end_turn");
        assert_eq!(result["permission_policy"], "cancel_requests");
        assert!(
            result["text"]
                .as_str()
                .unwrap()
                .contains("cancelled permission verified")
        );
        {
            let result = job_harness_protocol::acp::run(
                temp.path(),
                executable,
                &peer,
                vec![
                    "--timeout-peer".into(),
                    temp.path().to_string_lossy().into_owned(),
                ],
                "Inspect synthetic state only".into(),
                std::time::Duration::from_secs(if cfg!(windows) { 5 } else { 2 }),
                false,
            )
            .await;
            assert!(result.is_err());
            for name in ["peer.pid", "child.pid"] {
                let pid = std::fs::read_to_string(temp.path().join(name)).unwrap();
                let running = || process_running(&pid);
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                while running() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                let survived = running();
                if survived {
                    cleanup_owned_process(&pid);
                }
                assert!(
                    !survived,
                    "The owned ACP process tree member survived the timeout"
                );
            }
        }
    });
    println!(
        "ACP subprocess handoff, restricted MCP, cancelled permission and process-tree timeout cleanup: passed"
    );
}
