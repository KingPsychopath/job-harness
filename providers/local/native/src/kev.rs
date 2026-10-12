//! Kev author encoding plus a native Swift MLX process. No pickle execution or interpreter.
use crate::runtime::{checked, decisions};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
const HEAD_SHA: &str = "f400bd12802b2b105ae45d6b03774a158a3db4fccff42413734ddca2e5c920b6";
const METAL_SHA: &str = "95974b7de464ec2be7833a914ca5a53d283a70789470fd84218738fc8fcea3e1";
const STORAGE: [(&str, usize, &str); 4] = [
    (
        "q.weight",
        1048576,
        "8b0c5514125d8177b17c8cb63418e1b4efb102626bb4cebbd44388a318fe6ad9",
    ),
    (
        "q.bias",
        1024,
        "6f8bea894a8a5347a6ebe334a9afb64ee1d5ba48f90efd80f5de65c1a31a0e9a",
    ),
    (
        "k.weight",
        1048576,
        "e43aa59692a4a2d09cd8c0beb9ae7ea9c18b2bc5a44a37a60aa4323b53da1b0d",
    ),
    (
        "k.bias",
        1024,
        "a02d492fd4372da9cb0053e5ed450607323b660bad899acae9df6df7e82d6169",
    ),
];
fn prepare(root: &Path) -> Result<PathBuf, String> {
    let head = root.join("adapter/head.pt");
    if crate::artifacts::hash(&head)? != HEAD_SHA {
        return Err("model_integrity_error: reviewed Kev head SHA256 required".into());
    }
    let mut zip = zip::ZipArchive::new(
        File::open(head).map_err(|_| "model_integrity_error: head archive missing")?,
    )
    .map_err(|_| "model_integrity_error: head archive invalid")?;
    let dir = root.join("native-head");
    fs::create_dir_all(&dir).map_err(|_| "model_integrity_error: cannot prepare head")?;
    // The exact immutable archive establishes storage/name/shape mapping. data.pkl is never parsed or executed.
    for (i, (name, size, sha)) in STORAGE.iter().enumerate() {
        let mut entry = zip
            .by_name(&format!("head/data/{i}"))
            .map_err(|_| "model_integrity_error: fixed head storage missing")?;
        if entry.size() != *size as u64 {
            return Err("model_integrity_error: head storage size mismatch".into());
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|_| "model_integrity_error: head storage unreadable")?;
        if hex::encode(Sha256::digest(&bytes)) != *sha {
            return Err("model_integrity_error: head storage SHA256 mismatch".into());
        }
        let output = dir.join(format!("{name}.f32"));
        if !output.is_file() || crate::artifacts::hash(&output)? != *sha {
            let tmp = output.with_extension("part");
            fs::write(&tmp, &bytes).map_err(|_| "model_integrity_error: head prepare failed")?;
            fs::rename(tmp, output)
                .map_err(|_| "model_integrity_error: head prepare publication failed")?;
        }
    }
    Ok(dir)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Branch {
    pub ids: Vec<u32>,
    pub decide: usize,
    pub options: Vec<usize>,
}
pub struct Encoder {
    tokenizer: tokenizers::Tokenizer,
    markers: [u32; 5],
    escape: regex::Regex,
}
impl Encoder {
    pub fn load(root: &Path) -> Result<Self, String> {
        let mut tokenizer = tokenizers::Tokenizer::from_file(root.join("base/tokenizer.json"))
            .map_err(|_| "model_integrity_error: Kev tokenizer invalid")?;
        tokenizer
            .with_truncation(None)
            .map_err(|_| "model_integrity_error: tokenizer limits invalid")?;
        tokenizer.with_padding(None);
        let mut markers = [0; 5];
        for (i, name) in [
            "<|fim_prefix|>",
            "<|fim_middle|>",
            "<|box_start|>",
            "<|box_end|>",
            "<|fim_suffix|>",
        ]
        .iter()
        .enumerate()
        {
            markers[i] = tokenizer
                .token_to_id(name)
                .ok_or("model_integrity_error: Kev delimiter missing")?;
        }
        Ok(Self {
            tokenizer,
            markers,
            escape: regex::Regex::new(r"<\|([A-Za-z0-9_]+)\|>").unwrap(),
        })
    }
    fn user(&self, text: &str) -> Result<Vec<u32>, String> {
        self.tokenizer
            .encode(self.escape.replace_all(text, "<¦$1¦>").as_ref(), false)
            .map(|e| e.get_ids().to_vec())
            .map_err(|_| "invalid_arguments: Kev tokenizer rejected text".into())
    }
    pub fn rows(
        &self,
        state: &str,
        questions: &Map<String, Value>,
    ) -> Result<(Vec<Branch>, usize), String> {
        let mut prefix = vec![self.markers[0]];
        prefix.extend(self.user(state)?);
        if prefix.len() > 8192 {
            return Err("context_budget: Kev state exceeds strict 8,192 token budget".into());
        }
        let mut rows = Vec::new();
        for q in questions.values() {
            let kind = q["type"].as_str().unwrap();
            let (instruction, options) = match kind {
                "choice" => (
                    q["instructions"].as_str().unwrap(),
                    q["criteria"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(key, description)| {
                            format!("{key}: {}", description.as_str().unwrap())
                        })
                        .collect::<Vec<_>>(),
                ),
                "score" => (
                    q["instructions"].as_str().unwrap(),
                    q["rubric"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().into())
                        .collect(),
                ),
                _ => (
                    q["condition"].as_str().unwrap(),
                    vec!["no".into(), "yes".into()],
                ),
            };
            let mut ids = prefix.clone();
            ids.push(self.markers[1]);
            ids.extend(self.user(instruction)?);
            let mut positions = Vec::new();
            for option in options {
                ids.push(self.markers[2]);
                ids.extend(self.user(&option)?);
                ids.push(self.markers[3]);
                positions.push(ids.len() - 1);
            }
            ids.push(self.markers[4]);
            if ids.len() > 16384 {
                return Err(
                    "context_budget: Kev causal row exceeds strict 16,384 token budget".into(),
                );
            }
            let decide = ids.len() - 1;
            rows.push(Branch {
                ids,
                decide,
                options: positions,
            });
        }
        Ok((rows, prefix.len()))
    }
}
pub struct Kev {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    encoder: Encoder,
}
impl Kev {
    pub fn load(root: &Path, executable: &Path, metallib: &Path) -> Result<Self, String> {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err(
                "unsupported_runtime: native Kev profile requires macOS Apple Silicon".into(),
            );
        }
        if crate::artifacts::hash(metallib)? != METAL_SHA {
            return Err("model_integrity_error: reviewed MLX metallib SHA256 required".into());
        }
        let prepared = prepare(root)?;
        let encoder = Encoder::load(root)?;
        let mut child = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "unsupported_runtime: build/install optional Kev native sidecar")?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut runtime = Self {
            child,
            input,
            output,
            encoder,
        };
        runtime.send(&json!({"base":root.join("base"),"adapter":root.join("adapter"),"prepared":prepared,"metallib":metallib}))?;
        let startup = runtime.recv()?;
        if startup["ok"] != true {
            return Err("runtime_start_failed: native Kev initialization failed".into());
        }
        Ok(runtime)
    }
    fn send(&mut self, value: &Value) -> Result<(), String> {
        let data =
            serde_json::to_vec(value).map_err(|_| "provider_schema_error: Kev JSON invalid")?;
        self.input
            .write_all(&data)
            .and_then(|_| self.input.write_all(b"\n"))
            .and_then(|_| self.input.flush())
            .map_err(|_| {
                "runtime_unavailable: native Kev IPC failed; restart serve explicitly".into()
            })
    }
    fn recv(&mut self) -> Result<Value, String> {
        let mut bytes = Vec::new();
        self.output
            .by_ref()
            .take(4 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| "runtime_unavailable: native Kev IPC failed; restart serve explicitly")?;
        if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 || bytes.last() != Some(&b'\n') {
            return Err(
                "runtime_unavailable: native Kev stopped or exceeded response bound; restart serve explicitly".into(),
            );
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            "runtime_unavailable: native Kev JSON invalid; restart serve explicitly".into()
        })
    }
    pub fn decide(&mut self, payload: &Value) -> Result<Value, String> {
        let (state, questions) = decisions(payload)?;
        let (rows, state_tokens) = self.encoder.rows(&state, &questions)?;
        self.send(&json!({"rows":rows}))?;
        let reply = self.recv()?;
        let probs = reply["probabilities"]
            .as_array()
            .ok_or("provider_schema_error: native Kev probabilities missing")?;
        if probs.len() != questions.len() {
            return Err("provider_schema_error: native Kev question count mismatch".into());
        }
        let mut results = Map::new();
        for ((name, q), p) in questions.iter().zip(probs) {
            let values = p
                .as_array()
                .ok_or("provider_schema_error: native Kev distribution missing")?
                .iter()
                .map(|v| crate::runtime::number(v, 0.0, 1.0))
                .collect::<Result<Vec<_>, _>>()?;
            let kind = q["type"].as_str().unwrap();
            let keys = match kind {
                "choice" => q["criteria"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                "score" => (0..q["rubric"].as_array().unwrap().len())
                    .map(|i| i.to_string())
                    .collect(),
                _ => vec!["false".into(), "true".into()],
            };
            if values.len() != keys.len() {
                return Err("provider_schema_error: native Kev option count mismatch".into());
            }
            let best = values
                .iter()
                .enumerate()
                .fold(0, |best, (i, p)| if *p > values[best] { i } else { best });
            let result = match kind {
                "choice" => {
                    let distribution = keys
                        .iter()
                        .zip(&values)
                        .map(|(k, p)| (k.clone(), json!(p)))
                        .collect::<Map<_, _>>();
                    let k = keys.len() as f64;
                    json!({"type":kind,"value":keys[best],"probabilities":distribution,"confidence":(values[best]-1.0/k)/(1.0-1.0/k)})
                }
                "score" => {
                    let length = values.len() as f64;
                    let uniform = (0..values.len())
                        .map(|i| (i as f64 - (length - 1.0) / 2.0).abs())
                        .sum::<f64>()
                        / length;
                    let deviation = values
                        .iter()
                        .enumerate()
                        .map(|(i, p)| p * (i as f64 - best as f64).abs())
                        .sum::<f64>();
                    json!({"type":kind,"value":values.iter().enumerate().map(|(i,p)|i as f64*p).sum::<f64>(),"level":best.to_string(),"probabilities":values,"confidence":(1.0-deviation/uniform).max(0.0),"scale":{"minimum":0,"maximum":keys.len()-1}})
                }
                _ => {
                    let p = values[1];
                    let value = if kind == "nullable_boolean" {
                        if p >= q["upper"].as_f64().unwrap() {
                            json!(true)
                        } else if p <= q["lower"].as_f64().unwrap() {
                            json!(false)
                        } else {
                            Value::Null
                        }
                    } else {
                        json!(p >= q["threshold"].as_f64().unwrap())
                    };
                    json!({"type":kind,"value":value,"probability_true":p,"confidence":(2.0*p-1.0).abs(),"abstained":value.is_null()})
                }
            };
            results.insert(name.clone(), checked(result)?);
        }
        Ok(
            json!({"model":"kev-0.8b","results":results,"action":"suggestion_only","canonical_changes":false,"runtime_metrics":{"mlx_process_peak_bytes":reply["mlxProcessPeakBytes"],"swift_process_peak_rss_bytes":reply["processPeakRssBytes"],"memory_scope":"Swift process lifetime; MLX allocation counter is separate from process RSS","state_tokens":state_tokens,"state_truncated":false,"branch_mode":"batched independent full causal rows"}}),
        )
    }
}
impl Drop for Kev {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub fn check_encoding(cache: &Path, output: &Path) -> Result<Value, String> {
    let root = crate::artifacts::verify(cache, &crate::catalog::model("kev-0.8b")?)?;
    let encoder = Encoder::load(&root)?;
    let reference: Value = serde_json::from_str(include_str!(
        "../../../../examples/models/kev-reference-token-ids.json"
    ))
    .map_err(|_| "provider_schema_error: reference encoder fixture invalid")?;
    let mut checked = 0;
    for case in reference["cases"].as_array().unwrap() {
        let payload = json!({"state":case["state"],"questions":case["questions"]});
        let (state, questions) = decisions(&payload)?;
        let (rows, _) = encoder.rows(&state, &questions)?;
        let expected: Vec<Branch> = serde_json::from_value(case["rows"].clone())
            .map_err(|_| "provider_schema_error: reference tokens invalid")?;
        if rows != expected {
            return Err("provider_schema_error: native token IDs or pointer positions differ from the pinned author encoder".into());
        }
        checked += 1;
    }
    let result = json!({"cases":checked,"token_ids_identical":true,"pointer_positions_identical":true,"reference_source":reference["source"],"model_revision":crate::catalog::model("kev-0.8b")?["revision"]});
    fs::write(output, serde_json::to_vec_pretty(&result).unwrap())
        .map_err(|_| "probe_failed: cannot write encoding report")?;
    Ok(result)
}
