mod artifacts;
mod catalog;
mod google;
mod kev;
mod probe;
mod runtime;
mod server;
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, time::Duration};
fn arguments() -> Result<(String, HashMap<String, String>), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or("help".into());
    let mut opts = HashMap::new();
    while let Some(key) = args.next() {
        if !key.starts_with("--") || opts.contains_key(&key) {
            return Err("invalid_arguments: use unique known flags".into());
        }
        let value = args.next().ok_or("invalid_arguments: flag value missing")?;
        opts.insert(key, value);
    }
    Ok((command, opts))
}
fn emit(value: Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema_version":2,"ok":true,"data":value,"error":null,"next":[]})
        )
        .unwrap()
    );
}
const HELP: &str = r#"Job Harness optional native model provider

This component is optional. The core CLI needs no models, SDKs or Python.
Every download requires the token printed by info. Serve never downloads.

Commands:
  list                         Compact model and native runtime options
  info --model ID              Full pinned weights, license and memory metadata
  info --runtime ID            Native SDK archive size, source and SHA256
  download --model ID --approve TOKEN
  download --runtime ID --approve TOKEN
  verify --model ID             Check every cached artifact against the plan
  verify --runtime ID           Check archive and extracted native library
  serve --decision-model laya-s256 [--embedding-model embeddinggemma2-text]
  serve --decision-model kev-0.8b --kev-sidecar PATH --kev-metallib PATH
  doctor                       Test a fresh native worker without SDKs or models
  --version                    Print this optional component's version
  summary --output PATH        Regenerate the compact catalog view
  encoding-check --output PATH Compare Kev token IDs with the author fixture
  probe --model ID --endpoint http://127.0.0.1:PORT/v1 --output REPORT.json
        [--harness PATH]        Fixed fictional cases; no downloads or actions

Common options: --cache PATH, --threads 1..16, --deadline 1..120, --port PORT.
The default address is http://127.0.0.1:18081. CLI results use version 2 JSON.

Google engines load reviewed standalone C libraries from official SDK archives;
no Python is installed or executed. Vendor builds exist for macOS arm64, Linux
x86_64/aarch64, Windows x86_64, and MediaPipe Windows arm64. Actual inference is
verified on macOS arm64. Gemma CPU threads are explicit; Laya threads are advisory.
Kev needs the separately compiled Swift/MLX sidecar on Apple Silicon macOS26+.
Available model profiles are EmbeddingGemma2 text, Laya s256 and Kev 0.8B.

Example (replace TOKEN after reviewing each info result):
  job-harness-local info --model laya-s256
  job-harness-local info --runtime mediapipe-1.1.0-macos-arm64
  job-harness-local download --model laya-s256 --approve TOKEN
  job-harness-local download --runtime mediapipe-1.1.0-macos-arm64 --approve TOKEN
  job-harness-local serve --decision-model laya-s256

Typed HTTP: /v1/embeddings, /v1/decisions, fixed triage /v1/chat/completions.
No free-form generation, tools, live audio or automatic consequential decisions.
See docs/MODELS.md for model sources, offline cache transfer and native Kev build.
"#;
fn run() -> Result<(), String> {
    if matches!(std::env::args().nth(1).as_deref(), Some("--version" | "-V")) {
        println!("job-harness-local {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if std::env::args().skip(1).any(|a| a == "--help" || a == "-h")
        || std::env::args().nth(1).is_none()
        || std::env::args().nth(1).as_deref() == Some("help")
    {
        print!("{HELP}");
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--worker") {
        return server::worker_main();
    }
    let (command, opts) = arguments()?;
    if opts.keys().any(|k| {
        ![
            "--model",
            "--runtime",
            "--cache",
            "--approve",
            "--embedding-model",
            "--decision-model",
            "--threads",
            "--port",
            "--deadline",
            "--kev-sidecar",
            "--kev-metallib",
            "--endpoint",
            "--output",
            "--harness",
        ]
        .contains(&k.as_str())
    }) {
        return Err("invalid_arguments: unknown flag".into());
    }
    let cache = opts
        .get("--cache")
        .map(PathBuf::from)
        .unwrap_or_else(catalog::default_cache);
    match command.as_str() {
        "summary" => {
            let value = catalog::summary();
            if let Some(output) = opts.get("--output") {
                std::fs::write(output, serde_json::to_vec_pretty(&value).unwrap())
                    .map_err(|_| "invalid_arguments: cannot write summary")?;
            }
            emit(value);
        }
        "encoding-check" => {
            let output = opts
                .get("--output")
                .ok_or("invalid_arguments: encoding-check requires --output")?;
            emit(kev::check_encoding(&cache, std::path::Path::new(output))?);
        }
        "doctor" => {
            let worker = server::Worker::start(HashMap::new(), 2, Duration::from_secs(5))?;
            emit(
                json!({"platform":catalog::platform(),"native_worker":worker.startup,"sdk_loads":0,"model_downloads":0}),
            );
        }
        "probe" => emit(probe::run(&opts)?),
        "list" => emit(catalog::summary()),
        "info" | "download" | "verify" => {
            let spec = if let Some(id) = opts.get("--runtime") {
                catalog::runtime(id)?
            } else {
                catalog::model(
                    opts.get("--model")
                        .ok_or("invalid_arguments: explicit --model or --runtime required")?,
                )?
            };
            if command == "info" {
                emit(catalog::info(spec, &cache));
            } else if command == "verify" {
                emit(json!({"verified":true,"path":artifacts::verify(&cache,&spec)?}));
            } else {
                emit(artifacts::install(
                    &cache,
                    &spec,
                    opts.get("--approve")
                        .ok_or("approval_required: inspect info and supply --approve")?,
                )?);
            }
        }
        "serve" => {
            let mut paths = HashMap::new();
            if let Some(id) = opts.get("--embedding-model") {
                if id != "embeddinggemma2-text" {
                    return Err("unknown_model: explicit supported embedding model required".into());
                }
                paths.insert(id.clone(), artifacts::verify(&cache, &catalog::model(id)?)?);
                paths.insert(
                    "litert-lm".into(),
                    artifacts::verify(&cache, &catalog::selected_runtime("litert-lm")?)?,
                );
            }
            if let Some(id) = opts.get("--decision-model") {
                if !["laya-s256", "kev-0.8b"].contains(&id.as_str()) {
                    return Err(
                        "unsupported_runtime: select a supported native decision model".into(),
                    );
                }
                paths.insert(id.clone(), artifacts::verify(&cache, &catalog::model(id)?)?);
                if id == "laya-s256" {
                    paths.insert(
                        "mediapipe".into(),
                        artifacts::verify(&cache, &catalog::selected_runtime("mediapipe")?)?,
                    );
                } else {
                    for (key, flag) in [
                        ("kev-native", "--kev-sidecar"),
                        ("kev-metallib", "--kev-metallib"),
                    ] {
                        let path = PathBuf::from(opts.get(flag).ok_or(
                            "unsupported_runtime: explicit Kev sidecar and metallib paths required",
                        )?);
                        paths.insert(
                            key.into(),
                            std::fs::canonicalize(path)
                                .map_err(|_| "unsupported_runtime: Kev component missing")?,
                        );
                    }
                }
            }
            if paths.is_empty() {
                return Err(
                    "invalid_arguments: select embedding or decision model explicitly".into(),
                );
            }
            let threads = opts.get("--threads").map_or(Ok(2), |s| {
                s.parse::<i32>()
                    .map_err(|_| "invalid_arguments: threads integer required")
            })?;
            let deadline = opts.get("--deadline").map_or(Ok(60), |s| {
                s.parse::<u64>()
                    .map_err(|_| "invalid_arguments: deadline integer required")
            })?;
            if !(1..=16).contains(&threads) || !(1..=120).contains(&deadline) {
                return Err(
                    "invalid_arguments: threads must be 1–16 and deadline 1–120 seconds".into(),
                );
            }
            let port = opts.get("--port").map_or(Ok(18081), |s| {
                s.parse::<u16>()
                    .map_err(|_| "invalid_arguments: port integer required")
            })?;
            for path in paths.values_mut() {
                *path = std::fs::canonicalize(&*path)
                    .map_err(|_| "model_integrity_error: verified artifact path unavailable")?;
            }
            let worker = server::Worker::start(paths, threads, Duration::from_secs(deadline))?;
            server::serve(worker, port)?;
        }
        _ => return Err("invalid_arguments: use list/info/download/verify/serve".into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        let (code, message) = error.split_once(':').unwrap_or(("provider_error", &error));
        let recovery = match code {
            "approval_required" => {
                "Run info for the selected model or runtime, review the download scope, then supply its exact --approve token."
            }
            "model_integrity_error" => {
                "Run verify, then repeat the explicitly approved download to repair cached artifacts."
            }
            "runtime_start_failed" | "unsupported_runtime" => {
                "Review info and the native SDK platform requirements; install only the explicitly selected runtime."
            }
            "runtime_timeout" | "runtime_unavailable" => {
                "Restart serve explicitly after inspecting the selected model and request bounds."
            }
            _ => "Run job-harness-local --help for commands and supported inputs.",
        };
        eprintln!(
            "{}",
            json!({"schema_version":2,"ok":false,"data":null,"error":{"code":code,"message":message.trim(),"retryable":false,"recovery":recovery},"next":[{"action":recovery}]})
        );
        std::process::exit(1)
    }
}
