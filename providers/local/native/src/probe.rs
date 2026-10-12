//! Reproducible native HTTP/Rust integration probe on fixed fictional cases. Never downloads.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    process::Command,
    time::{Duration, Instant},
};
const FIXTURE: &str = include_str!("../../../../examples/models/triage-fixture.json");
fn request(endpoint: &str, route: &str, payload: Option<&Value>) -> Result<(Value, f64), String> {
    let started = Instant::now();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(40)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = if let Some(p) = payload {
        agent
            .post(format!("{endpoint}{route}"))
            .header("Content-Type", "application/json")
            .send(serde_json::to_vec(p).unwrap())
            .map_err(|_| "probe_failed: HTTP request failed")?
    } else {
        agent
            .get(format!("{endpoint}{route}"))
            .call()
            .map_err(|_| "probe_failed: health request failed")?
    };
    let data = response
        .body_mut()
        .with_config()
        .limit(4 * 1024 * 1024)
        .read_to_string()
        .map_err(|_| "probe_failed: response bound exceeded")?;
    let result = serde_json::from_str(&data).map_err(|_| "probe_failed: invalid responseJSON")?;
    Ok((result, started.elapsed().as_secs_f64() * 1000.0))
}
pub fn run(opts: &HashMap<String, String>) -> Result<Value, String> {
    let selected = opts
        .get("--model")
        .ok_or("invalid_arguments: probe --model required")?;
    if !["embeddinggemma2-text", "laya-s256", "kev-0.8b"].contains(&selected.as_str()) {
        return Err("invalid_arguments: select known decision model".into());
    }
    let endpoint = opts
        .get("--endpoint")
        .ok_or("invalid_arguments: explicit loopback --endpoint required")?
        .trim_end_matches('/')
        .trim_end_matches("/v1");
    if !endpoint.starts_with("http://127.0.0.1:") || endpoint[17..].parse::<u16>().is_err() {
        return Err("invalid_arguments: select explicit loopback port".into());
    }
    let spec = crate::catalog::model(selected)?;
    if selected == "embeddinggemma2-text" {
        return embedding_probe(opts, endpoint, &spec);
    }
    let revision = &spec["revision"];
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let (health, _) = request(endpoint, "/health", None)?;
    let mut rows = Vec::new();
    let mut warm = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        for order in ["original", "reversed"] {
            let mut questions = fixture["questions"].clone();
            if order == "reversed" {
                for name in ["label", "urgency"] {
                    let old = questions[name]["criteria"].as_object().unwrap();
                    let reversed = old
                        .iter()
                        .rev()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<serde_json::Map<_, _>>();
                    questions[name]["criteria"] = json!(reversed);
                }
            }
            let payload = json!({"model":selected,"state":case["text"],"questions":questions});
            let (_, first) = request(endpoint, "/v1/decisions", Some(&payload))?;
            let (result, last) = request(endpoint, "/v1/decisions", Some(&payload))?;
            if &result["model_revision"] != revision
                || result["canonical_changes"] != false
                || !result["results"]["action"]["value"].is_boolean()
            {
                return Err("probe_failed: typed model contract violated".into());
            }
            warm.push(last);
            rows.push(json!({"id":case["id"],"order":order,"truth":{"label":case["expected_label"],"needs_reply":case["expected_needs_reply"]},"raw_decision":result,"first_shape_http_ms":first,"warm_repeat_http_ms":last}));
        }
    }
    let mut guarded = Vec::new();
    let mut harness_rows = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let payload = json!({"model":selected,"messages":[{"role":"system","content":"Return ONLY JSON {label,needs_reply,urgency,uncertain,rationale}."},{"role":"user","content":serde_json::to_string(&json!({"text":case["text"]})).unwrap()}],"response_format":{"type":"json_object"},"stream":false});
        let (reply, elapsed) = request(endpoint, "/v1/chat/completions", Some(&payload))?;
        let decision: Value = serde_json::from_str(
            reply["choices"][0]["message"]["content"]
                .as_str()
                .ok_or("probe_failed: typed triage missing")?,
        )
        .map_err(|_| "probe_failed: typed triage JSON invalid")?;
        if case["expected_needs_reply"] == true
            && decision["needs_reply"] != true
            && decision["uncertain"] != true
        {
            return Err("probe_failed: action lost without abstention".into());
        }
        guarded.push(json!({"id":case["id"],"decision":decision,"http_elapsed_ms":elapsed}));
        if let Some(binary) = opts.get("--harness") {
            let directory =
                tempfile::tempdir().map_err(|_| "probe_failed: temporary fixture unavailable")?;
            let path = directory.path().join("message.txt");
            std::fs::write(&path, case["text"].as_str().unwrap())
                .map_err(|_| "probe_failed: fixture write failed")?;
            let started = Instant::now();
            let output = Command::new(binary)
                .args([
                    "--root",
                    directory.path().to_str().unwrap(),
                    "context",
                    "triage",
                    "--input",
                    path.to_str().unwrap(),
                    "--endpoint",
                    &format!("{endpoint}/v1"),
                    "--model",
                    selected,
                    "--revision",
                    revision.as_str().unwrap(),
                ])
                .output()
                .map_err(|_| "probe_failed: Rust core command failed")?;
            let mut response: Value = serde_json::from_slice(&output.stdout)
                .map_err(|_| "probe_failed: Rust core response invalid")?;
            if !output.status.success()
                || response["ok"] != true
                || response["data"]["archive_permitted"] != false
            {
                return Err("probe_failed: Rust triage contract violated".into());
            }
            sanitize(&mut response, directory.path().to_str().unwrap());
            harness_rows.push(json!({"id":case["id"],"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"response":response}));
        }
    }
    let flips = rows
        .chunks_exact(2)
        .filter(|pair| {
            pair[0]["raw_decision"]["results"]["label"]["value"]
                != pair[1]["raw_decision"]["results"]["label"]["value"]
        })
        .map(|pair| pair[0]["id"].clone())
        .collect::<Vec<_>>();
    let choice_flips=rows.chunks_exact(2).flat_map(|pair|["label","urgency"].into_iter().filter_map(move |question|{let a=&pair[0]["raw_decision"]["results"][question]["value"];let b=&pair[1]["raw_decision"]["results"][question]["value"];if a!=b {Some(json!({"id":pair[0]["id"],"question":question,"original":a,"reversed":b}))}else{None}})).collect::<Vec<_>>();
    let mut sorted = warm.clone();
    sorted.sort_by(f64::total_cmp);
    let median = (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0;
    let report = json!({"measurement_date":"2026-10-12","scope":"Real native HTTP/Rust feasibility smoke: six fictional cases and two option orders; no private facts, quality ranking, actions or downloads.","fixture_sha256":hex::encode(Sha256::digest(FIXTURE)),"model":selected,"revision":revision,"health":health,"raw_cases":rows,"guarded_triage":guarded,"rust_cli_triage":harness_rows,"observed_option_order_label_flips":flips,"observed_option_order_choice_flips":choice_flips,"warm_repeat_http_ms":{"values":warm,"median":median,"maximum":sorted.last()},"quality_claim":"No general accuracy or calibration claim; inspect raw errors and abstentions."});
    let output = opts
        .get("--output")
        .ok_or("invalid_arguments: explicit report --output required")?;
    if let Some(parent) = std::path::Path::new(output).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| "probe_failed: output directory unavailable")?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap())
        .map_err(|_| "probe_failed: report write failed")?;
    Ok(
        json!({"model":selected,"rows":rows.len(),"warm_median_ms":median,"option_order_label_flips":flips,"output":output}),
    )
}
fn sanitize(value: &mut Value, root: &str) {
    match value {
        Value::String(s) => {
            *s = s.replace(root, "<synthetic-temp-root>");
        }
        Value::Array(values) => {
            for value in values {
                sanitize(value, root)
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                sanitize(value, root)
            }
        }
        _ => {}
    }
}
fn embedding_probe(
    opts: &HashMap<String, String>,
    endpoint: &str,
    spec: &Value,
) -> Result<Value, String> {
    let (health, _) = request(endpoint, "/health", None)?;
    let documents = [
        "Reduced database response times by removing repeated SQL queries.",
        "Built an accessible calendar interface for selecting appointments.",
        "Added image cropping and colour controls to a photo editor.",
        "Wrote recovery tests that reconcile interrupted application attempts.",
    ];
    let inputs = (0..32)
        .map(|i| format!("title: none | text: {}", documents[i % 4]))
        .collect::<Vec<_>>();
    let (indexed, index_ms) = request(
        endpoint,
        "/v1/embeddings",
        Some(&json!({"model":"embeddinggemma2-text","input":inputs})),
    )?;
    let mut warm = Vec::new();
    let mut query = Value::Null;
    for _ in 0..5 {
        let (value, elapsed) = request(
            endpoint,
            "/v1/embeddings",
            Some(
                &json!({"model":"embeddinggemma2-text","input":["task: search result | query: speeding up slow database queries"]}),
            ),
        )?;
        query = value;
        warm.push(elapsed);
    }
    let vector = query["data"][0]["embedding"]
        .as_array()
        .ok_or("probe_failed: query embedding missing")?;
    if vector.len() != 768 || indexed["data"].as_array().is_none_or(|v| v.len() != 32) {
        return Err("probe_failed: embedding dimensions or count mismatch".into());
    }
    let scores = (0..4)
        .map(|i| {
            vector
                .iter()
                .zip(indexed["data"][i]["embedding"].as_array().unwrap())
                .map(|(a, b)| a.as_f64().unwrap() * b.as_f64().unwrap())
                .sum::<f64>()
        })
        .collect::<Vec<_>>();
    if scores
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .unwrap()
        .0
        != 0
    {
        return Err("probe_failed: synthetic retrieval rank mismatch".into());
    }
    let mut harness_report = Value::Null;
    if let Some(binary) = opts.get("--harness") {
        let temp =
            tempfile::tempdir().map_err(|_| "probe_failed: temporary workspace unavailable")?;
        let root = temp.path().join("workspace");
        let execute = |arguments: Vec<String>| -> Result<Value, String> {
            let started = Instant::now();
            let output = Command::new(binary)
                .arg("--root")
                .arg(&root)
                .args(arguments)
                .output()
                .map_err(|_| "probe_failed: Rust core command failed")?;
            let mut value: Value = serde_json::from_slice(&output.stdout)
                .map_err(|_| "probe_failed: Rust core response invalid")?;
            if !output.status.success() || value["ok"] != true {
                return Err("probe_failed: Rust core embedding contract failed".into());
            }
            sanitize(&mut value, root.to_str().unwrap());
            Ok(json!({"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"response":value}))
        };
        let initialized = execute(vec!["init".into()])?;
        std::fs::create_dir_all(root.join("context/projects"))
            .map_err(|_| "probe_failed: fixture folder unavailable")?;
        std::fs::write(root.join("context/projects/database.md"),"# Fictional database project\n\nSource: synthetic fixture; no career evidence.\n\nReduced response times by eliminating repeated SQL reads.\n").map_err(|_|"probe_failed: fixture unavailable")?;
        std::fs::write(root.join("context/projects/images.md"),"# Fictional photo editor\n\nSource: synthetic fixture; no career evidence.\n\nAdded image cropping and colour controls to a photo editing interface.\n").map_err(|_|"probe_failed: fixture unavailable")?;
        let common = vec![
            "--endpoint".into(),
            format!("{endpoint}/v1"),
            "--model".into(),
            "embeddinggemma2-text".into(),
            "--revision".into(),
            spec["revision"].as_str().unwrap().into(),
            "--document-prefix".into(),
            "title: none | text: ".into(),
            "--query-prefix".into(),
            "task: search result | query: ".into(),
            "--timeout-seconds".into(),
            "30".into(),
        ];
        let mut index_args = vec!["context".into(), "index".into()];
        index_args.extend(common.clone());
        index_args.extend(
            [
                "--source",
                "context/projects/database.md",
                "--source",
                "context/projects/images.md",
            ]
            .map(String::from),
        );
        let index = execute(index_args)?;
        let mut search_args = vec!["context".into(), "search".into()];
        search_args.extend(common);
        search_args.extend(["--query", "speeding up slow database queries"].map(String::from));
        let search = execute(search_args)?;
        if search["response"]["data"]["matches"][0]["path"] != "context/projects/database.md" {
            return Err("probe_failed: Rust retrieval source mismatch".into());
        }
        harness_report = json!({"init":initialized,"index":index,"search":search});
    }
    let report = json!({"measurement_date":"2026-10-12","scope":"Real native C API via HTTP; fictional retrieval inputs; no Python runtime, private facts or model downloads.","model":"embeddinggemma2-text","revision":spec["revision"],"health":health,"embedding_batch":{"passages":32,"dimensions":768,"http_elapsed_ms":index_ms,"metrics":indexed["metrics"]},"warm_query_http_ms":warm,"toy_cosine_scores":scores,"quality_claim":"One synthetic retrieval smoke, not a relevance benchmark.","rust_harness":harness_report,"reference_vectors":{"documents":indexed["data"].as_array().unwrap()[..4].to_vec(),"query":query["data"][0]}});
    let output = opts
        .get("--output")
        .ok_or("invalid_arguments: explicit report --output required")?;
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap())
        .map_err(|_| "probe_failed: report write failed")?;
    Ok(
        json!({"model":"embeddinggemma2-text","passages":32,"dimensions":768,"batch_http_ms":index_ms,"warm_query_http_ms":warm,"output":output}),
    )
}
