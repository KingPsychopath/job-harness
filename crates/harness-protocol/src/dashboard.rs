//! Loopback workspace views and explicitly enabled, named human review routes.
mod changes;
mod readiness;
use axum::{
    Json, Router,
    extract::{Path as UrlPath, Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use harness_core::Engine;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use subtle::ConstantTimeEq;

const INDEX: &str = include_str!("../../../gui/index.html");
const ICON: &str = include_str!("../../../docs/assets/icon.svg");
const CSS: &str = include_str!("../../../gui/app.css");
const JS: &str = include_str!("../../../gui/app.js");
const CONNECTIONS_JS: &str = include_str!("../../../gui/connections.js");
const COOKIE: &str = "job_harness_session";

#[derive(Clone)]
pub struct DashboardState {
    engine: Arc<Engine>,
    authority: String,
    origin: String,
    session: String,
    csrf: String,
    local_writes: Arc<AtomicBool>,
    native_controls: bool,
    cookie_name: String,
    launch_token: Arc<Mutex<Option<String>>>,
    changes: Arc<changes::Changes>,
}

impl DashboardState {
    pub fn new(root: &Path, port: u16) -> anyhow::Result<Self> {
        Self::with_policy(root, port, false)
    }
    pub fn with_policy(root: &Path, port: u16, local_writes: bool) -> anyhow::Result<Self> {
        let authority = format!("127.0.0.1:{port}");
        let engine = Arc::new(Engine::open(root)?);
        let changes = Arc::new(changes::Changes::new(engine.root()));
        Ok(Self {
            engine,
            changes,
            origin: format!("http://{authority}"),
            authority,
            session: uuid::Uuid::new_v4().simple().to_string(),
            csrf: uuid::Uuid::new_v4().simple().to_string(),
            local_writes: Arc::new(AtomicBool::new(local_writes)),
            native_controls: false,
            cookie_name: format!("{COOKIE}_{port}"),
            launch_token: Arc::new(Mutex::new(Some(format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            )))),
        })
    }
    /// Native host-owned switch. No HTTP or frontend IPC changes this policy.
    pub fn with_native_policy(root: &Path, port: u16, local_writes: bool) -> anyhow::Result<Self> {
        let mut state = Self::with_policy(root, port, local_writes)?;
        state.native_controls = true;
        Ok(state)
    }
    pub fn set_native_review_mode(&self, enabled: bool) -> anyhow::Result<()> {
        if !self.native_controls {
            anyhow::bail!("This host fixes review policy at launch");
        }
        self.local_writes.store(enabled, Ordering::Release);
        self.changes.bump();
        Ok(())
    }
    fn review_enabled(&self) -> bool {
        self.local_writes.load(Ordering::Acquire)
    }
    /// Host shutdown closes streams before the HTTP server drains connections.
    pub fn close_notifications(&self) {
        self.changes.close();
    }
    /// One-use browser bootstrap capability. Never log, persist, or return it in tool output.
    pub fn launch_url(&self) -> anyhow::Result<String> {
        let token = self
            .launch_token
            .lock()
            .map_err(|_| anyhow::anyhow!("Local launch unavailable"))?;
        let token = token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Local launch already used"))?;
        Ok(format!("{}/launch/{token}", self.origin))
    }
}

fn same_origin(headers: &HeaderMap, state: &DashboardState) -> bool {
    headers.get("host").and_then(|h| h.to_str().ok()) == Some(state.authority.as_str())
        && headers
            .get("origin")
            .map(|h| h.to_str().ok() == Some(state.origin.as_str()))
            .unwrap_or(true)
        && headers
            .get("sec-fetch-site")
            .map(|h| matches!(h.to_str().ok(), Some("same-origin" | "none")))
            .unwrap_or(true)
}

fn has_session(headers: &HeaderMap, state: &DashboardState) -> bool {
    headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .map(|cookies| {
            cookies
                .split(';')
                .filter_map(|cookie| cookie.trim().split_once('='))
                .any(|(name, value)| {
                    name == state.cookie_name
                        && bool::from(value.as_bytes().ct_eq(state.session.as_bytes()))
                })
        })
        .unwrap_or(false)
}

async fn guard(
    State(state): State<DashboardState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if request.method() != Method::GET
        && !(request.method() == Method::POST && state.review_enabled())
    {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if !same_origin(request.headers(), &state) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if (request.uri().path() == "/" || request.uri().path().starts_with("/api/"))
        && !has_session(request.headers(), &state)
    {
        return if request.uri().path() == "/" {
            session_ended()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        };
    }
    if request.uri().path().starts_with("/api/")
        && !request
            .headers()
            .get("x-job-harness-csrf")
            .is_some_and(|v| bool::from(v.as_bytes().ct_eq(state.csrf.as_bytes())))
    {
        return if request.method() == Method::POST {
            StatusCode::FORBIDDEN.into_response()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        };
    }
    if request.method() == Method::POST
        && (request
            .headers()
            .get("origin")
            .and_then(|h| h.to_str().ok())
            != Some(state.origin.as_str())
            || !request
                .headers()
                .get("x-job-harness-csrf")
                .is_some_and(|v| bool::from(v.as_bytes().ct_eq(state.csrf.as_bytes()))))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("content-security-policy", HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"));
    response
}

async fn home(State(state): State<DashboardState>) -> Response {
    render_page(&state, "")
}
fn session_ended() -> Response {
    (StatusCode::UNAUTHORIZED, Html("<!doctype html><html><head><meta name=\"viewport\" content=\"width=device-width\"><title>Reopen Job Harness</title><link rel=\"stylesheet\" href=\"/app.css\"></head><body><main class=\"empty\"><h1>Reopen Job Harness</h1><p>This private session has ended. Close this window and reopen the desktop app or dashboard.</p></main></body></html>")).into_response()
}
fn render_page(state: &DashboardState, nonce: &str) -> Response {
    let html = INDEX.replace("{{CSRF}}", nonce).replace(
        "{{WRITE_MODE}}",
        if state.review_enabled() {
            "local_writes"
        } else {
            "read_only"
        },
    );
    Html(html).into_response()
}
async fn launch(
    State(state): State<DashboardState>,
    UrlPath(supplied): UrlPath<String>,
) -> Response {
    let Ok(mut token) = state.launch_token.lock() else {
        return session_ended();
    };
    if !token
        .as_ref()
        .is_some_and(|expected| bool::from(expected.as_bytes().ct_eq(supplied.as_bytes())))
    {
        return session_ended();
    }
    token.take();
    let mut response = render_page(&state, &state.csrf);
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict",
        state.cookie_name, state.session
    );
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert("set-cookie", value);
    }
    response
}

async fn icon() -> impl IntoResponse {
    ([("content-type", "image/svg+xml")], ICON)
}
async fn css() -> impl IntoResponse {
    ([("content-type", "text/css; charset=utf-8")], CSS)
}
async fn js() -> impl IntoResponse {
    ([("content-type", "text/javascript; charset=utf-8")], JS)
}
async fn connections_js() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        CONNECTIONS_JS,
    )
}

async fn overview(State(state): State<DashboardState>) -> Json<Value> {
    let engine = state.engine.clone();
    let value = tokio::task::spawn_blocking(move || engine.call("bootstrap", &json!({})))
        .await
        .unwrap_or_else(|_| unavailable());
    Json(value)
}

async fn pipeline(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    let mut args = json!({"offset":query.get("offset").and_then(|v|v.parse::<usize>().ok()).unwrap_or(0),"limit":50});
    if let Some(q) = query.get("q").filter(|q| !q.trim().is_empty()) {
        args["query"] = json!(q);
    }
    if let Some(stage) = query.get("stage").filter(|q| !q.is_empty()) {
        args["stage"] = json!(stage);
    }
    let engine = state.engine.clone();
    Json(
        tokio::task::spawn_blocking(move || engine.call("find_application", &args))
            .await
            .unwrap_or_else(|_| unavailable()),
    )
}

fn unavailable() -> Value {
    json!({"schema_version":2,"ok":false,"data":null,"error":{"code":"io_error","message":"Read stopped; inspect workspace availability.","retryable":true,"recovery":"Refresh after checking the workspace."},"next":[]})
}

async fn invoke(state: DashboardState, tool: &'static str, args: Value) -> Json<Value> {
    Json(
        tokio::task::spawn_blocking(move || state.engine.call(tool, &args))
            .await
            .unwrap_or_else(|_| unavailable()),
    )
}
async fn attention(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    invoke(state,"review_queue",json!({"offset":query.get("offset").and_then(|s|s.parse::<usize>().ok()).unwrap_or(0),"limit":50})).await
}
async fn search(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    invoke(
        state,
        "search_workspace",
        json!({"query":query.get("q").cloned().unwrap_or_default(),"limit":30}),
    )
    .await
}
async fn source(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    invoke(
        state,
        "read_record",
        json!({"path":query.get("path").cloned().unwrap_or_default()}),
    )
    .await
}
async fn notes(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    invoke(state,"list_notes",json!({"offset":query.get("offset").and_then(|s|s.parse::<usize>().ok()).unwrap_or(0),"limit":50,"include_archived":query.get("archived").is_some_and(|s|s=="true")})).await
}
async fn review(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    let mut args = json!({"note_id":query.get("note_id").cloned().unwrap_or_default(),"include_original":true});
    if let Some(id) = query.get("proposal_id") {
        args["proposal_id"] = json!(id);
    }
    invoke(state, "review_note", args).await
}
async fn packet(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    invoke(
        state,
        "preview_packet",
        json!({"packet_id":query.get("packet_id").cloned().unwrap_or_default()}),
    )
    .await
}
async fn company(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    let mut args = json!({"task":"Prepare a sourced company screening brief","limit":6});
    if let Some(name) = query.get("company").filter(|s| !s.trim().is_empty()) {
        args["company"] = json!(name);
    }
    if let Some(record) = query.get("record").filter(|s| !s.trim().is_empty()) {
        args["paths"] = json!([record]);
    }
    invoke(state, "context_pack", args).await
}
async fn connections(State(state): State<DashboardState>) -> Json<Value> {
    Json(
        json!({"schema_version":2,"ok":true,"data":{"mode":if state.review_enabled(){"local_writes"}else{"read_only"},"native_model_actions":state.native_controls&&state.review_enabled(),"version":env!("CARGO_PKG_VERSION"),"mcp":{"transport":"stdio","focused_tools":crate::FOCUSED_TOOLS.len(),"full_tools":harness_core::contracts::specs().len()},"acp":{"role":"client","external_actions_default":false},"models":crate::context_host::model_metadata(),"workspace":state.engine.root(),"capabilities":["local_records","notes_review","search","packet_preview"],"external_connectors":"Configured and authenticated in the agent host"},"error":null,"next":[]}),
    )
}
fn host_result(result: harness_core::Result<Value>) -> Json<Value> {
    Json(match result {
        Ok(data) => json!({"schema_version":2,"ok":true,"data":data,"error":null,"next":[]}),
        Err(error) => {
            json!({"schema_version":2,"ok":false,"data":null,"error":{"code":error.code,"message":error.message,"retryable":false,"recovery":"Review the current device state and explicit download plan."},"next":[]})
        }
    })
}
async fn device() -> Json<Value> {
    host_result(Ok(readiness::inspect().await))
}
async fn model_plan(Query(query): Query<HashMap<String, String>>) -> Json<Value> {
    if query.len() != 1 || !query.contains_key("id") {
        return host_result(Err(harness_core::Error::validation(
            "Choose one fixed model ID",
        )));
    }
    host_result(readiness::plan(&query["id"]).await)
}
fn model_arguments(args: &Value, installing: bool) -> harness_core::Result<(&str, &str)> {
    let Some(object) = args.as_object() else {
        return Err(harness_core::Error::validation(
            "Model action must be an object",
        ));
    };
    if object.len() != if installing { 2 } else { 1 }
        || object
            .keys()
            .any(|key| key != "id" && !(installing && key == "expected_plan_sha256"))
    {
        return Err(harness_core::Error::validation(
            "Model action accepts only a fixed ID and the reviewed plan identity",
        ));
    }
    let id = args["id"]
        .as_str()
        .ok_or_else(|| harness_core::Error::validation("Model ID required"))?;
    let hash = if installing {
        args["expected_plan_sha256"]
            .as_str()
            .filter(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| harness_core::Error::validation("Exact reviewed plan SHA256 required"))?
    } else {
        ""
    };
    Ok((id, hash))
}
async fn verify_model(Json(args): Json<Value>) -> Json<Value> {
    host_result(match model_arguments(&args, false) {
        Ok((id, _)) => readiness::verify(id).await,
        Err(error) => Err(error),
    })
}
async fn install_model(Json(args): Json<Value>) -> Json<Value> {
    host_result(match model_arguments(&args, true) {
        Ok((id, hash)) => readiness::install(id, hash).await,
        Err(error) => Err(error),
    })
}
async fn import_note(State(state): State<DashboardState>, Json(args): Json<Value>) -> Json<Value> {
    invoke(state, "intake_note", args).await
}
async fn propose_note(State(state): State<DashboardState>, Json(args): Json<Value>) -> Json<Value> {
    invoke(state, "propose_note", args).await
}
async fn accept_note(State(state): State<DashboardState>, Json(args): Json<Value>) -> Json<Value> {
    invoke(state, "accept_note", args).await
}
async fn archive_note(State(state): State<DashboardState>, Json(args): Json<Value>) -> Json<Value> {
    invoke(state, "archive_note", args).await
}
async fn tracking(State(state): State<DashboardState>, Json(args): Json<Value>) -> Json<Value> {
    let engine = state.engine.clone();
    Json(tokio::task::spawn_blocking(move || {
        let record = args["record"].as_str().unwrap_or_default();
        let key = args["idempotency_key"].as_str().unwrap_or_default();
        let reason = args["reason"].as_str().unwrap_or_default();
        let Some(fields) = args["tracking"].as_object() else { return crate::denied(); };
        if key.is_empty() || reason.trim().is_empty() || fields.keys().any(|k| !["stage","next_action","status_detail"].contains(&k.as_str())) || args.get("correction_reason").is_some() { return crate::denied(); }
    {
        let Some(_expected) = args["expected_record_sha256"]
            .as_str()
            .filter(|s| s.len() == 64)
        else {
            return crate::denied();
        };
        let stage = fields
            .get("stage")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(stage, "applied" | "prepared") {
            let rows = match harness_core::pipeline::build(engine.root()) {
                Ok(pipeline) => pipeline.records,
                Err(_) => return crate::denied(),
            };
            let row = rows.iter().find(|row| {
                row["record"]
                    .as_str()
                    .is_some_and(|name| format!("applications/{name}") == record)
            });
            let Some(row) = row else {
                return crate::denied();
            };
            if row["stage"] != stage {
                let duplicate = engine.call(
                    "check_duplicate",
                    &json!({"company":row["company"],"vacancy_url":row["vacancy_url"]}),
                );
                let supported = duplicate["data"]["blocking_attempts"]
                    .as_array()
                    .is_some_and(|attempts| {
                        attempts.iter().any(|a| {
                            a["record_path"] == record
                                && if stage == "applied" {
                                    a["status"] == "submitted"
                                } else {
                                    a["packet_id"]
                                        .as_str()
                                        .filter(|p| !p.is_empty())
                                        .is_some_and(|packet| {
                                            let preview = engine.call(
                                                "preview_packet",
                                                &json!({"packet_id":packet}),
                                            );
                                            preview["ok"] == true
                                                && preview["data"]["stale"] == false
                                        })
                                }
                        })
                    });
                if !supported {
                    return json!({"schema_version":2,"ok":false,"data":null,"error":{"code":"recorded_evidence_required","message":if stage=="applied"{"This move needs a recorded submission. Moving the board does not send an application."}else{"Prepare this application with your agent before marking its materials ready."},"retryable":false,"recovery":"Use the application workflow and verify its materials or submission first."},"next":[]});
                }
            }
        }
    }

        let run = engine.call("start_run", &json!({"operation":"reconcile","scope":{"record":record},"user_request":format!("Explicit GUI review: {reason}"),"idempotency_key":format!("{key}:start")}));
        if run["ok"] != true { return run; }
        let mut update = json!({"run_id":run["data"]["run_id"],"record":record,"tracking":fields,"summary":format!("Explicit local GUI review: {reason}"),"evidence":[record],"idempotency_key":format!("{key}:update")});
        update["expected_record_sha256"] = args["expected_record_sha256"].clone();
        let result = engine.call("update_application", &update);
        let ok = result["ok"] == true;
        let finish = engine.call("finish_run", &json!({"run_id":run["data"]["run_id"],"status":if ok {"completed"} else {"blocked"},"summary":if ok {"Explicit GUI tracking review recorded"} else {"GUI tracking review refused; canonical record was retained"},"evidence":[record],"idempotency_key":format!("{key}:finish")}));
        if ok && finish["ok"] != true { finish } else { result }
    }).await.unwrap_or_else(|_| unavailable()))
}

async fn activity(
    State(state): State<DashboardState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    let engine = state.engine.clone();
    let requested = query.get("offset").and_then(|s| s.parse::<usize>().ok());
    Json(
        tokio::task::spawn_blocking(move || {
            let head = engine.call("list_events", &json!({"limit":1}));
            if head["ok"] != true {
                return head;
            }
            let total = head["data"]["page"]["total"].as_u64().unwrap_or(0) as usize;
            let offset = requested.unwrap_or_else(|| total.saturating_sub(30));
            engine.call("list_events", &json!({"offset":offset,"limit":30}))
        })
        .await
        .unwrap_or_else(|_| unavailable()),
    )
}
fn change_snapshot(state: &DashboardState) -> Value {
    json!({"schema_version":2,"ok":true,"data":{"revision":state.changes.revision.borrow().to_string(),"mode":if state.review_enabled(){"local_writes"}else{"read_only"},"notifications":if state.changes.healthy.load(Ordering::Acquire){"watching"}else{"manual"}},"error":null,"next":[]})
}
async fn updates(State(state): State<DashboardState>) -> Json<Value> {
    // No coordinator reads, file scans, hashing or polling work.
    Json(change_snapshot(&state))
}
async fn change_stream(State(state): State<DashboardState>) -> Response {
    let receiver = state.changes.revision.subscribe();
    let closed = state.changes.closed.subscribe();
    let stream = futures_util::stream::unfold(
        (state, receiver, closed, true),
        |(state, mut receiver, mut closed, initial)| async move {
            if *closed.borrow() {
                return None;
            }
            if !initial {
                tokio::select! {
                    _=closed.changed()=>return None,
                    changed=receiver.changed()=>if changed.is_err() { return None; },
                }
                // Collapse atomic-file/journal bursts; the client then rereads the engine.
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            if *closed.borrow() {
                return None;
            }
            receiver.borrow_and_update();
            let line = format!("{}\n", change_snapshot(&state));
            Some((
                Ok::<_, std::convert::Infallible>(line),
                (state, receiver, closed, false),
            ))
        },
    );
    (
        [("content-type", "application/x-ndjson; charset=utf-8")],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

pub fn router(state: DashboardState) -> Router {
    let mut app = Router::new()
        .route("/", get(home))
        .route("/launch/{token}", get(launch))
        .route("/icon.svg", get(icon))
        .route("/app.css", get(css))
        .route("/app.js", get(js))
        .route("/connections.js", get(connections_js))
        .route("/api/overview", get(overview))
        .route("/api/pipeline", get(pipeline))
        .route("/api/attention", get(attention))
        .route("/api/search", get(search))
        .route("/api/source", get(source))
        .route("/api/notes", get(notes))
        .route("/api/notes/review", get(review))
        .route("/api/packet", get(packet))
        .route("/api/company", get(company))
        .route("/api/connections", get(connections))
        .route("/api/device", get(device))
        .route("/api/models/plan", get(model_plan))
        .route("/api/activity", get(activity))
        .route("/api/updates", get(updates))
        .route("/api/changes", get(change_stream));
    if state.native_controls || state.review_enabled() {
        app = app
            .route("/api/notes/import", post(import_note))
            .route("/api/notes/propose", post(propose_note))
            .route("/api/notes/accept", post(accept_note))
            .route("/api/notes/archive", post(archive_note))
            .route("/api/tracking", post(tracking));
    }
    // Only the native host exposes fixed, explicitly reviewed optional installs.
    // Read-only browser/MCP/model sessions gain no executable/download endpoint.
    if state.native_controls {
        app = app
            .route("/api/models/verify", post(verify_model))
            .route("/api/models/install", post(install_model));
    }
    app.layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

fn open_local_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    let status = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("Unable to open the local browser"))
    }
}
pub async fn serve(root: &Path, port: u16, local_writes: bool) -> anyhow::Result<()> {
    let listener =
        tokio::net::TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).await?;
    let actual_port = listener.local_addr()?.port();
    let state = DashboardState::with_policy(root, actual_port, local_writes)?;
    eprintln!(
        "Job Harness {} dashboard: http://127.0.0.1:{actual_port}",
        if local_writes {
            "review-enabled"
        } else {
            "read-only"
        }
    );
    eprintln!("Opening your browser for a private local session. Ctrl-C closes it.");
    let launch_url = state.launch_url()?;
    // The one-use capability travels directly to the OS browser; never to terminal logs.
    std::thread::spawn(move || {
        if open_local_browser(&launch_url).is_err() {
            eprintln!(
                "The browser could not open. Use the native desktop app or retry from a desktop session."
            );
        }
    });
    let stopping = state.clone();
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            stopping.close_notifications();
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    async fn reviewed_post(
        app: Router,
        cookie: &str,
        nonce: &str,
        route: &str,
        args: Value,
    ) -> Value {
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(route)
                    .header("host", "127.0.0.1:3210")
                    .header("origin", "http://127.0.0.1:3210")
                    .header("cookie", cookie)
                    .header("x-job-harness-csrf", nonce)
                    .header("content-type", "application/json")
                    .body(Body::from(args.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }
    #[tokio::test]
    async fn human_review_requires_selected_supported_items_and_the_current_proposal_hash() {
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        let state = DashboardState::with_policy(temp.path(), 3210, true).unwrap();
        let nonce = state.csrf.clone();
        let cookie = format!("{}={}", state.cookie_name, state.session);
        let app = router(state.clone());
        let original = reviewed_post(app.clone(), &cookie, &nonce, "/api/notes/import",
            json!({"text":"I built an internal dashboard.\n","source_kind":"personal_notes","idempotency_key":"review-source"})).await;
        assert_eq!(original["ok"], true);
        let note = &original["data"];
        let proposal = reviewed_post(app.clone(), &cookie, &nonce, "/api/notes/propose", json!({
            "note_id":note["note_id"],"expected_source_sha256":note["source_sha256"],
            "items":[{"id":"dashboard","kind":"summary","text":"Built an internal dashboard.",
            "citations":[{"path":note["original_path"],"sha256":note["source_sha256"],"start_line":1,"end_line":1}]}],
            "idempotency_key":"review-proposal"})).await;
        assert_eq!(proposal["ok"], true, "{proposal}");
        let review = state.engine.call(
            "review_note",
            &json!({"note_id":note["note_id"],"proposal_id":proposal["data"]["proposal_id"]}),
        );
        let mut approval = json!({"note_id":note["note_id"],"proposal_id":proposal["data"]["proposal_id"],
            "expected_proposal_sha256":review["data"]["proposals"][0]["sha256"],
            "selected_item_ids":[],"approved_by":"Synthetic reviewer","approval_evidence":"I checked the original source and selected only its supported summary.","idempotency_key":"review-accept"});
        let empty = reviewed_post(
            app.clone(),
            &cookie,
            &nonce,
            "/api/notes/accept",
            approval.clone(),
        )
        .await;
        assert_eq!(empty["ok"], false);
        approval["selected_item_ids"] = json!(["dashboard"]);
        let mut stale = approval.clone();
        stale["expected_proposal_sha256"] = json!("0".repeat(64));
        assert_eq!(
            reviewed_post(app.clone(), &cookie, &nonce, "/api/notes/accept", stale).await["ok"],
            false
        );
        assert_eq!(
            state
                .engine
                .call("review_note", &json!({"note_id":note["note_id"]}))["data"]["accepted_reviews"],
            json!([])
        );
        let accepted = reviewed_post(app, &cookie, &nonce, "/api/notes/accept", approval).await;
        assert_eq!(accepted["ok"], true, "{accepted}");
        let after = state.engine.call(
            "review_note",
            &json!({"note_id":note["note_id"],"include_original":true}),
        );
        assert_eq!(
            after["data"]["original"]["text"],
            "I built an internal dashboard.\n"
        );
        assert_eq!(
            after["data"]["accepted_reviews"].as_array().unwrap().len(),
            1
        );
    }
    #[tokio::test]
    async fn reviewed_tracking_records_intent_but_cannot_regress_a_submission_stage() {
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        let path = temp.path().join("applications/example.md");
        std::fs::write(&path, "# Example — Engineer\n\n<!-- tracking\n{\"stage\":\"applied\",\"company\":\"Example\",\"vacancy_url\":\"https://example.test/role\",\"checked\":\"2026-10-12\",\"next_action\":\"Wait for reply\"}\n-->\n").unwrap();
        let state = DashboardState::with_policy(temp.path(), 3210, true).unwrap();
        let nonce = state.csrf.clone();
        let cookie = format!("{}={}", state.cookie_name, state.session);
        let source = state
            .engine
            .call("read_record", &json!({"path":"applications/example.md"}));
        let expected = source["data"]["sha256"].clone();
        let app = router(state);
        let accepted = reviewed_post(app.clone(), &cookie, &nonce, "/api/tracking", json!({
            "record":"applications/example.md","tracking":{"stage":"screening","next_action":"Prepare for the screening call"},
            "reason":"Hiring team invited me to a screening call.","expected_record_sha256":expected,"idempotency_key":"review-tracking"})).await;
        assert_eq!(accepted["ok"], true, "{accepted}");
        let bytes = std::fs::read(&path).unwrap();
        let refused = reviewed_post(app.clone(), &cookie, &nonce, "/api/tracking", json!({
            "record":"applications/example.md","tracking":{"stage":"saved","next_action":"Apply again"},
            "reason":"Synthetic attempted regression","expected_record_sha256":expected,"idempotency_key":"review-regression"})).await;
        assert_eq!(refused["ok"], false, "{refused}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let correction = reviewed_post(app, &cookie, &nonce, "/api/tracking", json!({
            "record":"applications/example.md","tracking":{"stage":"saved"},"correction_reason":"Erase duplicate blocker",
            "reason":"Synthetic correction","idempotency_key":"review-correction"})).await;
        assert_eq!(correction["error"]["code"], "tool_denied");
    }
    #[tokio::test]
    async fn reviewed_posts_require_write_mode_session_origin_and_csrf_and_have_no_arbitrary_route()
    {
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        let state = DashboardState::with_policy(temp.path(), 3210, true).unwrap();
        let nonce = state.csrf.clone();
        let launch_path = state
            .launch_url()
            .unwrap()
            .strip_prefix("http://127.0.0.1:3210")
            .unwrap()
            .to_owned();
        let app = router(state);
        let home = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&launch_path)
                    .header("host", "127.0.0.1:3210")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = home.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let launch_html = String::from_utf8(
            home.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(launch_html.contains(&nonce));
        let home = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("host", "127.0.0.1:3210")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let html = String::from_utf8(
            home.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(!html.contains(&nonce));
        let body = json!({"text":"# Supplied notes\nUser provided an example.\n","source_kind":"personal_notes","idempotency_key":"gui-import-test"});
        let missing = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/notes/import")
                    .header("host", "127.0.0.1:3210")
                    .header("origin", "http://127.0.0.1:3210")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::FORBIDDEN);
        let cross = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/notes/import")
                    .header("host", "127.0.0.1:3210")
                    .header("origin", "https://example.test")
                    .header("cookie", &cookie)
                    .header("x-job-harness-csrf", &nonce)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cross.status(), StatusCode::FORBIDDEN);
        let accepted = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/notes/import")
                    .header("host", "127.0.0.1:3210")
                    .header("origin", "http://127.0.0.1:3210")
                    .header("cookie", &cookie)
                    .header("x-job-harness-csrf", &nonce)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);
        let result: Value =
            serde_json::from_slice(&accepted.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(result["ok"], true);
        let arbitrary = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/tool")
                    .header("host", "127.0.0.1:3210")
                    .header("origin", "http://127.0.0.1:3210")
                    .header("cookie", &cookie)
                    .header("x-job-harness-csrf", &nonce)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(arbitrary.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn board_moves_need_current_sources_and_cannot_invent_submission_or_materials() {
        // Phase-only diagnostics: no workspace paths, source text or session values.
        // A process deadline must bound a blocked spawn_blocking task; a Tokio
        // timer alone cannot stop it or prevent runtime shutdown from waiting.
        let phase = |name: &str| eprintln!("[board-move-test] {name}");
        phase("workspace.initialize.begin");
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        phase("workspace.initialize.complete");
        let path = temp.path().join("applications/example.md");
        std::fs::write(&path, "# Example — Engineer\n\n<!-- tracking\n{\"company\":\"Example\",\"stage\":\"saved\",\"checked\":\"2026-10-12T09:00:00Z\",\"vacancy_url\":\"https://example.com/job\",\"next_action\":\"Review role\"}\n-->\n").unwrap();
        phase("dashboard.create.begin");
        let state = DashboardState::with_policy(temp.path(), 3210, true).unwrap();
        phase("dashboard.create.complete");
        phase("source.read.begin");
        let source = state
            .engine
            .call("read_record", &json!({"path":"applications/example.md"}));
        phase("source.read.complete");
        let cookie = format!("{}={}", state.cookie_name, state.session);
        let app = router(state.clone());
        let request = json!({"record":"applications/example.md","tracking":{"stage":"applied","next_action":"Wait for response"},"board_move":true,"reason":"Dragged to a later stage","expected_record_sha256":source["data"]["sha256"],"idempotency_key":"board-applied"});
        phase("post.applied.begin");
        let denied = reviewed_post(
            app.clone(),
            &cookie,
            &state.csrf,
            "/api/tracking",
            request.clone(),
        )
        .await;
        phase("post.applied.complete");
        assert_eq!(denied["error"]["code"], "recorded_evidence_required");
        let mut prepared = request.clone();
        prepared["tracking"]["stage"] = json!("prepared");
        phase("post.prepared.begin");
        let prepared_denied =
            reviewed_post(app.clone(), &cookie, &state.csrf, "/api/tracking", prepared).await;
        phase("post.prepared.complete");
        assert_eq!(
            prepared_denied["error"]["code"],
            "recorded_evidence_required"
        );
        let mut stale = request.clone();
        stale["tracking"]["stage"] = json!("screening");
        stale["expected_record_sha256"] = json!("0".repeat(64));
        phase("post.stale.begin");
        let stale_denied =
            reviewed_post(app.clone(), &cookie, &state.csrf, "/api/tracking", stale).await;
        phase("post.stale.complete");
        assert_eq!(stale_denied["error"]["code"], "stale_record");
        let mut reviewed = request;
        reviewed["tracking"]["stage"] = json!("screening");
        reviewed["reason"] = json!("The user observed a screening request in this synthetic test.");
        reviewed["idempotency_key"] = json!("board-screening");
        phase("post.reviewed.begin");
        let applied = reviewed_post(
            app.clone(),
            &cookie,
            &state.csrf,
            "/api/tracking",
            reviewed.clone(),
        )
        .await;
        phase("post.reviewed.complete");
        assert_eq!(applied["ok"], true, "{applied}");
        phase("post.replay.begin");
        let replay = reviewed_post(app, &cookie, &state.csrf, "/api/tracking", reviewed).await;
        phase("post.replay.complete");
        assert_eq!(replay["ok"], true, "{replay}");
        phase("bootstrap.begin");
        let bootstrap = state.engine.call("bootstrap", &json!({}));
        phase("bootstrap.complete");
        assert_eq!(bootstrap["data"]["health"]["unfinished_run_count"], 0);
        assert!(std::fs::read_to_string(path).unwrap().contains("screening"));
        phase("dashboard.drop.begin");
        drop(state);
        phase("dashboard.drop.complete");
        phase("workspace.close.begin");
        temp.close().unwrap();
        phase("workspace.close.complete");
    }

    #[tokio::test]
    async fn native_review_switch_is_host_owned_and_browser_policy_is_fixed() {
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        let browser = DashboardState::new(temp.path(), 3210).unwrap();
        assert!(browser.set_native_review_mode(true).is_err());
        let state = DashboardState::with_native_policy(temp.path(), 3210, false).unwrap();
        let cookie = format!("{}={}", state.cookie_name, state.session);
        let app = router(state.clone());
        let make = || {
            Request::builder().method("POST").uri("/api/notes/import")
            .header("host", "127.0.0.1:3210").header("origin", "http://127.0.0.1:3210")
            .header("cookie", &cookie).header("x-job-harness-csrf", &state.csrf)
            .header("content-type", "application/json")
            .body(Body::from(json!({"text":"User supplied note","source_kind":"personal_notes","idempotency_key":"native-switch"}).to_string())).unwrap()
        };
        assert_eq!(
            app.clone().oneshot(make()).await.unwrap().status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        state.set_native_review_mode(true).unwrap();
        assert_eq!(
            app.clone().oneshot(make()).await.unwrap().status(),
            StatusCode::OK
        );
        state.set_native_review_mode(false).unwrap();
        assert_eq!(
            app.oneshot(make()).await.unwrap().status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
    }

    #[tokio::test]
    async fn notifications_require_tab_nonce_and_stream_only_current_hints() {
        use futures_util::StreamExt;
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        let state = DashboardState::with_native_policy(temp.path(), 3210, false).unwrap();
        let app = router(state.clone());
        let cookie = format!("{}={}", state.cookie_name, state.session);
        let request = |nonce: bool| {
            let mut builder = Request::builder()
                .uri("/api/changes")
                .header("host", "127.0.0.1:3210")
                .header("cookie", &cookie);
            if nonce {
                builder = builder.header("x-job-harness-csrf", &state.csrf);
            }
            builder.body(Body::empty()).unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request(false)).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let response = app.oneshot(request(true)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "application/x-ndjson; charset=utf-8"
        );
        let mut body = response.into_body().into_data_stream();
        let initial = body.next().await.unwrap().unwrap();
        let initial: Value = serde_json::from_slice(&initial).unwrap();
        assert_eq!(initial["data"]["mode"], "read_only");
        assert!(initial["data"].get("paths").is_none());
        // An idle signal performs no periodic workspace work and emits no snapshots.
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(250), body.next())
                .await
                .is_err()
        );
        state.set_native_review_mode(true).unwrap();
        let changed = tokio::time::timeout(std::time::Duration::from_secs(2), body.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let changed: Value = serde_json::from_slice(&changed).unwrap();
        assert_ne!(initial["data"]["revision"], changed["data"]["revision"]);
        assert_eq!(changed["data"]["mode"], "local_writes");
        assert_eq!(
            state.engine.call("list_events", &json!({"limit":1}))["data"]["page"]["total"],
            0
        );
        state.close_notifications();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), body.next())
                .await
                .unwrap()
                .is_none(),
            "Host shutdown must finish an idle stream"
        );
    }

    #[tokio::test]
    async fn native_model_actions_are_fixed_reviewed_host_routes() {
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        let browser = DashboardState::with_policy(temp.path(), 3210, true).unwrap();
        let request = |state: &DashboardState, body: Value| {
            Request::builder()
                .method("POST")
                .uri("/api/models/install")
                .header("host", "127.0.0.1:3210")
                .header("origin", "http://127.0.0.1:3210")
                .header("cookie", format!("{}={}", state.cookie_name, state.session))
                .header("x-job-harness-csrf", &state.csrf)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        let invalid = json!({"id":"embeddinggemma2-text","expected_plan_sha256":"0".repeat(64),"executable":"untrusted","url":"https://untrusted.example"});
        let response = router(browser.clone())
            .oneshot(request(&browser, invalid.clone()))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "A browser edit session gains no native install route"
        );
        let native = DashboardState::with_native_policy(temp.path(), 3210, false).unwrap();
        let app = router(native.clone());
        assert_eq!(
            app.clone()
                .oneshot(request(&native, invalid.clone()))
                .await
                .unwrap()
                .status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        native.set_native_review_mode(true).unwrap();
        let response = app.oneshot(request(&native, invalid)).await.unwrap();
        let value: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(value["error"]["code"], "invalid_arguments");
        assert_eq!(
            native.engine.call("list_events", &json!({"limit":1}))["data"]["page"]["total"],
            0
        );
        assert!(model_arguments(&json!({"id":true}), false).is_err());
        assert!(
            model_arguments(
                &json!({"id":"laya-s256","expected_plan_sha256":"stale"}),
                true
            )
            .is_err()
        );
    }

    fn fixture() -> (tempfile::TempDir, Router, String, String) {
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        let state = DashboardState::new(temp.path(), 3210).unwrap();
        let url = state.launch_url().unwrap();
        let launch = url
            .strip_prefix("http://127.0.0.1:3210")
            .unwrap()
            .to_owned();
        {
            let nonce = state.csrf.clone();
            (temp, router(state), launch, nonce)
        }
    }
    #[tokio::test]
    async fn dashboard_requires_local_origin_and_session() {
        let (_temp, app, launch_path, nonce) = fixture();
        let unauthenticated_home = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("host", "127.0.0.1:3210")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated_home.status(), StatusCode::UNAUTHORIZED);
        let root = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&launch_path)
                    .header("host", "127.0.0.1:3210")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(root.status(), StatusCode::OK);
        let replay = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&launch_path)
                    .header("host", "127.0.0.1:3210")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
        let cookie = root.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        assert!(
            root.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("HttpOnly; SameSite=Strict")
        );
        let unauth = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/overview")
                    .header("host", "127.0.0.1:3210")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
        let cross = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/overview")
                    .header("host", "127.0.0.1:3210")
                    .header("cookie", &cookie)
                    .header("origin", "https://example.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cross.status(), StatusCode::FORBIDDEN);
        let rebind = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("host", "evil.test:3210")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rebind.status(), StatusCode::FORBIDDEN);
        let allowed = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/overview")
                    .header("host", "127.0.0.1:3210")
                    .header("cookie", &cookie)
                    .header("x-job-harness-csrf", &nonce)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
        let mutation = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/pipeline")
                    .header("host", "127.0.0.1:3210")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(mutation.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
