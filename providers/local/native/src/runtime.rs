//! Bounded typed contract shared by the native Google engines and the optional Kev sidecar.
use crate::{
    catalog,
    google::{Gemma, Laya},
};
use serde_json::{Map, Value, json};
use std::{collections::HashMap, io::Read, path::PathBuf, time::Instant};
pub fn text(v: &Value, bound: usize) -> Result<&str, String> {
    let s = v.as_str().ok_or("invalid_arguments: text required")?;
    if s.trim().is_empty() || s.len() > bound || s.contains('\0') {
        return Err("invalid_arguments: supply bounded nonempty UTF-8 text without NUL".into());
    }
    Ok(s)
}
pub fn number(v: &Value, lo: f64, hi: f64) -> Result<f64, String> {
    let n = v
        .as_f64()
        .ok_or("invalid_arguments: numeric value required")?;
    if !n.is_finite() || n < lo || n > hi {
        return Err("invalid_arguments: numeric value outside range".into());
    }
    Ok(n)
}
fn object<'a>(v: &'a Value, allowed: &[&str]) -> Result<&'a Map<String, Value>, String> {
    let m = v.as_object().ok_or("invalid_arguments: object required")?;
    if m.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("invalid_arguments: unsupported fields".into());
    }
    Ok(m)
}
pub fn question(mut q: Value) -> Result<Value, String> {
    let kind = q["type"]
        .as_str()
        .ok_or("unsupported_question: typed question required")?
        .to_owned();
    let allowed = match kind.as_str() {
        "choice" => vec!["type", "instructions", "criteria"],
        "score" => vec!["type", "instructions", "rubric"],
        "boolean" => vec!["type", "condition", "threshold"],
        "nullable_boolean" => vec!["type", "condition", "lower", "upper"],
        _ => {
            return Err(
                "unsupported_question: choose choice, boolean, nullable_boolean or score".into(),
            );
        }
    };
    object(&q, &allowed)?;
    match kind.as_str() {
        "choice" => {
            let criteria = q["criteria"]
                .as_object()
                .ok_or("invalid_arguments: choice criteria required")?;
            if !(2..=8).contains(&criteria.len()) {
                return Err("invalid_arguments: choice requires 2–8 options".into());
            }
            for (k, v) in criteria {
                text(&json!(k), 64)?;
                text(v, 512)?;
            }
            if q.get("instructions").is_none() {
                q["instructions"] = json!("Select the best supported option.")
            }
            text(&q["instructions"], 512)?;
        }
        "score" => {
            let rubric = q["rubric"]
                .as_array()
                .ok_or("invalid_arguments: score rubric required")?;
            if !(2..=8).contains(&rubric.len()) {
                return Err("invalid_arguments: score requires 2–8 levels".into());
            }
            for level in rubric {
                text(level, 512)?;
            }
            if q.get("instructions").is_none() {
                q["instructions"] = json!("Evaluate the supplied rubric.")
            }
            text(&q["instructions"], 512)?;
        }
        _ => {
            text(&q["condition"], 512)?;
            if kind == "boolean" {
                if q.get("threshold").is_none() {
                    q["threshold"] = json!(0.5)
                }
                number(&q["threshold"], 0.0, 1.0)?;
            } else {
                if q.get("lower").is_none() {
                    q["lower"] = json!(0.2)
                }
                if q.get("upper").is_none() {
                    q["upper"] = json!(0.8)
                }
                if number(&q["lower"], 0.0, 1.0)? >= number(&q["upper"], 0.0, 1.0)? {
                    return Err("invalid_arguments: lower must be less than upper".into());
                }
            }
        }
    }
    Ok(q)
}
pub fn decisions(payload: &Value) -> Result<(String, Map<String, Value>), String> {
    object(payload, &["model", "state", "questions"])?;
    let state = text(&payload["state"], 8192)?.to_owned();
    let questions = payload["questions"]
        .as_object()
        .ok_or("invalid_arguments: questions required")?;
    if !(1..=8).contains(&questions.len()) {
        return Err("invalid_arguments: supply 1–8 typed questions".into());
    }
    let mut checked = Map::new();
    for (key, q) in questions {
        text(&json!(key), 64)?;
        checked.insert(key.clone(), question(q.clone())?);
    }
    Ok((state, checked))
}
pub fn embeddings(payload: &Value) -> Result<Vec<String>, String> {
    object(
        payload,
        &["model", "input", "encoding_format", "dimensions"],
    )?;
    if payload.get("encoding_format").is_some_and(|v| v != "float")
        || payload.get("dimensions").is_some_and(|v| v != 768)
    {
        return Err("unsupported_format: use 768-dimensional float embeddings".into());
    }
    let rows = if payload["input"].is_string() {
        vec![payload["input"].clone()]
    } else {
        payload["input"]
            .as_array()
            .ok_or("invalid_arguments: input text array required")?
            .clone()
    };
    if !(1..=32).contains(&rows.len()) {
        return Err("invalid_arguments: supply 1–32 text strings".into());
    }
    let mut checked = Vec::new();
    for r in &rows {
        checked.push(text(r, 32768)?.to_owned());
    }
    if checked.iter().map(String::len).sum::<usize>() > 512 * 1024 {
        return Err("context_budget: input exceeds 512 KiB".into());
    }
    Ok(checked)
}
pub fn triage_text(payload: &Value) -> Result<String, String> {
    object(payload, &["model", "messages", "response_format", "stream"])
        .map_err(|_| "unsupported_generation: typed triage only")?;
    let m = payload["messages"]
        .as_array()
        .ok_or("unsupported_generation: expected system/user triage")?;
    if m.len() != 2
        || payload["response_format"] != json!({"type":"json_object"})
        || payload.get("stream").is_some_and(|v| v != false)
    {
        return Err("unsupported_generation: no generation, streaming or tools".into());
    }
    for message in m {
        object(message, &["role", "content"])
            .map_err(|_| "unsupported_generation: fixed messages required")?;
    }
    if m[0]["role"] != "system" || m[1]["role"] != "user" {
        return Err("unsupported_generation: fixed roles required".into());
    }
    let s = text(&m[0]["content"], 4096)?;
    if ["label", "needs_reply", "urgency", "uncertain", "rationale"]
        .iter()
        .any(|k| !s.contains(k))
    {
        return Err("unsupported_generation: typed triage fields required".into());
    }
    let selected: Value = serde_json::from_str(text(&m[1]["content"], 16384)?)
        .map_err(|_| "unsupported_generation: selected JSON text required")?;
    let selected = object(&selected, &["text"])?;
    if selected.len() != 1 {
        return Err("unsupported_generation: one selected text required".into());
    }
    Ok(text(&selected["text"], 8192)?.into())
}
pub fn triage_questions() -> Value {
    json!({"label":{"type":"choice","instructions":"Classify the message's main purpose.","criteria":{"receipt":"Acknowledges receiving a job application.","rejection":"Rejects the candidate or ends the hiring process.","scheduling":"Requests interview availability or choosing a meeting time.","assessment":"Requests completing a hiring test or assessment.","other":"Other or unclear hiring message."}},"needs_reply":{"type":"nullable_boolean","condition":"The message requests a reply, availability, booking or completing a task."},"urgency":{"type":"choice","instructions":"Choose the review priority.","criteria":{"normal":"Informational update with no requested action.","review":"Requests a reply or action without an immediate deadline.","urgent":"Action required today or within the next twenty-four hours."}}})
}
pub fn consistency(state: &str, r: &Value) -> Value {
    let label = r["label"]["value"].as_str().unwrap_or("other");
    let original = &r["needs_reply"]["value"];
    let lower = state.to_lowercase();
    let clean = lower
        .split_whitespace()
        .filter(|w| !w.contains("://"))
        .collect::<Vec<_>>()
        .join(" ");
    let request = [
        "please reply",
        "let me know",
        "availability",
        "choose a time",
        "pick a slot",
        "booking link",
        "complete the test",
        "complete the assessment",
        "schedule a call",
        "are you available",
    ]
    .iter()
    .any(|term| clean.contains(term));
    let action = matches!(label, "scheduling" | "assessment");
    let reply = if request || action {
        json!(true)
    } else {
        original.clone()
    };
    let mut urgency = r["urgency"]["value"]
        .as_str()
        .unwrap_or("review")
        .to_owned();
    let uncertain = r["needs_reply"]["abstained"] == true
        || r["label"]["confidence"].as_f64().unwrap_or(0.0) < 0.75
        || r["urgency"]["confidence"].as_f64().unwrap_or(0.0) < 0.75
        || &reply != original
        || (reply == true && !action)
        || (reply == true && urgency == "normal");
    if reply == true && urgency == "normal" {
        urgency = "review".into()
    }
    json!({"label":label,"needs_reply":reply,"urgency":urgency,"uncertain":uncertain,"rationale":"Typed model proposal; inspect the source before acting.","probabilities":r,"request_guard":request})
}
pub fn checked(r: Value) -> Result<Value, String> {
    let validate = (|| {
        number(&r["confidence"], 0.0, 1.0)?;
        if r.get("probability_true").is_some() {
            number(&r["probability_true"], 0.0, 1.0)?;
        }
        if let Some(v) = r.get("probabilities") {
            let values: Vec<_> = if let Some(m) = v.as_object() {
                m.values().collect()
            } else {
                v.as_array()
                    .ok_or("provider_schema_error")?
                    .iter()
                    .collect()
            };
            let total = values
                .iter()
                .map(|n| number(n, 0.0, 1.0))
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .sum::<f64>();
            if values.is_empty() || (total - 1.0).abs() > 0.001 {
                return Err("provider_schema_error".into());
            }
        }
        if r["type"] == "score" {
            number(
                &r["value"],
                0.0,
                r["scale"]["maximum"].as_f64().unwrap_or(-1.0),
            )?;
        }
        Ok::<_, String>(())
    })();
    validate
        .map_err(|_| "provider_schema_error: native result violates typed contract".to_string())?;
    Ok(r)
}
pub struct Runtime {
    gemma: Option<Gemma>,
    laya: Option<Laya>,
    kev: Option<crate::kev::Kev>,
    decision_id: String,
    tokenizer: Option<tokenizers::Tokenizer>,
    pub load_ms: Map<String, Value>,
}
impl Runtime {
    pub fn load(paths: &HashMap<String, PathBuf>, threads: i32) -> Result<Self, String> {
        let mut r = Self {
            gemma: None,
            laya: None,
            kev: None,
            decision_id: "laya-s256".into(),
            tokenizer: None,
            load_ms: Map::new(),
        };
        if let Some(path) = paths.get("embeddinggemma2-text") {
            let start = Instant::now();
            r.gemma = Some(Gemma::load(
                paths
                    .get("litert-lm")
                    .ok_or("unsupported_runtime: native embedding library required")?,
                path,
                threads,
            )?);
            r.load_ms.insert(
                "embeddinggemma2-text".into(),
                json!(start.elapsed().as_secs_f64() * 1000.0),
            );
        }
        if let Some(path) = paths.get("laya-s256") {
            let start = Instant::now();
            r.laya = Some(Laya::load(
                paths
                    .get("mediapipe")
                    .ok_or("unsupported_runtime: native decision library required")?,
                path,
            )?);
            let mut zip = zip::ZipArchive::new(
                std::fs::File::open(path)
                    .map_err(|_| "model_integrity_error: tokenizer bundle missing")?,
            )
            .map_err(|_| "model_integrity_error: tokenizer bundle invalid")?;
            let mut file = zip
                .by_name("tokenizer.json")
                .map_err(|_| "model_integrity_error: tokenizer missing")?;
            if file.size() > 40 * 1024 * 1024 {
                return Err("model_integrity_error: unexpected tokenizer size".into());
            }
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|_| "model_integrity_error: tokenizer read failed")?;
            let mut tok = tokenizers::Tokenizer::from_bytes(&bytes)
                .map_err(|_| "model_integrity_error: tokenizer invalid")?;
            tok.with_truncation(None)
                .map_err(|_| "model_integrity_error: tokenizer limits invalid")?;
            tok.with_padding(None);
            r.tokenizer = Some(tok);
            r.load_ms.insert(
                "laya-s256".into(),
                json!(start.elapsed().as_secs_f64() * 1000.0),
            );
        }
        if let Some(root) = paths.get("kev-0.8b") {
            let start = Instant::now();
            r.kev = Some(crate::kev::Kev::load(
                root,
                paths
                    .get("kev-native")
                    .ok_or("unsupported_runtime: Kev sidecar required")?,
                paths
                    .get("kev-metallib")
                    .ok_or("unsupported_runtime: MLX metallib required")?,
            )?);
            r.decision_id = "kev-0.8b".into();
            r.load_ms.insert(
                "kev-0.8b".into(),
                json!(start.elapsed().as_secs_f64() * 1000.0),
            );
        }
        Ok(r)
    }
    fn question(&mut self, state: &str, q: &Value) -> Result<Value, String> {
        let estimate = self
            .tokenizer
            .as_ref()
            .ok_or("model_not_loaded: select explicit decision model")?
            .encode(
                format!("{state} {}", serde_json::to_string(q).unwrap()),
                true,
            )
            .map_err(|_| "invalid_arguments: cannot tokenize selected input")?
            .len()
            + 32;
        if estimate > 256 {
            return Err("context_budget: Laya conservative 256 token budget exceeded".into());
        }
        let l = self
            .laya
            .as_mut()
            .ok_or("model_not_loaded: select explicit decision model")?;
        let kind = q["type"].as_str().unwrap();
        let result = match kind {
            "choice" => {
                let criteria = q["criteria"].as_object().unwrap();
                let keys = criteria.keys().cloned().collect::<Vec<_>>();
                let descs = criteria
                    .values()
                    .map(|v| v.as_str().unwrap().into())
                    .collect::<Vec<_>>();
                let r = l.choice(state, &keys, &descs, q["instructions"].as_str().unwrap())?;
                if !keys.contains(&r.value)
                    || r.probabilities.len() != keys.len()
                    || r.probabilities.iter().any(|(k, _)| !keys.contains(k))
                {
                    return Err("provider_schema_error: native option keys mismatch".into());
                }
                let probs = r
                    .probabilities
                    .iter()
                    .map(|(k, p)| (k.clone(), json!(p)))
                    .collect::<Map<_, _>>();
                json!({"type":kind,"value":r.value,"probabilities":probs,"confidence":r.confidence})
            }
            "boolean" | "nullable_boolean" => {
                let r = l.boolean(
                    state,
                    q["condition"].as_str().unwrap(),
                    q.get("threshold").and_then(Value::as_f64).unwrap_or(0.5) as f32,
                )?;
                let p = r.probability_true as f64;
                let value = if kind == "boolean" {
                    json!(r.value)
                } else if p >= q["upper"].as_f64().unwrap() {
                    json!(true)
                } else if p <= q["lower"].as_f64().unwrap() {
                    json!(false)
                } else {
                    Value::Null
                };
                json!({"type":kind,"value":value,"probability_true":p,"confidence":r.confidence,"abstained":value.is_null()})
            }
            _ => {
                let rubric = q["rubric"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect::<Vec<_>>();
                let r = l.score(state, &rubric, q["instructions"].as_str().unwrap())?;
                if r.probabilities.len() != rubric.len()
                    || (r
                        .probabilities
                        .iter()
                        .enumerate()
                        .map(|(i, p)| i as f64 * *p as f64)
                        .sum::<f64>()
                        - r.value as f64)
                        .abs()
                        > 0.001
                {
                    return Err("provider_schema_error: rubric scale mismatch".into());
                }
                json!({"type":kind,"value":r.value,"level":r.level,"probabilities":r.probabilities,"confidence":r.confidence,"scale":{"minimum":0,"maximum":rubric.len()-1}})
            }
        };
        checked(result)
    }
    fn decide(&mut self, payload: &Value) -> Result<Value, String> {
        if let Some(kev) = self.kev.as_mut() {
            return kev.decide(payload);
        }
        let (state, questions) = decisions(payload)?;
        let mut results = Map::new();
        for (key, q) in questions {
            results.insert(key, self.question(&state, &q)?);
        }
        Ok(
            json!({"model":"laya-s256","results":results,"action":"suggestion_only","canonical_changes":false}),
        )
    }
    pub fn call(&mut self, route: &str, payload: Value) -> Result<Value, String> {
        let expected = if route == "/v1/embeddings" {
            "embeddinggemma2-text".to_owned()
        } else {
            self.decision_id.clone()
        };
        if payload["model"] != expected {
            return Err("unknown_model: request explicitlyselectedmodel".into());
        }
        let start = Instant::now();
        let mut result = match route {
            "/v1/embeddings" => {
                let rows = embeddings(&payload)?;
                let gemma = self
                    .gemma
                    .as_mut()
                    .ok_or("model_not_loaded: select explicitembeddingmodel")?;
                let mut data = Vec::new();
                for (i, row) in rows.iter().enumerate() {
                    let vector = gemma.embed(row)?;
                    if vector.iter().any(|n| !n.is_finite())
                        || (vector.iter().map(|n| n * n).sum::<f32>() - 1.0).abs() > 0.01
                    {
                        return Err("provider_schema_error: native embedding invalid".into());
                    }
                    data.push(json!({"object":"embedding","index":i,"embedding":vector}));
                }
                json!({"object":"list","model":expected,"data":data})
            }
            "/v1/decisions" => self.decide(&payload)?,
            "/v1/chat/completions" => {
                let state = triage_text(&payload)?;
                let raw = self.decide(&json!({"state":state,"questions":triage_questions()}))?;
                let content = serde_json::to_string(&consistency(&state, &raw["results"])).unwrap();
                json!({"object":"chat.completion","model":expected,"choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":"stop"}]})
            }
            _ => return Err("unsupported_route: embeddings or typed decisions only".into()),
        };
        result["model_revision"] = catalog::model(&expected)?["revision"].clone();
        result["metrics"] = json!({"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"worker_peak_rss_bytes":peak_rss()});
        Ok(result)
    }
}
pub fn peak_rss() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } == 0 {
            let bytes = unsafe { usage.assume_init() }.ru_maxrss as u64;
            return Some(if cfg!(target_os = "macos") {
                bytes
            } else {
                bytes * 1024
            });
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_question_fields_and_numbers() {
        assert!(question(json!({"type":"nullable_boolean","condition":"Action requested?","lower":0.8,"upper":0.2})).is_err());
        assert!(question(json!({"type":"boolean","condition":"A","threshold":true})).is_err());
        assert!(question(json!({"type":"choice","criteria":{"a":"A"},"tools":[]})).is_err());
        assert!(question(json!({"type":"generate","condition":"A"})).is_err());
        assert!(question(json!({"type":"boolean","condition":"A"})).is_ok());
    }
    #[test]
    fn embedding_count_format_and_text_bounds() {
        assert!(embeddings(&json!({"input":["a"],"dimensions":768})).is_ok());
        assert!(embeddings(&json!({"input":["a"],"dimensions":true})).is_err());
        assert!(embeddings(&json!({"input":vec!["a";33]})).is_err());
        assert!(embeddings(&json!({"input":"a\u{0000}"})).is_err());
        assert!(embeddings(&json!({"input":"a","tools":[]})).is_err());
    }
    #[test]
    fn freeform_generation_and_tools_rejected() {
        assert!(
            triage_text(&json!({"messages":[{"role":"user","content":"Write my CV"}]})).is_err()
        );
        assert!(triage_text(&json!({"messages":[],"tools":[]})).is_err());
    }
    #[test]
    fn missing_knowledge_preserves_abstention() {
        let raw = json!({"label":{"value":"other","confidence":0.9},"urgency":{"value":"normal","confidence":0.9},"needs_reply":{"value":null,"abstained":true}});
        let value = consistency("Only the subject is available.", &raw);
        assert_eq!(value["needs_reply"], Value::Null);
        assert_eq!(value["uncertain"], true);
    }
    #[test]
    fn request_guard_independent_of_wrong_label() {
        let raw = json!({"label":{"value":"receipt","confidence":0.9},"urgency":{"value":"normal","confidence":0.9},"needs_reply":{"value":false,"abstained":false}});
        let value = consistency(
            "We received your application. Please reply with interview availability.",
            &raw,
        );
        assert_eq!(value["needs_reply"], true);
        assert_eq!(value["uncertain"], true);
        assert_eq!(value["urgency"], "review");
        assert_eq!(value["label"], "receipt");
        assert_eq!(
            consistency("Visit https://example.test/availability?x=yes", &raw)["request_guard"],
            false
        );
    }
    #[test]
    fn invalid_native_probabilities_rejected() {
        assert!(
            checked(json!({"type":"choice","confidence":0.9,"probabilities":{"a":0.6,"b":0.6}}))
                .is_err()
        );
        assert!(
            checked(json!({"type":"boolean","confidence":-1.0,"probability_true":0.5})).is_err()
        );
    }
}
