//! Exact-source intake and explicit review plans. Only the engine commits these changes.
use crate::{HarnessError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

const MAX_TEXT: usize = 2 * 1024 * 1024;
const HOME: &str = "context/intake";
#[derive(Debug, Clone)]
pub struct Plan {
    pub data: Value,
    pub changes: Vec<Value>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    PersonalNotes,
    Transcript,
    ProviderSummary,
    GeneratedSummary,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub kind: String,
    pub path: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    text: String,
    source_kind: SourceKind,
    #[serde(default)]
    source_label: Option<String>,
    #[serde(default)]
    source_uri: Option<String>,
    #[serde(default)]
    occurred_at: Option<String>,
    #[serde(default)]
    occurred_period: Option<String>,
    #[serde(default)]
    valid_from: Option<String>,
    #[serde(default)]
    valid_to: Option<String>,
    #[serde(default)]
    supersedes: Option<String>,
    #[serde(default)]
    source_group: Option<String>,
    #[serde(default)]
    speakers: Option<Vec<String>>,
    #[serde(default)]
    links: Vec<Link>,
    #[serde(skip_serializing)]
    idempotency_key: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub path: String,
    pub sha256: String,
    pub start_line: usize,
    pub end_line: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub citations: Vec<Citation>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    note_id: String,
    expected_source_sha256: String,
    items: Vec<Item>,
    #[serde(default)]
    questions: Vec<String>,
    idempotency_key: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    note_id: String,
    #[serde(default)]
    proposal_id: Option<String>,
    #[serde(default)]
    include_original: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Accept {
    note_id: String,
    proposal_id: String,
    expected_proposal_sha256: String,
    selected_item_ids: Vec<String>,
    approved_by: String,
    approval_evidence: String,
    idempotency_key: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    note_id: String,
    expected_metadata_sha256: String,
    reason: String,
    evidence: String,
    idempotency_key: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Listing {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    include_archived: bool,
}
fn default_limit() -> usize {
    50
}
fn fail(code: &str, message: impl Into<String>) -> HarnessError {
    HarnessError::new(code, message)
}
fn stamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}
fn hash(bytes: &[u8]) -> String {
    crate::storage::digest(bytes)
}
fn encoded(value: &Value) -> Vec<u8> {
    job_harness_domain::canonical::encoded(value)
}
fn parse<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T> {
    serde_json::from_value(args.clone()).map_err(|e| fail("invalid_arguments", e.to_string()))
}
fn text(value: &str, label: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max {
        return Err(fail(
            "invalid_arguments",
            format!("{label} must be nonempty and within its size limit"),
        ));
    }
    Ok(())
}
fn sha(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(fail(
            "invalid_arguments",
            "Expected a lowercase SHA-256 digest",
        ));
    }
    Ok(())
}
fn identifier(id: &str, prefix: &str) -> Result<()> {
    if !id.strip_prefix(prefix).is_some_and(|tail| {
        tail.len() == 24
            && tail
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) {
        return Err(fail("invalid_arguments", "Invalid intake identifier"));
    }
    Ok(())
}
fn note_home(id: &str) -> Result<String> {
    identifier(id, "note-")?;
    Ok(format!("{HOME}/{id}"))
}
fn read(root: &Path, path: &str) -> Result<Vec<u8>> {
    let path = crate::workspace::safe_path(root, path)?;
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() || metadata.len() > MAX_TEXT as u64 * 4 {
        return Err(fail(
            "invalid_resource",
            "Intake text resource exceeds its limit or is not a file",
        ));
    }
    Ok(fs::read(path)?)
}
fn json_read(root: &Path, path: &str) -> Result<(Value, String)> {
    let bytes = read(root, path)?;
    let value = serde_json::from_slice(&bytes)?;
    Ok((value, hash(&bytes)))
}
fn changes(root: &Path, files: Vec<(String, String)>, immutable: bool) -> Result<Vec<Value>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (path, text) in files {
        if !seen.insert(path.clone()) {
            return Err(fail("invalid_arguments", "Duplicate intake change path"));
        }
        let target = crate::workspace::safe_path(root, &path)?;
        let before = if target.exists() {
            let bytes = read(root, &path)?;
            if immutable {
                if bytes != text.as_bytes() {
                    return Err(fail(
                        "immutable_conflict",
                        "Existing intake artifact differs; originals and review artifacts are immutable",
                    ));
                }
                continue;
            }
            Some(hash(&bytes))
        } else {
            None
        };
        out.push(json!({"path":path,"text":text,"before_sha256":before}));
    }
    Ok(out)
}
fn as_text(value: &Value) -> String {
    String::from_utf8(encoded(value)).expect("Canonical JSON is UTF-8")
}
fn metadata(root: &Path, id: &str) -> Result<(Value, String)> {
    let home = note_home(id)?;
    let (meta, digest) = json_read(root, &format!("{home}/metadata.json"))?;
    if meta["schema_version"] != 1
        || meta["note_id"] != id
        || meta["original"]["path"] != format!("{home}/original.md")
        || meta["archived"].as_bool().is_none()
        || serde_json::from_value::<SourceKind>(meta["source_kind"].clone()).is_err()
    {
        return Err(fail(
            "invalid_resource",
            "Unsupported or malformed intake metadata",
        ));
    }
    sha(meta["original"]["sha256"]
        .as_str()
        .ok_or_else(|| fail("invalid_resource", "Missing intake source digest"))?)?;
    Ok((meta, digest))
}
fn original(root: &Path, meta: &Value) -> Result<(String, String)> {
    let bytes = read(
        root,
        meta["original"]["path"]
            .as_str()
            .ok_or_else(|| fail("invalid_resource", "Missing original path"))?,
    )?;
    let digest = hash(&bytes);
    let content = String::from_utf8(bytes)
        .map_err(|_| fail("invalid_resource", "Intake original is not UTF-8"))?;
    Ok((content, digest))
}
fn fresh(root: &Path, id: &str) -> Result<(Value, String, String)> {
    let (meta, metadata_sha) = metadata(root, id)?;
    let (_, current) = original(root, &meta)?;
    if meta["original"]["sha256"] != current {
        return Err(fail(
            "source_changed",
            "The exact original changed; create a new intake instead of overwriting its provenance",
        ));
    }
    if meta["archived"] == true {
        return Err(fail(
            "archived_note",
            "Archived intake is retained; import a new note for new work",
        ));
    }
    Ok((meta, metadata_sha, current))
}
fn check_links(root: &Path, links: &[Link]) -> Result<()> {
    if links.len() > 40 {
        return Err(fail("invalid_arguments", "Too many note links"));
    }
    let mut seen = BTreeSet::new();
    for link in links {
        if !seen.insert((link.kind.clone(), link.path.clone())) {
            return Err(fail("invalid_arguments", "Duplicate note links"));
        }
        let allowed = match link.kind.as_str() {
            "application" => link.path.strip_prefix("applications/").is_some_and(|p| {
                !p.contains('/') && crate::pipeline::is_vacancy_filename(p) && p.ends_with(".md")
            }),
            "project" => link.path.starts_with("context/projects/") && link.path.ends_with(".md"),
            _ => false,
        };
        if !allowed || !crate::workspace::safe_path(root, &link.path)?.is_file() {
            return Err(fail(
                "missing_resource",
                "Note links must identify existing canonical application/project Markdown",
            ));
        }
    }
    Ok(())
}
fn citation(root: &Path, c: &Citation) -> Result<Value> {
    sha(&c.sha256)?;
    crate::workspace::validate_relative_data(
        &c.path,
        &["context", "applications", "sources"],
        None,
    )?;
    if ![Some("md"), Some("json"), Some("txt")]
        .contains(&Path::new(&c.path).extension().and_then(|x| x.to_str()))
        || !crate::bundle::allowed_path(&c.path)
        || c.path.starts_with("applications/operations/")
    {
        return Err(fail(
            "invalid_arguments",
            "Citations must refer to canonical text evidence, never credentials or coordinator state",
        ));
    }
    let bytes = read(root, &c.path)?;
    if hash(&bytes) != c.sha256 {
        return Err(fail(
            "source_changed",
            format!("Citation source changed: {}", c.path),
        ));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| fail("invalid_resource", "Citation is not UTF-8 text"))?;
    let lines = content.lines().collect::<Vec<_>>();
    if c.start_line == 0 || c.end_line < c.start_line || c.end_line > lines.len() {
        return Err(fail(
            "invalid_arguments",
            "Citation line bounds are outside the exact source",
        ));
    }
    let full = lines[c.start_line - 1..c.end_line].join("\n");
    let end = full
        .char_indices()
        .map(|(offset, _)| offset)
        .take_while(|offset| *offset <= 2000)
        .last()
        .unwrap_or(0);
    let excerpt = if full.len() <= 2000 {
        full.clone()
    } else {
        full[..end].to_owned()
    };
    Ok(
        json!({"path":c.path,"sha256":c.sha256,"start_line":c.start_line,"end_line":c.end_line,"excerpt_truncated":full.len()>2000,"excerpt":excerpt}),
    )
}

pub fn record(root: &Path, args: &Value) -> Result<Plan> {
    let request: Record = parse(args)?;
    text(&request.idempotency_key, "Idempotency key", 20000)?;
    text(&request.text, "Original text", MAX_TEXT)?;
    if let Some(label) = &request.source_label {
        text(label, "Source label", 2000)?;
    }
    if let Some(uri) = &request.source_uri {
        text(uri, "Source URI", 4000)?;
        let parsed = url::Url::parse(uri)
            .map_err(|_| fail("invalid_arguments", "Source URI must be an HTTP(S) URL"))?;
        if !["http", "https"].contains(&parsed.scheme())
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(fail(
                "invalid_arguments",
                "Source URI must not contain credentials",
            ));
        }
    }
    if let Some(date) = &request.occurred_at {
        crate::domain::iso_datetime(date)?;
    }
    for (label, value) in [
        ("Occurred period", &request.occurred_period),
        ("Valid from", &request.valid_from),
        ("Valid to", &request.valid_to),
        ("Source group", &request.source_group),
    ] {
        if let Some(value) = value {
            text(value, label, 300)?;
        }
    }
    if let Some(prior) = &request.supersedes {
        metadata(root, prior)?;
    }

    if let Some(speakers) = &request.speakers {
        if speakers.len() > 100 {
            return Err(fail("invalid_arguments", "Too many supplied speakers"));
        }
        for speaker in speakers {
            text(speaker, "Speaker", 300)?;
        }
    }
    check_links(root, &request.links)?;
    // Optional fields added later cannot change IDs for earlier, unchanged request shapes.
    let mut identity = args.clone();
    identity
        .as_object_mut()
        .ok_or_else(|| fail("invalid_arguments", "Intake arguments must be an object"))?
        .remove("idempotency_key");
    let request_sha = hash(&encoded(&identity));
    let id = format!("note-{}", &request_sha[..24]);
    let home = note_home(&id)?;
    let original_path = format!("{home}/original.md");
    let metadata_path = format!("{home}/metadata.json");
    let source_sha = hash(request.text.as_bytes());
    let path = crate::workspace::safe_path(root, &metadata_path)?;
    if path.exists() {
        let (meta, metadata_sha) = metadata(root, &id)?;
        let (_, actual) = original(root, &meta)?;
        if meta["request_sha256"] != request_sha || actual != source_sha {
            return Err(fail(
                "immutable_conflict",
                "Existing intake content or provenance differs",
            ));
        }
        return Ok(Plan {
            data: json!({"note_id":id,"source_kind":meta["source_kind"],"original_path":original_path,"source_sha256":source_sha,"metadata_path":metadata_path,"metadata_sha256":metadata_sha,"created":false,"replayed":true,"archived":meta["archived"]}),
            changes: vec![],
        });
    }
    let meta = json!({"schema_version":1,"note_id":id,"request_sha256":request_sha,"source_kind":request.source_kind,"source_label":request.source_label,"source_uri":request.source_uri,"occurred_at":request.occurred_at,"occurred_period":request.occurred_period,"valid_from":request.valid_from,"valid_to":request.valid_to,"supersedes":request.supersedes,"source_group":request.source_group,"speakers":request.speakers,"links":request.links,"recorded_at":stamp(),"archived":false,"archive":null,"original":{"path":original_path,"sha256":source_sha,"bytes":request.text.len(),"line_count":request.text.lines().count()},"note":"Exact supplied text. Source kind and supplied attribution remain distinct from verified career facts."});
    let metadata_text = as_text(&meta);
    let metadata_sha = hash(metadata_text.as_bytes());
    let changes = changes(
        root,
        vec![
            (original_path.clone(), request.text),
            (metadata_path.clone(), metadata_text),
        ],
        true,
    )?;
    Ok(Plan {
        data: json!({"note_id":id,"source_kind":meta["source_kind"],"original_path":original_path,"source_sha256":source_sha,"metadata_path":metadata_path,"metadata_sha256":metadata_sha,"created":true,"replayed":false,"archived":false,"next":["Review the original; draft a separately cited proposal. Unknown dates and speakers remain unknown."]}),
        changes,
    })
}

pub fn propose(root: &Path, args: &Value) -> Result<Plan> {
    let request: Proposal = parse(args)?;
    text(&request.idempotency_key, "Idempotency key", 20000)?;
    sha(&request.expected_source_sha256)?;
    let (_, metadata_sha, current) = fresh(root, &request.note_id)?;
    if current != request.expected_source_sha256 {
        return Err(fail(
            "source_changed",
            "The original differs from the proposal's expected source",
        ));
    }
    if request.items.is_empty() || request.items.len() > 100 || request.questions.len() > 40 {
        return Err(fail(
            "invalid_arguments",
            "A proposal needs 1–100 items and at most 40 questions",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut items = Vec::new();
    for item in request.items {
        if item.id.is_empty()
            || item.id.len() > 80
            || !item
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || !ids.insert(item.id.clone())
        {
            return Err(fail(
                "invalid_arguments",
                "Proposal item IDs must be unique simple identifiers",
            ));
        }
        if !["summary", "addition", "next_step"].contains(&item.kind.as_str()) {
            return Err(fail("invalid_arguments", "Unknown proposal item kind"));
        }
        text(&item.text, "Proposal item", 20000)?;
        if item.citations.is_empty() || item.citations.len() > 20 {
            return Err(fail(
                "invalid_arguments",
                "Every proposal item needs 1–20 exact-source citations",
            ));
        }
        let citations = item
            .citations
            .iter()
            .map(|c| citation(root, c))
            .collect::<Result<Vec<_>>>()?;
        items.push(json!({"id":item.id,"kind":item.kind,"text":item.text,"citations":citations}));
    }
    for question in &request.questions {
        text(question, "Unresolved question", 3000)?;
    }
    let content = json!({"note_id":request.note_id,"source_sha256":current,"source_metadata_sha256":metadata_sha,"items":items,"questions":request.questions});
    let identity = hash(&encoded(&content));
    let id = format!("proposal-{}", &identity[..24]);
    let path = format!("{}/proposals/{id}.json", note_home(&request.note_id)?);
    let target = crate::workspace::safe_path(root, &path)?;
    if target.exists() {
        let (existing, digest) = json_read(root, &path)?;
        if existing["identity_sha256"] != identity {
            return Err(fail(
                "immutable_conflict",
                "Existing proposal identity differs",
            ));
        }
        return Ok(Plan {
            data: json!({"note_id":request.note_id,"proposal_id":id,"proposal_path":path,"proposal_sha256":digest,"item_count":items.len(),"created":false,"replayed":true}),
            changes: vec![],
        });
    }
    let mut proposal = content;
    proposal["schema_version"] = json!(1);
    proposal["proposal_id"] = json!(id);
    proposal["identity_sha256"] = json!(identity);
    proposal["created_at"] = json!(stamp());
    proposal["status"] = json!("proposed");
    proposal["note"] = json!(
        "Citations verify location and freshness, not semantic entailment. Items remain proposals until explicit review."
    );
    let serialized = as_text(&proposal);
    if serialized.len() > MAX_TEXT * 2 {
        return Err(fail(
            "invalid_arguments",
            "Proposal exceeds 4 MiB; split it into smaller reviewable proposals",
        ));
    }
    let digest = hash(serialized.as_bytes());
    let changes = changes(root, vec![(path.clone(), serialized)], true)?;
    Ok(Plan {
        data: json!({"note_id":request.note_id,"proposal_id":id,"proposal_path":path,"proposal_sha256":digest,"item_count":items.len(),"created":true,"replayed":false,"next":["Review all citations and select exact item IDs. No facts, CV, application stage or external action changed."]}),
        changes,
    })
}

fn proposals(root: &Path, id: &str) -> Result<Vec<Value>> {
    let home = note_home(id)?;
    let directory = crate::workspace::safe_path(root, &format!("{home}/proposals"))?;
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut rows = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let filename = entry
            .file_name()
            .into_string()
            .map_err(|_| fail("invalid_resource", "Invalid proposal filename"))?;
        let prop_id = filename
            .strip_suffix(".json")
            .ok_or_else(|| fail("invalid_resource", "Invalid proposal filename"))?;
        identifier(prop_id, "proposal-")?;
        let path = format!("{home}/proposals/{filename}");
        let (proposal, sha) = json_read(root, &path)?;
        if proposal["note_id"] != id || proposal["proposal_id"] != prop_id {
            return Err(fail(
                "invalid_resource",
                "Proposal references a different intake",
            ));
        }
        rows.push(json!({"proposal_id":prop_id,"sha256":sha,"path":path,"item_count":proposal["items"].as_array().map_or(0,Vec::len),"created_at":proposal["created_at"]}));
    }
    rows.sort_by(|a, b| b["created_at"].as_str().cmp(&a["created_at"].as_str()));
    Ok(rows)
}
fn accepted(root: &Path, id: &str) -> Result<Vec<Value>> {
    let home = note_home(id)?;
    let directory = crate::workspace::safe_path(root, &format!("{home}/reviews"))?;
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut rows = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| fail("invalid_resource", "Invalid review filename"))?;
        identifier(&id, "review-")?;
        let (path, _) = json_read(root, &format!("{home}/reviews/{id}/metadata.json"))?;
        rows.push(path);
    }
    rows.sort_by(|a, b| b["approved_at"].as_str().cmp(&a["approved_at"].as_str()));
    Ok(rows)
}
fn proposal(root: &Path, id: &str, prop_id: &str) -> Result<(Value, String)> {
    identifier(prop_id, "proposal-")?;
    let path = format!("{}/proposals/{prop_id}.json", note_home(id)?);
    let (value, sha) = json_read(root, &path)?;
    if value["schema_version"] != 1
        || value["note_id"] != id
        || value["proposal_id"] != prop_id
        || !value["items"].is_array()
    {
        return Err(fail("invalid_resource", "Malformed proposal"));
    }
    Ok((value, sha))
}
pub fn review(root: &Path, args: &Value) -> Result<Plan> {
    let request: Review = parse(args)?;
    let (meta, metadata_sha) = metadata(root, &request.note_id)?;
    let (content, current) = original(root, &meta)?;
    let source_fresh = meta["original"]["sha256"] == current;
    let mut issues = Vec::new();
    if !source_fresh {
        issues.push("original_changed".to_owned());
    }
    let selected = if let Some(id) = &request.proposal_id {
        let (prop, sha) = proposal(root, &request.note_id, id)?;
        if prop["source_sha256"] != current || prop["source_metadata_sha256"] != metadata_sha {
            issues.push("proposal_source_changed".into());
        }
        for item in prop["items"].as_array().expect("checked") {
            for c in item["citations"]
                .as_array()
                .ok_or_else(|| fail("invalid_resource", "Proposal citation list is invalid"))?
            {
                let value: Citation = parse(
                    &json!({"path":c["path"],"sha256":c["sha256"],"start_line":c["start_line"],"end_line":c["end_line"]}),
                )?;
                if let Err(error) = citation(root, &value) {
                    issues.push(format!("{}: {}", error.code, value.path));
                }
            }
        }
        let mut prop = prop;
        prop["sha256"] = json!(sha);
        prop
    } else {
        Value::Null
    };
    let mut original = json!({"path":meta["original"]["path"],"sha256":current,"bytes":content.len(),"line_count":content.lines().count()});
    if request.include_original {
        original["text"] = json!(content);
    }
    Ok(Plan {
        data: json!({"note_id":request.note_id,"metadata":meta,"metadata_sha256":metadata_sha,"original":original,"source_fresh":source_fresh,"issues":issues,"proposals":proposals(root,&request.note_id)?,"proposal":selected,"accepted_reviews":accepted(root,&request.note_id)?,"archived":meta["archived"]}),
        changes: vec![],
    })
}

pub fn accept(root: &Path, args: &Value) -> Result<Plan> {
    let request: Accept = parse(args)?;
    text(&request.idempotency_key, "Idempotency key", 20000)?;
    text(&request.approved_by, "Approval attribution", 2000)?;
    text(&request.approval_evidence, "Approval evidence", 20000)?;
    sha(&request.expected_proposal_sha256)?;
    let (meta, metadata_sha, current) = fresh(root, &request.note_id)?;
    let (prop, actual) = proposal(root, &request.note_id, &request.proposal_id)?;
    if actual != request.expected_proposal_sha256 {
        return Err(fail(
            "stale_proposal",
            "Proposal changed since review; inspect the current revision",
        ));
    }
    if prop["source_sha256"] != current || prop["source_metadata_sha256"] != metadata_sha {
        return Err(fail(
            "source_changed",
            "Proposal provenance changed since review",
        ));
    }
    if request.selected_item_ids.is_empty() || request.selected_item_ids.len() > 100 {
        return Err(fail(
            "invalid_arguments",
            "Explicitly select 1–100 proposal item IDs",
        ));
    }
    let selected = request
        .selected_item_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if selected.len() != request.selected_item_ids.len() {
        return Err(fail(
            "invalid_arguments",
            "Selected item IDs must be unique",
        ));
    }
    let items = prop["items"]
        .as_array()
        .expect("checked")
        .iter()
        .filter(|item| item["id"].as_str().is_some_and(|id| selected.contains(id)))
        .cloned()
        .collect::<Vec<_>>();
    if items.len() != selected.len() {
        return Err(fail(
            "invalid_arguments",
            "Selected item does not exist in this proposal",
        ));
    }
    for item in &items {
        for c in item["citations"]
            .as_array()
            .ok_or_else(|| fail("invalid_resource", "Missing proposal citations"))?
        {
            let parsed: Citation = parse(
                &json!({"path":c["path"],"sha256":c["sha256"],"start_line":c["start_line"],"end_line":c["end_line"]}),
            )?;
            citation(root, &parsed)?;
        }
    }
    let payload = json!({"note_id":request.note_id,"proposal_id":request.proposal_id,"proposal_sha256":actual,"selected_item_ids":selected,"approved_by":request.approved_by,"approval_evidence":request.approval_evidence});
    let identity = hash(&encoded(&payload));
    let id = format!("review-{}", &identity[..24]);
    let home = format!("{}/reviews/{id}", note_home(&request.note_id)?);
    let accepted_path = format!("{home}/accepted.md");
    let metadata_path = format!("{home}/metadata.json");
    if crate::workspace::safe_path(root, &metadata_path)?.exists() {
        let (existing, _) = json_read(root, &metadata_path)?;
        if existing["identity_sha256"] != identity {
            return Err(fail(
                "immutable_conflict",
                "Existing review identity differs",
            ));
        }
        return Ok(Plan {
            data: json!({"note_id":request.note_id,"review_id":id,"proposal_id":request.proposal_id,"accepted_item_ids":request.selected_item_ids,"accepted_path":accepted_path,"metadata_path":metadata_path,"created":false,"replayed":true}),
            changes: vec![],
        });
    }
    let approved_at = stamp();
    let mut review = payload;
    review["schema_version"] = json!(1);
    review["review_id"] = json!(id);
    review["identity_sha256"] = json!(identity);
    review["approved_at"] = json!(approved_at);
    review["source_kind"] = meta["source_kind"].clone();
    review["original_path"] = meta["original"]["path"].clone();
    review["source_sha256"] = json!(current);
    review["accepted_path"] = json!(accepted_path);
    review["note"] = json!(
        "Explicitly reviewed derived material. No canonical career fact, CV, application status or external action was changed."
    );
    let mut markdown = format!(
        "# Reviewed intake material\n\n- Source kind: {}\n- Original: `{}`\n- Original SHA-256: `{current}`\n- Approved by: {}\n- Approval recorded at: {approved_at}\n- Approval evidence: {}\n\nThis derived note preserves the source type. Citations establish location and freshness; they do not certify semantic truth. Next steps are suggestions, never executed actions.\n",
        meta["source_kind"].as_str().unwrap_or("unknown"),
        meta["original"]["path"].as_str().unwrap_or(""),
        request.approved_by,
        request.approval_evidence
    );
    for item in &items {
        markdown.push_str(&format!(
            "\n## {} — {}\n\n{}\n\n",
            item["kind"].as_str().unwrap_or(""),
            item["id"].as_str().unwrap_or(""),
            item["text"].as_str().unwrap_or("")
        ));
        for c in item["citations"].as_array().expect("validated") {
            markdown.push_str(&format!(
                "- Source: `{}` lines {}–{} · SHA-256 `{}`\n",
                c["path"].as_str().unwrap_or(""),
                c["start_line"],
                c["end_line"],
                c["sha256"].as_str().unwrap_or("")
            ));
        }
    }
    let changes = changes(
        root,
        vec![
            (accepted_path.clone(), markdown),
            (metadata_path.clone(), as_text(&review)),
        ],
        true,
    )?;
    Ok(Plan {
        data: json!({"note_id":request.note_id,"review_id":id,"proposal_id":request.proposal_id,"accepted_item_ids":request.selected_item_ids,"accepted_path":accepted_path,"metadata_path":metadata_path,"created":true,"replayed":false,"facts_changed":false,"cv_changed":false,"next_steps_executed":false}),
        changes,
    })
}

pub fn archive(root: &Path, args: &Value) -> Result<Plan> {
    let request: Archive = parse(args)?;
    text(&request.idempotency_key, "Idempotency key", 20000)?;
    text(&request.reason, "Archive reason", 3000)?;
    text(&request.evidence, "Archive evidence", 20000)?;
    sha(&request.expected_metadata_sha256)?;
    let (mut meta, current) = metadata(root, &request.note_id)?;
    if current != request.expected_metadata_sha256 {
        return Err(fail(
            "stale_proposal",
            "Intake metadata changed since review",
        ));
    }
    if meta["archived"] == true {
        return Ok(Plan {
            data: json!({"note_id":request.note_id,"archived":true,"retained":true,"replayed":true}),
            changes: vec![],
        });
    }
    meta["archived"] = json!(true);
    meta["archive"] =
        json!({"reason":request.reason,"evidence":request.evidence,"recorded_at":stamp()});
    let path = format!("{}/metadata.json", note_home(&request.note_id)?);
    let changes = changes(root, vec![(path, as_text(&meta))], false)?;
    Ok(Plan {
        data: json!({"note_id":request.note_id,"archived":true,"retained":true,"replayed":false,"note":"Only local review-queue metadata changed; originals, proposals, reviews and submission evidence remain."}),
        changes,
    })
}

pub fn list(root: &Path, args: &Value) -> Result<Plan> {
    let request: Listing = parse(args)?;
    if request.limit == 0 || request.limit > 100 {
        return Err(fail("invalid_arguments", "Note page limit must be 1–100"));
    }
    let directory = crate::workspace::safe_path(root, HOME)?;
    let mut rows = Vec::new();
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let id = entry
                .file_name()
                .into_string()
                .map_err(|_| fail("invalid_resource", "Invalid intake filename"))?;
            identifier(&id, "note-")?;
            let (meta, sha) = metadata(root, &id)?;
            if !request.include_archived && meta["archived"] == true {
                continue;
            }
            rows.push(json!({"note_id":id,"source_kind":meta["source_kind"],"source_label":meta["source_label"],"occurred_at":meta["occurred_at"],"occurred_period":meta["occurred_period"],"valid_from":meta["valid_from"],"valid_to":meta["valid_to"],"supersedes":meta["supersedes"],"source_group":meta["source_group"],"recorded_at":meta["recorded_at"],"archived":meta["archived"],"original_path":meta["original"]["path"],"source_sha256":meta["original"]["sha256"],"metadata_path":format!("{}/{id}/metadata.json",HOME),"metadata_sha256":sha,"proposal_count":proposals(root,&id)?.len(),"accepted_review_count":accepted(root,&id)?.len()}));
        }
    }
    rows.sort_by(|a, b| b["recorded_at"].as_str().cmp(&a["recorded_at"].as_str()));
    let total = rows.len();
    let page = rows
        .into_iter()
        .skip(request.offset)
        .take(request.limit)
        .collect::<Vec<_>>();
    let next = request.offset.saturating_add(page.len());
    let more = next < total;
    Ok(Plan {
        data: json!({"notes":page,"page":{"offset":request.offset,"limit":request.limit,"total":total,"has_more":more,"next_offset":if more{Some(next)}else{None}}}),
        changes: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        crate::workspace::populate(root.path()).unwrap();
        fs::write(
            root.path().join("content/baseline.json"),
            b"{\"fixture\":\"unchanged\"}\n",
        )
        .unwrap();
        root
    }
    fn apply(root: &Path, plan: &Plan) {
        for change in &plan.changes {
            let path = crate::workspace::safe_path(root, change["path"].as_str().unwrap()).unwrap();
            crate::storage::atomic_write(&path, change["text"].as_str().unwrap().as_bytes())
                .unwrap();
        }
    }
    fn recorded(root: &Path) -> Value {
        let plan=record(root,&json!({"text":"I explained the import design.\r\nAn earlier date is unconfirmed.\r\n","source_kind":"transcript","idempotency_key":"fixture-note"})).unwrap();
        apply(root, &plan);
        plan.data
    }
    fn proposed(root: &Path, note: &Value) -> Value {
        let item = |id: &str, kind: &str, text: &str| json!({"id":id,"kind":kind,"text":text,"citations":[{"path":note["original_path"],"sha256":note["source_sha256"],"start_line":1,"end_line":1}]});
        let plan=propose(root,&json!({"note_id":note["note_id"],"expected_source_sha256":note["source_sha256"],"items":[item("summary","summary","Discussed import design"),item("addition","addition","UNSELECTED hypothesis"),item("next","next_step","UNSELECTED suggested outreach")],"questions":["What was the actual interview date?"],"idempotency_key":"fixture-proposal"})).unwrap();
        apply(root, &plan);
        plan.data
    }
    fn accepting(note: &Value, proposal: &Value) -> Value {
        json!({"note_id":note["note_id"],"proposal_id":proposal["proposal_id"],"expected_proposal_sha256":proposal["proposal_sha256"],"selected_item_ids":["summary"],"approved_by":"Synthetic user","approval_evidence":"Explicit fixture selection","idempotency_key":"fixture-review"})
    }
    #[test]
    fn original_is_exact_and_unknowns_stay_unknown() {
        let root = fixture();
        let request = json!({"text":"Original\r\nwith exact trailing newline.\r\n","source_kind":"personal_notes","idempotency_key":"raw"});
        let planned = record(root.path(), &request).unwrap();
        assert!(
            !root
                .path()
                .join(planned.data["original_path"].as_str().unwrap())
                .exists()
        );
        apply(root.path(), &planned);
        assert_eq!(
            read(root.path(), planned.data["original_path"].as_str().unwrap()).unwrap(),
            request["text"].as_str().unwrap().as_bytes()
        );
        let (meta, _) = metadata(root.path(), planned.data["note_id"].as_str().unwrap()).unwrap();
        assert!(meta["occurred_at"].is_null());
        assert!(meta["speakers"].is_null());
        let replay = record(root.path(), &request).unwrap();
        assert!(replay.changes.is_empty());
        assert_eq!(replay.data["replayed"], true);
    }
    #[test]
    fn changed_original_cannot_be_reused() {
        let root = fixture();
        let note = recorded(root.path());
        let path =
            crate::workspace::safe_path(root.path(), note["original_path"].as_str().unwrap())
                .unwrap();
        fs::write(path, "Changed source").unwrap();
        let viewed = review(
            root.path(),
            &json!({"note_id":note["note_id"],"include_original":true}),
        )
        .unwrap();
        assert_eq!(viewed.data["source_fresh"], false);
        assert!(propose(root.path(),&json!({"note_id":note["note_id"],"expected_source_sha256":note["source_sha256"],"items":[],"idempotency_key":"changed"})).is_err());
    }
    #[test]
    fn acceptance_keeps_only_selected_items_without_fact_or_cv_changes() {
        let root = fixture();
        let baseline = fs::read(root.path().join("content/baseline.json")).unwrap();
        let note = recorded(root.path());
        let proposal = proposed(root.path(), &note);
        let plan = accept(root.path(), &accepting(&note, &proposal)).unwrap();
        assert!(
            plan.changes
                .iter()
                .all(|c| c["path"].as_str().unwrap().starts_with("context/intake/"))
        );
        apply(root.path(), &plan);
        let accepted = String::from_utf8(
            read(root.path(), plan.data["accepted_path"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        assert!(accepted.contains("Discussed import design"));
        assert!(!accepted.contains("UNSELECTED"));
        assert!(accepted.contains("Source kind: transcript"));
        assert_eq!(
            fs::read(root.path().join("content/baseline.json")).unwrap(),
            baseline
        );
        assert_eq!(
            accept(root.path(), &accepting(&note, &proposal))
                .unwrap()
                .changes
                .len(),
            0
        );
        let original =
            String::from_utf8(read(root.path(), note["original_path"].as_str().unwrap()).unwrap())
                .unwrap();
        assert!(original.contains("An earlier date is unconfirmed"));
    }
    #[test]
    fn stale_proposal_and_bad_citations_are_rejected() {
        let root = fixture();
        let note = recorded(root.path());
        let proposal = proposed(root.path(), &note);
        let mut request = accepting(&note, &proposal);
        request["expected_proposal_sha256"] = json!("0".repeat(64));
        assert_eq!(
            accept(root.path(), &request).unwrap_err().code,
            "stale_proposal"
        );
        let bad = |start, end, hash| json!({"note_id":note["note_id"],"expected_source_sha256":note["source_sha256"],"items":[{"id":"x","kind":"summary","text":"Draft","citations":[{"path":note["original_path"],"sha256":hash,"start_line":start,"end_line":end}]}],"idempotency_key":"bad"});
        assert!(propose(root.path(), &bad(0, 1, note["source_sha256"].clone())).is_err());
        assert!(propose(root.path(), &bad(1, 99, note["source_sha256"].clone())).is_err());
        assert!(propose(root.path(), &bad(1, 1, json!("0".repeat(64)))).is_err());
    }
    #[test]
    fn a_changed_enrichment_source_blocks_acceptance() {
        let root = fixture();
        let note = recorded(root.path());
        let source = "context/profile/company.md";
        let path = crate::workspace::safe_path(root.path(), source).unwrap();
        fs::write(&path, "Supplied company context\n").unwrap();
        let digest = hash(&fs::read(&path).unwrap());
        let plan=propose(root.path(),&json!({"note_id":note["note_id"],"expected_source_sha256":note["source_sha256"],"items":[{"id":"x","kind":"addition","text":"A draft interpretation","citations":[{"path":source,"sha256":digest,"start_line":1,"end_line":1}]}],"idempotency_key":"draft"})).unwrap();
        apply(root.path(), &plan);
        fs::write(path, "Changed context\n").unwrap();
        let mut request = accepting(&note, &plan.data);
        request["selected_item_ids"] = json!(["x"]);
        assert_eq!(
            accept(root.path(), &request).unwrap_err().code,
            "source_changed"
        );
    }
    #[test]
    fn archive_only_changes_provenance_and_queue_metadata() {
        let root = fixture();
        let note = recorded(root.path());
        let proposal = proposed(root.path(), &note);
        let before = read(root.path(), note["original_path"].as_str().unwrap()).unwrap();
        let plan=archive(root.path(),&json!({"note_id":note["note_id"],"expected_metadata_sha256":note["metadata_sha256"],"reason":"Finished review","evidence":"Fixture request","idempotency_key":"archive"})).unwrap();
        assert_eq!(plan.changes.len(), 1);
        assert!(
            plan.changes[0]["path"]
                .as_str()
                .unwrap()
                .ends_with("/metadata.json")
        );
        apply(root.path(), &plan);
        assert_eq!(
            read(root.path(), note["original_path"].as_str().unwrap()).unwrap(),
            before
        );
        assert!(
            crate::workspace::safe_path(root.path(), proposal["proposal_path"].as_str().unwrap())
                .unwrap()
                .is_file()
        );
        assert_eq!(
            list(root.path(), &json!({})).unwrap().data["page"]["total"],
            0
        );
        assert_eq!(
            list(root.path(), &json!({"include_archived":true}))
                .unwrap()
                .data["page"]["total"],
            1
        );
    }
    #[test]
    fn links_and_source_provenance_are_validated() {
        let root = fixture();
        assert!(record(root.path(),&json!({"text":"Supplied text","source_kind":"provider_summary","occurred_at":"invented","idempotency_key":"date"})).is_err());
        assert!(record(root.path(),&json!({"text":"Supplied text","source_kind":"provider_summary","links":[{"kind":"project","path":"context/projects/missing.md"}],"idempotency_key":"link"})).is_err());
        assert!(record(root.path(),&json!({"text":"Supplied text","source_kind":"provider_summary","source_uri":"https://user:secret@example.invalid","idempotency_key":"credential"})).is_err());
    }
    #[test]
    fn backup_restore_preserves_exact_original_and_review_provenance() {
        let root = fixture();
        let note = recorded(root.path());
        let proposal = proposed(root.path(), &note);
        let accepted = accept(root.path(), &accepting(&note, &proposal)).unwrap();
        apply(root.path(), &accepted);
        let out = tempfile::tempdir().unwrap();
        let bundle = out.path().join("notes.jhb");
        crate::bundle::export_bundle(root.path(), &bundle, "workspace").unwrap();
        let restored = out.path().join("restored");
        crate::bundle::import_bundle(&restored, &bundle).unwrap();
        for key in ["original_path", "metadata_path"] {
            assert_eq!(
                read(root.path(), note[key].as_str().unwrap()).unwrap(),
                read(&restored, note[key].as_str().unwrap()).unwrap()
            );
        }
        assert_eq!(
            read(
                root.path(),
                accepted.data["accepted_path"].as_str().unwrap()
            )
            .unwrap(),
            read(&restored, accepted.data["accepted_path"].as_str().unwrap()).unwrap()
        );
    }
    #[test]
    fn rough_project_capture_preserves_collaborators_and_superseded_sources() {
        let root = fixture();
        fs::create_dir_all(root.path().join("context/projects")).unwrap();
        fs::write(
            root.path().join("context/projects/example.md"),
            "# Fictional example project\n",
        )
        .unwrap();
        let raw = "handled rollout; Sam designed schema";
        let first=record(root.path(),&json!({"text":raw,"source_kind":"personal_notes","source_group":"Example delivery","occurred_period":"around Q2 2024","valid_from":"2024","valid_to":"ongoing","links":[{"kind":"project","path":"context/projects/example.md"}],"idempotency_key":"rough"})).unwrap();
        apply(root.path(), &first);
        let (meta, _) = metadata(root.path(), first.data["note_id"].as_str().unwrap()).unwrap();
        assert_eq!(meta["occurred_period"], "around Q2 2024");
        assert!(meta["occurred_at"].is_null());
        assert!(meta["speakers"].is_null());
        assert_eq!(meta["valid_to"], "ongoing");
        let cite = json!({"path":first.data["original_path"],"sha256":first.data["source_sha256"],"start_line":1,"end_line":1});
        let proposal=propose(root.path(),&json!({"note_id":first.data["note_id"],"expected_source_sha256":first.data["source_sha256"],"items":[{"id":"mine","kind":"summary","text":"Handled the rollout.","citations":[cite.clone()]},{"id":"unapproved","kind":"addition","text":"Owned the schema design.","citations":[cite]}],"idempotency_key":"rough-proposal"})).unwrap();
        apply(root.path(), &proposal);
        let mut request = accepting(&first.data, &proposal.data);
        request["selected_item_ids"] = json!(["mine"]);
        let approved = accept(root.path(), &request).unwrap();
        apply(root.path(), &approved);
        let accepted = String::from_utf8(
            read(
                root.path(),
                approved.data["accepted_path"].as_str().unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(accepted.contains("Handled the rollout."));
        assert!(!accepted.contains("Owned the schema design."));
        let next=record(root.path(),&json!({"text":"Further clarification supplied by the user.","source_kind":"personal_notes","supersedes":first.data["note_id"],"source_group":"Example delivery","idempotency_key":"followup"})).unwrap();
        apply(root.path(), &next);
        assert_eq!(
            read(root.path(), first.data["original_path"].as_str().unwrap()).unwrap(),
            raw.as_bytes()
        );
        let (latest, _) = metadata(root.path(), next.data["note_id"].as_str().unwrap()).unwrap();
        assert_eq!(latest["supersedes"], first.data["note_id"]);
    }
}
