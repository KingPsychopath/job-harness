//! Real local HTTP wire tests; no downloaded model, provider key or candidate data.
use axum::{Json, Router, body::Bytes, routing::post};
use job_harness_protocol::context_host::{self, Connection};
use serde_json::{Value, json};
use std::fs;

async fn fixture() -> (Connection, tokio::task::JoinHandle<()>) {
    async fn embed(Json(input): Json<Value>) -> Json<Value> {
        if input["model"] == "wrong-revision" {
            return Json(
                json!({"model_revision":"another-revision","data":[{"index":0,"embedding":[1.0,0.0]}]}),
            );
        }
        if input["model"] == "overlong" {
            return Json(json!({"padding":"x".repeat(5*1024*1024)}));
        }
        let rows=input["input"].as_array().unwrap().iter().enumerate().rev().map(|(i,s)|json!({"index":i,"embedding":if s.as_str().unwrap().to_lowercase().contains("autonomy"){vec![1.0,0.0]}else{vec![1.0,0.2]}})).collect::<Vec<_>>();
        Json(json!({"data":rows}))
    }
    async fn chat(Json(input): Json<Value>) -> Json<Value> {
        assert!(input.get("tools").is_none());
        if input["model"] == "enrich" || input["model"] == "bad-citation" {
            let context: Value =
                serde_json::from_str(input["messages"][1]["content"].as_str().unwrap()).unwrap();
            let source = &context["supplied_note"]["original"];
            let proposal = json!({"items":[{"id":"summary","kind":"summary","text":"Discussed the queue rollout.","citations":[{"path":source["path"],"sha256":if input["model"]=="bad-citation" {json!("0".repeat(64))}else{source["sha256"].clone()},"start_line":1,"end_line":1}]}],"questions":[]});
            return Json(json!({"choices":[{"message":{"content":proposal.to_string()}}]}));
        }
        Json(
            json!({"choices":[{"message":{"content":"{\"label\":\"other\",\"needs_reply\":null,\"urgency\":\"review\",\"uncertain\":true,\"rationale\":\"Fixture ambiguity\"}"}}]}),
        )
    }
    async fn audio(bytes: Bytes) -> Json<Value> {
        let wire = String::from_utf8_lossy(&bytes);
        assert!(wire.contains("name=\"file\""));
        assert!(wire.contains("synthetic audio"));
        Json(json!({"text":"Synthetic supplied transcript"}))
    }
    let router = Router::new()
        .route("/v1/embeddings", post(embed))
        .route("/v1/chat/completions", post(chat))
        .route("/v1/audio/transcriptions", post(audio));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (
        Connection {
            endpoint: format!("http://{address}/v1"),
            model: "fixture".into(),
            revision: "fixture-v1".into(),
            key_env: None,
            timeout_seconds: 10,
        },
        task,
    )
}
#[tokio::test]
async fn optional_embeddings_preserve_source_anchors_and_never_change_canonical_facts() {
    let (mut options, server) = fixture().await;
    let t = tempfile::tempdir().unwrap();
    harness_core::workspace::initialize(t.path()).unwrap();
    fs::create_dir_all(t.path().join("context/projects")).unwrap();
    let file = t.path().join("context/projects/story.md");
    let original = b"# Ownership\nDesigned a queue and owned its operation.\n";
    fs::write(&file, original).unwrap();
    let indexed = context_host::index_sources(
        t.path(),
        &options,
        &["context/projects/story.md".into()],
        40,
        "",
        "",
    )
    .await
    .unwrap();
    assert_eq!(indexed["passages"], 1);
    let found = context_host::semantic_search(t.path(), &options, "autonomy", 40, "", "", 5)
        .await
        .unwrap();
    assert_eq!(found["matches"][0]["path"], "context/projects/story.md");
    assert_eq!(found["matches"][0]["line_end"], 2);
    assert_eq!(fs::read(&file).unwrap(), original);
    fs::write(&file, b"Source changed").unwrap();
    let changed = context_host::semantic_search(t.path(), &options, "autonomy", 40, "", "", 5)
        .await
        .unwrap();
    assert!(changed["matches"].as_array().unwrap().is_empty());
    options.model = "overlong".into();
    assert!(
        context_host::embeddings(&options, &["bounded request".into()])
            .await
            .is_err()
    );
    options.model = "wrong-revision".into();
    assert!(
        context_host::embeddings(&options, &["selected".into()])
            .await
            .is_err()
    );
    server.abort();
}
#[tokio::test]
async fn enrichment_returns_chainable_validated_arguments_without_publishing() {
    let (mut options, server) = fixture().await;
    let t = tempfile::tempdir().unwrap();
    harness_core::workspace::initialize(t.path()).unwrap();
    let engine = harness_core::Engine::open(t.path()).unwrap();
    let note=engine.call("intake_note",&json!({"text":"Discussed the queue rollout.\n","source_kind":"personal_notes","idempotency_key":"fixture-note"}));
    assert_eq!(note["ok"], true);
    let id = note["data"]["note_id"].as_str().unwrap();
    options.model = "enrich".into();
    let output = context_host::enrich(t.path(), &options, id, &[])
        .await
        .unwrap();
    assert_eq!(output["canonical_changes"], false);
    let before = engine.call("review_note", &json!({"note_id":id}));
    assert!(before["data"]["proposals"].as_array().unwrap().is_empty());
    assert_eq!(
        engine.call("propose_note", &output["propose_arguments"])["ok"],
        true
    );
    options.model = "bad-citation".into();
    assert!(
        context_host::enrich(t.path(), &options, id, &[])
            .await
            .is_err()
    );
    server.abort();
}
#[tokio::test]
async fn transcript_and_triage_are_drafts_with_explicit_unknowns_and_no_tool_calls() {
    let (options, server) = fixture().await;
    let decision = context_host::classify(&options, "A vague fixture update")
        .await
        .unwrap();
    assert!(decision["decision"]["needs_reply"].is_null());
    assert_eq!(decision["archive_permitted"], false);
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("supplied.wav");
    fs::write(&file, b"synthetic audio").unwrap();
    let result = context_host::transcribe(&options, &file).await.unwrap();
    assert_eq!(result["accepted"], false);
    assert_eq!(result["canonical_changes"], false);
    assert_eq!(result["text"], "Synthetic supplied transcript");
    assert!(result["speakers"].is_null());
    server.abort();
}
