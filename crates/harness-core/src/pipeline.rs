//! Disposable views built from canonical vacancy Markdown; never a source of facts.
use crate::{HarnessError, Result};
use regex::Regex;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

pub const STAGES: &[&str] = &[
    "saved",
    "prepared",
    "applied",
    "introduction",
    "screening",
    "technical",
    "final",
    "offer",
    "closed",
    "needs-review",
];
pub const TRACKING_PATTERN: &str = r"(?s)<!-- tracking\n(.*?)\n-->";
pub const SCHEDULE_STATES: &[&str] = &[
    "unbooked",
    "proposed",
    "availability-sent",
    "booked",
    "completed",
    "awaiting-outcome",
    "handled-unconfirmed",
];
const RESERVED: &[&str] = &[
    "README.md",
    "shortlist.md",
    "template.md",
    "pipeline.md",
    "approved-answers.md",
    "platform-profiles.md",
];
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub records: Vec<Value>,
    pub markdown: String,
}
fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}
fn bad(name: &str, message: &str) -> HarnessError {
    HarnessError::validation(format!("{name}: {message}"))
}

pub fn is_vacancy_filename(name: &str) -> bool {
    !RESERVED.contains(&name)
        && !rx(r"^\d{4}-").is_match(name)
        && !["email-", "search-", "rejection-review-"]
            .iter()
            .any(|p| name.starts_with(p))
        && !rx(r"-answers(?:-|\.)").is_match(name)
}

pub fn classify(text: &str) -> (&'static str, String) {
    let plain = text.replace("**", "");
    let current = rx(r"(?mi)^\s*(?:- )?Current status[^:]*:\s*(.+)$")
        .captures_iter(&plain)
        .last()
        .map(|c| c[1].to_owned());
    let stage = rx(r"(?mi)^\s*(?:- )?(?:Stage|Status):\s*(.+)$")
        .captures(&plain)
        .map(|c| c[1].to_owned())
        .or_else(|| {
            rx(r"(?m)^- ((?:Submitted|Applied) \d.*)$")
                .captures(&plain)
                .map(|c| c[1].to_owned())
        });
    let value = current.or(stage).unwrap_or_default();
    if rx(r"(?m)^- Outcome: rejection\.").is_match(text)
        || rx(r"(?i)conversation shows .*declined the request").is_match(text)
    {
        return (
            "closed",
            if value.is_empty() {
                "Explicit rejection outcome".into()
            } else {
                value
            },
        );
    }
    let lower = value.to_lowercase();
    let contains = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let state = if lower.contains("unassigned rejection") {
        "needs-review"
    } else if contains(&["rejected", "closed", "unavailable", "withdrawn"]) {
        "closed"
    } else if contains(&["unverified", "unknown", "undefined", "incomplete"]) {
        "needs-review"
    } else if lower.contains("technical interview") {
        "technical"
    } else if contains(&["invited", "introductory", "introduced", "interest"]) {
        "introduction"
    } else if contains(&["submitted", "applied", "application/message sent"]) {
        "applied"
    } else if contains(&["prepared", "preparing", "drafted", "blocked"]) {
        "prepared"
    } else if contains(&["researching", "saved"]) {
        "saved"
    } else {
        "needs-review"
    };
    (
        state,
        if value.is_empty() {
            "No structured stage yet".into()
        } else {
            value
        },
    )
}

fn valid_iso(value: &str) -> bool {
    crate::domain::iso_datetime(value).is_ok()
}

pub fn validate_tracking(root: &Path, name: &str, data: &Value) -> Result<()> {
    let map = data
        .as_object()
        .ok_or_else(|| bad(name, "tracking must be an object"))?;
    for (key, value) in map {
        if !value.is_string() && !(key == "posting_id" && value.is_null()) {
            return Err(bad(
                name,
                &format!(
                    "{key} must be a string{}",
                    if key == "posting_id" { " or null" } else { "" }
                ),
            ));
        }
    }
    if !STAGES.contains(&data["stage"].as_str().unwrap_or(""))
        || data["next_action"].as_str().is_none_or(str::is_empty)
        || data["checked"].as_str().is_none_or(str::is_empty)
    {
        return Err(bad(name, "missing or invalid tracking fields"));
    }
    if !valid_iso(data["checked"].as_str().unwrap())
        || data["scheduled_start"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && !valid_iso(s))
    {
        return Err(bad(name, "invalid tracking date"));
    }
    for key in ["brief", "record_evidence"] {
        if let Some(link) = data[key].as_str().filter(|s| !s.is_empty()) {
            let relative = format!("applications/{link}");
            let path = crate::workspace::safe_path(root, &relative)?;
            if !path.is_file() {
                return Err(bad(name, &format!("missing or out-of-scope {key}")));
            }
        }
    }
    if data["schedule_state"]
        .as_str()
        .is_some_and(|s| !s.is_empty() && !SCHEDULE_STATES.contains(&s))
    {
        return Err(bad(name, "invalid schedule state"));
    }
    if let Some(time) = data["scheduled_start"].as_str().filter(|s| !s.is_empty())
        && (!crate::domain::iso_datetime(time).is_ok_and(|(_, offset)| offset.is_some())
            || data["schedule_state"] != "booked")
    {
        return Err(bad(name, "scheduled time needs timezone and booked state"));
    }
    Ok(())
}

pub fn build(root: &Path) -> Result<Pipeline> {
    let directory = root.join("applications");
    if !directory.exists() {
        return Ok(Pipeline {
            records: vec![],
            markdown: render(&[]),
        });
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut records = Vec::new();
    let block = rx(TRACKING_PATTERN);
    let title_rx = rx(r"(?m)^# (.+)$");
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let kind = fs::symlink_metadata(&path)?.file_type();
        if !kind.is_file() && !kind.is_symlink() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| HarnessError::validation("Non-UTF-8 vacancy filename"))?;
        let path = crate::workspace::safe_path(root, &format!("applications/{name}"))?;
        let text = fs::read_to_string(path)?;
        let title = title_rx.captures(&text).map(|c| c[1].to_string());
        let matches = block.captures_iter(&text).collect::<Vec<_>>();
        if matches.is_empty() {
            // A prose example marker is allowed; every actual tracking marker must use the canonical format.
            if rx(r"(?i)<!--\s*tracking\b([^\r\n]*)")
                .captures_iter(&text)
                .any(|m| !rx(r"(?i)^[ \t-]+example\b").is_match(&m[1]))
            {
                return Err(bad(name, "malformed tracking block"));
            }
            if !is_vacancy_filename(name) || title.is_none() {
                continue;
            }
        } else {
            if RESERVED.contains(&name) {
                return Err(bad(
                    name,
                    "reserved non-vacancy file contains a tracking block",
                ));
            }
            if title.is_none() {
                return Err(bad(name, "tracking block without '# ' title"));
            }
            if matches.len() > 1 {
                return Err(bad(name, "duplicate tracking block"));
            }
        }
        let data = if matches.is_empty() {
            let (stage, summary) = classify(&text);
            json!({"stage":stage,"status_detail":summary,"next_action":"Read latest record before acting; legacy prose is not a live status check.","tracking_source":"legacy-prose"})
        } else {
            let mut data: Value = serde_json::from_str(&matches[0][1])
                .map_err(|_| bad(name, "invalid tracking JSON"))?;
            validate_tracking(root, name, &data)?;
            data["tracking_source"] = json!("structured");
            data
        };
        let mut row = serde_json::Map::new();
        row.insert("record".into(), json!(name));
        row.insert("title".into(), json!(title.unwrap()));
        for (key, value) in data.as_object().expect("validated tracking object") {
            if key != "record" && key != "title" {
                row.insert(key.clone(), value.clone());
            }
        }
        records.push(Value::Object(row));
    }
    let markdown = render(&records);
    Ok(Pipeline { records, markdown })
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn render(records: &[Value]) -> String {
    let mut counts = BTreeMap::new();
    for r in records {
        *counts.entry(text(r, "stage")).or_insert(0usize) += 1;
    }
    let mut lines=vec!["# Application pipeline".into(),"".into(),"Generated from canonical vacancy records. Update the record, then call Job Harness `rebuild_pipeline`; do not edit this view. No live board, email or calendar sync.".into(),"".into(),"Structured entries retain their attributed review dates. Legacy prose can be stale; use the linked record before taking action. Applied includes direct requests/profile sharing, not only ATS receipts. Proposed times are never booked times.".into(),"".into(),"| Stage | Records |".into(),"|---|---:|".into()];
    for stage in STAGES {
        lines.push(format!("| {stage} | {} |", counts.get(stage).unwrap_or(&0)));
    }
    lines.extend([
        "".into(),
        "## Active conversations and interviews".into(),
        "".into(),
        "| Role | Stage / scheduling | Next action | Booking route | Brief |".into(),
        "|---|---|---|---|---|".into(),
    ]);
    for r in records.iter().filter(|r| {
        text(r, "tracking_source") == "structured"
            && ["introduction", "screening", "technical", "final", "offer"]
                .contains(&text(r, "stage"))
    }) {
        let scheduling = ["scheduled_start", "proposed_time", "schedule_state"]
            .iter()
            .map(|key| text(r, key))
            .find(|s| !s.is_empty())
            .unwrap_or("unknown");
        let brief = if text(r, "brief").is_empty() {
            "Needed".into()
        } else {
            format!("[Prep]({})", text(r, "brief"))
        };
        let booking = if text(r, "booking_url").is_empty() {
            "Email / no employer link saved".into()
        } else {
            format!("[Employer calendar]({})", text(r, "booking_url"))
        };
        lines.push(format!(
            "| [{}]({}) | {}: {} | {} | {} | {} |",
            text(r, "title"),
            text(r, "record"),
            text(r, "stage"),
            scheduling,
            text(r, "next_action"),
            booking,
            brief
        ));
    }
    for stage in STAGES {
        let subset = records
            .iter()
            .filter(|r| text(r, "stage") == *stage)
            .collect::<Vec<_>>();
        if !subset.is_empty() {
            let mut chars = stage.chars();
            let heading = chars.next().unwrap().to_uppercase().to_string() + chars.as_str();
            lines.extend(["".into(), format!("## {heading}"), "".into()]);
            for r in subset {
                let detail = ["status_detail", "schedule_state"]
                    .iter()
                    .map(|k| text(r, k))
                    .find(|s| !s.is_empty())
                    .unwrap_or("See record");
                lines.push(format!(
                    "- [{}]({}) — {}",
                    text(r, "title"),
                    text(r, "record"),
                    detail
                ));
            }
        }
    }
    lines.extend(["".into(),"## Maintain and prepare".into(),"".into(),"Use the [tracking and briefing workflow](../docs/application-pipeline.md). Interview notes remain under `context/interviews/`; company briefs are coaching under `applications/briefs/`. Neither changes career facts or the CV.".into(),"".into()]);
    lines.join("\n")
}

/// Historical pipeline encoding differs from sorted UTF-8 packet/state encoding.
pub fn json_bytes(records: &[Value]) -> Result<Vec<u8>> {
    let mut document = serde_json::Map::new();
    document.insert("schemaVersion".into(), json!(1));
    document.insert(
        "source".into(),
        json!("canonical application Markdown records; generated, not live synced"),
    );
    document.insert("records".into(), json!(records));
    Ok(job_harness_domain::canonical::ascii_pretty(&Value::Object(
        document,
    )))
}

pub fn write(root: &Path) -> Result<Value> {
    let root = fs::canonicalize(root)?;
    let pipeline = build(&root)?;
    fs::create_dir_all(root.join("applications"))?;
    crate::storage::atomic_write(
        &root.join("applications/pipeline.json"),
        &json_bytes(&pipeline.records)?,
    )?;
    crate::storage::atomic_write(
        &root.join("applications/pipeline.md"),
        pipeline.markdown.as_bytes(),
    )?;
    Ok(
        json!({"records":pipeline.records.len(),"paths":["applications/pipeline.json","applications/pipeline.md"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(text: &str) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir(t.path().join("applications")).unwrap();
        fs::write(t.path().join("applications/sample.md"), text).unwrap();
        t
    }
    #[test]
    fn structured_is_authoritative() {
        let t = fixture(
            "# Example engineer\n\nCurrent status: Rejected\n<!-- tracking\n{\"stage\":\"screening\",\"next_action\":\"Reply\",\"checked\":\"2026-01-02\",\"schedule_state\":\"proposed\"}\n-->\n",
        );
        let p = build(t.path()).unwrap();
        assert_eq!(p.records[0]["stage"], "screening");
        assert!(p.markdown.contains("screening: proposed"));
    }
    #[test]
    fn unknown_prose_cannot_look_applied() {
        assert_eq!(
            classify("# Example\nStatus: Applied, outcome unknown").0,
            "needs-review"
        );
    }
    #[test]
    fn malformed_and_duplicate_blocks_fail() {
        let t = fixture("# Example\n<!-- tracking {broken} -->");
        assert!(build(t.path()).is_err());
        let t = fixture("# Example\n<!-- tracking\n{}\n-->\n<!-- tracking\n{}\n-->");
        assert!(build(t.path()).is_err());
    }
    #[test]
    fn projection_keeps_python_ascii_and_field_order() {
        let t = fixture(
            "# Synthetic £ 😀\n<!-- tracking\n{\"stage\":\"saved\",\"next_action\":\"Review\",\"checked\":\"2026-01-02\"}\n-->\n",
        );
        let p = build(t.path()).unwrap();
        let bytes = json_bytes(&p.records).unwrap();
        let serialized = String::from_utf8(bytes).unwrap();
        assert!(serialized.starts_with("{\n  \"schemaVersion\": 1,\n  \"source\":"));
        assert!(serialized.contains("\n      \"record\": \"sample.md\",\n      \"title\": \"Synthetic \\u00a3 \\ud83d\\ude00\""));
        assert!(serialized.ends_with("\n"));
    }
    #[test]
    fn complete_projection_matches_python_v1_golden_corpus() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/pipeline-v1.json")).unwrap();
        let root = tempfile::tempdir().unwrap();
        for (relative, text) in fixture["files"].as_object().unwrap() {
            let path = root.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text.as_str().unwrap()).unwrap();
        }
        let built = build(root.path()).unwrap();
        assert_eq!(
            built.records.len(),
            fixture["record_count"].as_u64().unwrap() as usize
        );
        assert_eq!(
            built.markdown,
            fixture["expected_markdown"].as_str().unwrap()
        );
        assert_eq!(
            json_bytes(&built.records).unwrap(),
            fixture["expected_json"].as_str().unwrap().as_bytes()
        );
    }
    #[test]
    fn booked_needs_timezone() {
        let t = fixture(
            "# Example\n<!-- tracking\n{\"stage\":\"screening\",\"next_action\":\"Attend\",\"checked\":\"2026-01-02\",\"schedule_state\":\"booked\",\"scheduled_start\":\"2026-01-03T11:00:00\"}\n-->",
        );
        assert!(build(t.path()).is_err());
    }
}
