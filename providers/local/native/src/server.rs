//! Serial loopback gateway. Native inference runs in a disposable process with bounded JSON IPC.
use crate::runtime::Runtime;
use serde_json::{Value, json};
#[cfg(unix)]
use std::fs::File;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
    },
    time::{Duration, Instant},
};
const BODY: usize = 1024 * 1024;
const IPC: u64 = 4 * 1024 * 1024;
pub struct Worker {
    child: Arc<Mutex<Child>>,
    input: ChildStdin,
    output: Receiver<Result<Value, String>>,
    deadline: Duration,
    pub startup: Value,
    _cwd: tempfile::TempDir,
    dead: Arc<AtomicBool>,
}
fn frame(reader: &mut impl BufRead) -> Result<Value, String> {
    let mut bytes = Vec::new();
    reader
        .take(IPC + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "runtime_unavailable: worker IPC failed")?;
    if bytes.is_empty() || bytes.len() as u64 > IPC || bytes.last() != Some(&b'\n') {
        return Err("runtime_unavailable: worker IPC closed or oversized".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "runtime_unavailable: worker JSON invalid".into())
}
fn send(writer: &mut impl Write, v: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(v).map_err(|_| "provider_schema_error: invalid JSON")?;
    if bytes.len() as u64 > IPC {
        return Err("context_budget: IPC response bound exceeded".into());
    }
    writer
        .write_all(&bytes)
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|_| "runtime_unavailable: worker IPC failed".into())
}
fn kill_child(child: &mut Child) {
    // The leader is never reaped before this call; its PID cannot be reused meanwhile.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}
impl Worker {
    pub fn start(
        paths: HashMap<String, PathBuf>,
        threads: i32,
        deadline: Duration,
    ) -> Result<Self, String> {
        let cwd = tempfile::tempdir()
            .map_err(|_| "runtime_start_failed: cannot create temporary working directory")?;
        let executable =
            std::env::current_exe().map_err(|_| "runtime_start_failed: executable unavailable")?;
        let mut command = Command::new(executable);
        command
            .arg("--worker")
            .env_clear()
            .current_dir(cwd.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        for name in [
            "OMP_NUM_THREADS",
            "OPENBLAS_NUM_THREADS",
            "VECLIB_MAXIMUM_THREADS",
            "MKL_NUM_THREADS",
        ] {
            command.env(name, threads.to_string());
        }
        command
            .env("TOKENIZERS_PARALLELISM", "false")
            .env("TMPDIR", cwd.path())
            .env("TEMP", cwd.path())
            .env("TMP", cwd.path());
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        let mut child = command
            .spawn()
            .map_err(|_| "runtime_start_failed: cannot spawn native worker")?;
        let mut input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let value = frame(&mut reader);
                let end = value.is_err();
                if tx.send(value).is_err() || end {
                    break;
                }
            }
        });
        if let Err(error) = send(&mut input, &json!({"paths":paths,"threads":threads})) {
            kill_child(&mut child);
            return Err(error);
        }
        let startup = match rx.recv_timeout(deadline) {
            Ok(Ok(v)) if v["ok"] == true => v,
            _ => {
                kill_child(&mut child);
                return Err("runtime_start_failed: bounded native worker failed; verify artifacts and library platform".into());
            }
        };
        let child = Arc::new(Mutex::new(child));
        let dead = Arc::new(AtomicBool::new(false));
        let cleanup = Arc::clone(&child);
        let cleanup_dead = Arc::clone(&dead);
        if ctrlc::set_handler(move || {
            if !cleanup_dead.swap(true, Ordering::SeqCst)
                && let Ok(mut child) = cleanup.lock()
            {
                kill_child(&mut child)
            }
            std::process::exit(0);
        })
        .is_err()
        {
            dead.store(true, Ordering::SeqCst);
            kill_child(&mut child.lock().unwrap_or_else(|e| e.into_inner()));
            return Err("runtime_start_failed: cannot register native worker cleanup".into());
        }
        Ok(Self {
            child,
            input,
            output: rx,
            deadline,
            startup,
            _cwd: cwd,
            dead,
        })
    }
    pub fn call(&mut self, route: &str, payload: Value) -> Result<Value, String> {
        if self.dead.load(Ordering::SeqCst) {
            return Err("runtime_unavailable: worker stopped; restart serve explicitly".into());
        }
        if send(&mut self.input, &json!([route, payload])).is_err() {
            self.stop();
            return Err("runtime_unavailable: worker IPC failed; restart serve explicitly".into());
        }
        match self.output.recv_timeout(self.deadline) {
            Ok(Ok(value)) if value["ok"] == true => Ok(value["data"].clone()),
            Ok(Ok(value)) => {
                let error = value["error"]
                    .as_str()
                    .unwrap_or("model_inference_failed: native failure")
                    .to_owned();
                if error.starts_with("runtime_unavailable:") {
                    self.stop();
                }
                Err(error)
            }
            Err(RecvTimeoutError::Timeout) => {
                self.stop();
                Err("runtime_timeout: native worker terminated; restart serve explicitly".into())
            }
            _ => {
                self.stop();
                Err("runtime_unavailable: native worker exited; restart serve explicitly".into())
            }
        }
    }
    fn stop(&mut self) {
        if self.dead.swap(true, Ordering::SeqCst) {
            return;
        }
        kill_child(&mut self.child.lock().unwrap_or_else(|e| e.into_inner()));
    }
    pub fn alive(&mut self) -> bool {
        if self.dead.load(Ordering::SeqCst) {
            return false;
        }
        // EOF detects death without reaping. The leader's PID stays reserved until group cleanup.
        match self.output.try_recv() {
            Err(TryRecvError::Empty) => true,
            _ => {
                self.stop();
                false
            }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}
pub fn worker_main() -> Result<(), String> {
    #[cfg(unix)]
    let mut output = {
        use std::os::fd::FromRawFd;
        let reserved = unsafe { libc::dup(1) };
        if reserved < 0 {
            return Err("runtime_start_failed: IPC reservation failed".into());
        }
        if unsafe { libc::fcntl(reserved, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            unsafe {
                libc::close(reserved);
            }
            return Err("runtime_start_failed: private IPC descriptor setup failed".into());
        }
        let sink = File::options()
            .write(true)
            .open("/dev/null")
            .map_err(|_| "runtime_start_failed: diagnostic suppression failed")?;
        use std::os::fd::AsRawFd;
        if unsafe { libc::dup2(sink.as_raw_fd(), 1) } < 0
            || unsafe { libc::dup2(sink.as_raw_fd(), 2) } < 0
        {
            return Err("runtime_start_failed: diagnostic suppression failed".into());
        }
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
            return Err("runtime_start_failed: core dump limit failed".into());
        }
        unsafe { File::from_raw_fd(reserved) }
    };
    #[cfg(not(unix))]
    let mut output = std::io::stdout();
    #[cfg(target_os = "linux")]
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0 {
        return Err("runtime_start_failed: worker hardening failed".into());
    }
    let mut reader = BufReader::new(std::io::stdin());
    let config = frame(&mut reader)?;
    let paths: HashMap<String, PathBuf> = serde_json::from_value(config["paths"].clone())
        .map_err(|_| "runtime_start_failed: invalid worker configuration")?;
    let threads = config["threads"]
        .as_i64()
        .filter(|n| (1..=16).contains(n))
        .ok_or("runtime_start_failed: thread bound invalid")? as i32;
    let mut runtime = Runtime::load(&paths, threads)?;
    send(
        &mut output,
        &json!({"ok":true,"load_ms":runtime.load_ms,"worker_peak_rss_bytes":crate::runtime::peak_rss(),"native":true,"python_runtime":false,"core_dump_limit_bytes":if cfg!(unix){Some(0)}else{None},"host_credentials_inherited":std::env::vars_os().any(|(k,_)|{let k=k.to_string_lossy();k.ends_with("_KEY")||k.ends_with("_TOKEN")||k.starts_with("AWS_")}),"isolation":"fresh environment, temporary working directory, bounded JSON IPC; native process retains same UID filesystem and network rights"}),
    )?;
    loop {
        let command = match frame(&mut reader) {
            Ok(v) => v,
            Err(_) => break,
        };
        if command.is_null() {
            break;
        }
        let result = if let Some(pair) = command.as_array().filter(|p| p.len() == 2) {
            runtime.call(pair[0].as_str().unwrap_or(""), pair[1].clone())
        } else {
            Err("invalid_arguments: worker command invalid".into())
        };
        let response = match result {
            Ok(data) => json!({"ok":true,"data":data}),
            Err(error) => json!({"ok":false,"error":error}),
        };
        send(&mut output, &response)?;
    }
    Ok(())
}
fn response(stream: &mut TcpStream, status: u16, data: Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(&data).map_err(|_| "gateway_failed: invalid response")?;
    let reason = match status {
        200 => "OK",
        400 => "BadRequest",
        403 => "Forbidden",
        404 => "NotFound",
        413 => "PayloadTooLarge",
        422 => "UnprocessableEntity",
        503 => "Unavailable",
        504 => "Timeout",
        _ => "Error",
    };
    write!(stream,"HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",bytes.len()).and_then(|_|stream.write_all(&bytes)).map_err(|_|"gateway_failed: response closed".into())
}
fn error(stream: &mut TcpStream, status: u16, code: &str) -> Result<(), String> {
    response(
        stream,
        status,
        json!({"error":{"code":code.split(':').next().unwrap_or("invalid_arguments"),"message":code}}),
    )
}
pub fn serve(mut worker: Worker, port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|_| "gateway_failed: loopback port not available")?;
    let actual = listener
        .local_addr()
        .map_err(|_| "gateway_failed: port unavailable")?
        .port();
    println!(
        "{}",
        json!({"listening":format!("http://127.0.0.1:{actual}"),"automatic_downloads":false,"native":true,"worker":worker.startup})
    );
    for connection in listener.incoming() {
        let Ok(mut stream) = connection else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let result = handle(&mut stream, actual, &mut worker);
        if result.is_err() {
            let _ = error(
                &mut stream,
                400,
                "invalid_arguments: malformed bounded HTTP request",
            );
        }
    }
    Ok(())
}
trait Backend {
    fn execute(&mut self, route: &str, payload: Value) -> Result<Value, String>;
    fn available(&mut self) -> bool;
    fn metadata(&self) -> &Value;
}
impl Backend for Worker {
    fn execute(&mut self, route: &str, payload: Value) -> Result<Value, String> {
        self.call(route, payload)
    }
    fn available(&mut self) -> bool {
        self.alive()
    }
    fn metadata(&self) -> &Value {
        &self.startup
    }
}
fn handle(stream: &mut TcpStream, port: u16, worker: &mut impl Backend) -> Result<(), String> {
    let expires = Instant::now() + Duration::from_secs(5);
    let mut header = Vec::new();
    let mut byte = [0];
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 16384 {
            return error(stream, 413, "context_budget: HTTP header bound exceeded");
        }
        stream
            .set_read_timeout(Some(
                expires
                    .checked_duration_since(Instant::now())
                    .ok_or("invalid_arguments: absolute HTTP read deadline exceeded")?,
            ))
            .map_err(|_| "invalid_arguments: HTTP timeout setup failed")?;
        if stream
            .read(&mut byte)
            .map_err(|_| "invalid_arguments: HTTP header read failed")?
            != 1
        {
            return Err("invalid_arguments: HTTP header closed".into());
        }
        header.push(byte[0]);
    }
    let header =
        std::str::from_utf8(&header).map_err(|_| "invalid_arguments: HTTP header UTF-8 invalid")?;
    let mut lines = header.split("\r\n");
    let first = lines
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>();
    if first.len() != 3 || first[2] != "HTTP/1.1" {
        return error(stream, 400, "invalid_arguments: HTTP request line invalid");
    }
    let mut headers = HashMap::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let (k, v) = line
            .split_once(':')
            .ok_or("invalid_arguments: HTTP header invalid")?;
        let key = k.trim().to_ascii_lowercase();
        if headers.insert(key, v.trim().to_owned()).is_some() {
            return error(stream, 400, "invalid_arguments: duplicate HTTP header");
        }
    }
    if headers.get("host") != Some(&format!("127.0.0.1:{port}")) || headers.contains_key("origin") {
        return error(
            stream,
            403,
            "forbidden_origin: exact loopback Host and no Origin required",
        );
    }
    if first[0] == "GET" && first[1] == "/health" {
        let alive = worker.available();
        return response(
            stream,
            if alive { 200 } else { 503 },
            json!({"ok":alive,"native":true,"python_runtime":false,"worker":worker.metadata()}),
        );
    }
    if first[0] != "POST"
        || !["/v1/embeddings", "/v1/decisions", "/v1/chat/completions"].contains(&first[1])
    {
        return error(stream, 404, "unsupported_route: typed provider route only");
    }
    if headers.contains_key("transfer-encoding")
        || !headers
            .get("content-type")
            .is_some_and(|s| s == "application/json")
    {
        return error(
            stream,
            400,
            "invalid_arguments: bounded application/json required",
        );
    }
    let size = headers
        .get("content-length")
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or("invalid_arguments: Content-Length required")?;
    if size > BODY {
        return error(stream, 413, "context_budget: body exceeds 1 MiB");
    }
    let mut body = vec![0; size];
    let mut consumed = 0;
    while consumed < size {
        stream
            .set_read_timeout(Some(
                expires
                    .checked_duration_since(Instant::now())
                    .ok_or("invalid_arguments: absolute HTTP read deadline exceeded")?,
            ))
            .map_err(|_| "invalid_arguments: HTTP timeout setup failed")?;
        let n = stream
            .read(&mut body[consumed..])
            .map_err(|_| "invalid_arguments: HTTP body closed or deadline exceeded")?;
        if n == 0 {
            return Err("invalid_arguments: HTTP body closed".into());
        }
        consumed += n;
    }
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return error(
                stream,
                400,
                "invalid_arguments: bounded UTF-8 JSON required",
            );
        }
    };
    let validation = match first[1] {
        "/v1/embeddings" => crate::runtime::embeddings(&payload).map(|_| ()),
        "/v1/decisions" => crate::runtime::decisions(&payload).map(|_| ()),
        _ => crate::runtime::triage_text(&payload).map(|_| ()),
    };
    if let Err(e) = validation {
        return error(stream, 422, &e);
    }
    match worker.execute(first[1], payload) {
        Ok(value) => response(stream, 200, value),
        Err(e) => error(
            stream,
            if e.starts_with("runtime_timeout") {
                504
            } else if e.starts_with("runtime_unavailable") {
                503
            } else {
                422
            },
            &e,
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        calls: usize,
        metadata: Value,
    }
    impl Backend for Fake {
        fn execute(&mut self, _: &str, _: Value) -> Result<Value, String> {
            self.calls += 1;
            Ok(json!({"synthetic":true}))
        }
        fn available(&mut self) -> bool {
            true
        }
        fn metadata(&self) -> &Value {
            &self.metadata
        }
    }
    fn exchange(build: impl FnOnce(u16) -> String) -> (String, usize) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let request = build(port);
        let thread = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            let mut bytes = String::new();
            stream.read_to_string(&mut bytes).unwrap();
            bytes
        });
        let (mut stream, _) = listener.accept().unwrap();
        let mut fake = Fake {
            calls: 0,
            metadata: json!({"fixture":true}),
        };
        let result = handle(&mut stream, port, &mut fake);
        if result.is_err() {
            let _ = error(
                &mut stream,
                400,
                "invalid_arguments: malformed bounded HTTP request",
            );
        }
        drop(stream);
        (thread.join().unwrap(), fake.calls)
    }
    #[test]
    fn host_origin_and_route_rejected() {
        for headers in [
            "Host: attacker.test\r\n".to_owned(),
            "Origin: http://example.test\r\n".to_owned(),
        ] {
            let (reply, calls) = exchange(|port| {
                format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{headers}\r\n")
            });
            assert!(reply.starts_with("HTTP/1.1 400") || reply.starts_with("HTTP/1.1 403"));
            assert_eq!(calls, 0);
        }
        let (reply, calls) =
            exchange(|port| format!("GET /generate HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"));
        assert!(reply.starts_with("HTTP/1.1 404"));
        assert_eq!(calls, 0);
    }
    #[test]
    fn bounded_json_and_validated_requests() {
        for body in [
            "{",
            "{\"model\":\"laya-s256\",\"state\":\"fixture\",\"questions\":{}}",
        ] {
            let (reply, calls) = exchange(|port| {
                format!(
                    "POST /v1/decisions HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
            });
            assert!(reply.starts_with("HTTP/1.1 400") || reply.starts_with("HTTP/1.1 422"));
            assert_eq!(calls, 0);
        }
        let body = r#"{"model":"laya-s256","state":"fixture","questions":{"action":{"type":"boolean","condition":"Action requested?"}}}"#;
        let (reply, calls) = exchange(|port| {
            format!(
                "POST /v1/decisions HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
        });
        assert!(reply.starts_with("HTTP/1.1 200"));
        assert_eq!(calls, 1);
    }
    #[test]
    fn oversized_request_rejected_before_worker() {
        let (reply, calls) = exchange(|port| {
            format!(
                "POST /v1/decisions HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 1048577\r\n\r\n"
            )
        });
        assert!(reply.starts_with("HTTP/1.1 413"));
        assert_eq!(calls, 0);
    }
    #[test]
    fn frame_rejects_trailing_or_oversized_protocol() {
        assert!(frame(&mut std::io::Cursor::new(b"{\"ok\":true}")).is_err());
        assert!(frame(&mut std::io::Cursor::new(vec![b'x'; 4 * 1024 * 1024 + 2])).is_err());
    }
}
#[cfg(test)]
mod worker_failure_tests {
    use super::*;
    #[test]
    #[ignore = "subprocess helper"]
    fn child_hold() {
        std::thread::sleep(Duration::from_secs(30));
    }
    fn held_worker() -> (Worker, mpsc::Sender<Result<Value, String>>) {
        let cwd = tempfile::tempdir().unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "server::worker_failure_tests::child_hold",
                "--ignored",
                "--nocapture",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let (tx, rx) = mpsc::channel();
        (
            Worker {
                child: Arc::new(Mutex::new(child)),
                input,
                output: rx,
                deadline: Duration::from_millis(20),
                startup: json!({"fixture":true}),
                _cwd: cwd,
                dead: Arc::new(AtomicBool::new(false)),
            },
            tx,
        )
    }
    #[test]
    fn timeout_kills_native_child_once_and_never_respawns() {
        let (mut worker, _tx) = held_worker();
        let pid = worker.child.lock().unwrap().id();
        assert!(
            worker
                .call("fixture", json!({}))
                .unwrap_err()
                .starts_with("runtime_timeout")
        );
        assert!(!worker.alive());
        assert!(worker.child.lock().unwrap().try_wait().unwrap().is_some());
        assert!(
            worker
                .call("fixture", json!({}))
                .unwrap_err()
                .starts_with("runtime_unavailable")
        );
        assert_eq!(worker.child.lock().unwrap().id(), pid);
        worker.stop();
    }
    #[test]
    fn eof_health_kills_group_before_reaping() {
        let (mut worker, tx) = held_worker();
        tx.send(Err("runtime_unavailable: fixture EOF".into()))
            .unwrap();
        assert!(!worker.alive());
        assert!(worker.dead.load(Ordering::SeqCst));
        assert!(worker.child.lock().unwrap().try_wait().unwrap().is_some());
    }
    #[test]
    fn scorer_death_is_fatal_to_worker_and_health() {
        let (mut worker, tx) = held_worker();
        tx.send(Ok(
            json!({"ok":false,"error":"runtime_unavailable: native Kev stopped"}),
        ))
        .unwrap();
        assert!(
            worker
                .call("fixture", json!({}))
                .unwrap_err()
                .starts_with("runtime_unavailable")
        );
        assert!(!worker.alive());
    }
}
