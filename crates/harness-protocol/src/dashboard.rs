//! Loopback workspace views and explicitly enabled, named human review routes.
use axum::{
    Json, Router,
    extract::{Query, State},
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
    sync::Arc,
};
use subtle::ConstantTimeEq;

const INDEX: &str = include_str!("../../../gui/index.html");
const CSS: &str = include_str!("../../../gui/app.css");
const JS: &str = include_str!("../../../gui/app.js");
const COOKIE: &str = "job_harness_session";

#[derive(Clone)]
pub struct DashboardState {
    engine: Arc<Engine>,
    authority: String,
    origin: String,
    session: String,
    csrf: String,
    local_writes: bool,
}

impl DashboardState {
    pub fn new(root: &Path, port: u16) -> anyhow::Result<Self> {
        Self::with_policy(root, port, false)
    }
    pub fn with_policy(root: &Path, port: u16, local_writes: bool) -> anyhow::Result<Self> {
        let authority = format!("127.0.0.1:{port}");
        Ok(Self {
            engine: Arc::new(Engine::open(root)?),
            origin: format!("http://{authority}"),
            authority,
            session: uuid::Uuid::new_v4().simple().to_string(),
            csrf: uuid::Uuid::new_v4().simple().to_string(),
            local_writes,
        })
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
                    name == COOKIE && bool::from(value.as_bytes().ct_eq(state.session.as_bytes()))
                })
        })
        .unwrap_or(false)
}

async fn guard(
    State(state): State<DashboardState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if request.method() != Method::GET && !(request.method() == Method::POST && state.local_writes)
    {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if !same_origin(request.headers(), &state) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if request.uri().path().starts_with("/api/") && !has_session(request.headers(), &state) {
        return StatusCode::UNAUTHORIZED.into_response();
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
    let html = INDEX.replace("{{CSRF}}", &state.csrf).replace(
        "{{WRITE_MODE}}",
        if state.local_writes {
            "local_writes"
        } else {
            "read_only"
        },
    );
    let mut response = Html(html).into_response();
    // A browser session, not a stored credential. Never put it in URLs or logs.
    let cookie = format!(
        "{COOKIE}={}; Path=/; HttpOnly; SameSite=Strict",
        state.session
    );
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert("set-cookie", value);
    }
    response
}
async fn css() -> impl IntoResponse {
    ([("content-type", "text/css; charset=utf-8")], CSS)
}
async fn js() -> impl IntoResponse {
    ([("content-type", "text/javascript; charset=utf-8")], JS)
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
        json!({"schema_version":2,"ok":true,"data":{"mode":if state.local_writes{"local_writes"}else{"read_only"},"version":env!("CARGO_PKG_VERSION"),"mcp":{"transport":"stdio","focused_tools":crate::FOCUSED_TOOLS.len(),"full_tools":harness_core::contracts::specs().len()},"acp":{"role":"client","external_actions_default":false},"models":crate::context_host::model_metadata(),"workspace":state.engine.root(),"capabilities":["local_records","notes_review","search","packet_preview"],"external_connectors":"Configured and authenticated in the agent host"},"error":null,"next":[]}),
    )
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
    let record = args["record"].as_str().unwrap_or_default();
    let key = args["idempotency_key"].as_str().unwrap_or_default();
    let reason = args["reason"].as_str().unwrap_or_default();
    let Some(fields) = args["tracking"].as_object() else {
        return Json(crate::denied());
    };
    if key.is_empty()
        || reason.trim().is_empty()
        || fields
            .keys()
            .any(|k| !["stage", "next_action", "status_detail"].contains(&k.as_str()))
        || args.get("correction_reason").is_some()
    {
        return Json(crate::denied());
    }
    let plan = json!({"steps":[{"id":"run","tool":"start_run","arguments":{"operation":"reconcile","scope":{"record":record},"user_request":format!("Explicit GUI review: {reason}"),"idempotency_key":format!("{key}:start")}},
       {"id":"update","tool":"update_application","arguments":{"run_id":{"$ref":"run.data.run_id"},"record":record,"tracking":fields,"summary":format!("Explicit local GUI review: {reason}"),"evidence":[record],"idempotency_key":format!("{key}:update")}},
       {"id":"finish","tool":"finish_run","arguments":{"run_id":{"$ref":"run.data.run_id"},"status":"completed","summary":"Explicit GUI tracking review recorded","evidence":[record],"idempotency_key":format!("{key}:finish")}}]});
    invoke(state, "execute_batch", plan).await
}

pub fn router(state: DashboardState) -> Router {
    let mut app = Router::new()
        .route("/", get(home))
        .route("/app.css", get(css))
        .route("/app.js", get(js))
        .route("/api/overview", get(overview))
        .route("/api/pipeline", get(pipeline))
        .route("/api/attention", get(attention))
        .route("/api/search", get(search))
        .route("/api/source", get(source))
        .route("/api/notes", get(notes))
        .route("/api/notes/review", get(review))
        .route("/api/packet", get(packet))
        .route("/api/company", get(company))
        .route("/api/connections", get(connections));
    if state.local_writes {
        app = app
            .route("/api/notes/import", post(import_note))
            .route("/api/notes/propose", post(propose_note))
            .route("/api/notes/accept", post(accept_note))
            .route("/api/notes/archive", post(archive_note))
            .route("/api/tracking", post(tracking));
    }
    app.layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
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
    eprintln!("Private local view. Ctrl-C closes this session.");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
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
        let cookie = format!("{COOKIE}={}", state.session);
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
        let cookie = format!("{COOKIE}={}", state.session);
        let app = router(state);
        let accepted = reviewed_post(app.clone(), &cookie, &nonce, "/api/tracking", json!({
            "record":"applications/example.md","tracking":{"stage":"screening","next_action":"Prepare for the screening call"},
            "reason":"Hiring team invited me to a screening call.","idempotency_key":"review-tracking"})).await;
        assert_eq!(accepted["ok"], true, "{accepted}");
        let bytes = std::fs::read(&path).unwrap();
        let refused = reviewed_post(app.clone(), &cookie, &nonce, "/api/tracking", json!({
            "record":"applications/example.md","tracking":{"stage":"saved","next_action":"Apply again"},
            "reason":"Synthetic attempted regression","idempotency_key":"review-regression"})).await;
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
        let app = router(state);
        let home = app
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
        let cookie = home.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let html = String::from_utf8(
            home.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(html.contains(&nonce));
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

    fn fixture() -> (tempfile::TempDir, Router) {
        let temp = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(temp.path()).unwrap();
        let state = DashboardState::new(temp.path(), 3210).unwrap();
        (temp, router(state))
    }
    #[tokio::test]
    async fn dashboard_requires_local_origin_and_session() {
        let (_temp, app) = fixture();
        let root = app
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
        assert_eq!(root.status(), StatusCode::OK);
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
