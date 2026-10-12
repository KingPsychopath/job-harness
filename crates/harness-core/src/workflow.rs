//! Deterministic checks over host observations. No connector, model or competing memory store.
use crate::{HarnessError, Result, storage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub source: String,
    pub observed_at: String,
    pub requested_period: String,
    pub pages_read: usize,
    pub pagination_exhausted: bool,
    pub failed_reads: Vec<String>,
    pub exclusions: Vec<String>,
    pub checkpoint: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mail {
    pub provider: String,
    pub thread_id: String,
    pub message_ids: Vec<String>,
    pub request_message_ids: Option<Vec<String>>,
    pub latest_message_id: Option<String>,
    pub vacancy_url: Option<String>,
    pub posting_id: Option<String>,
    pub company: Option<String>,
    pub candidate_records: Vec<String>,
    pub requires_reply: Option<bool>,
    pub replied_to_message_ids: Vec<String>,
    pub observed_at: String,
}
#[derive(Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub question_key: String,
    pub value: Value,
    pub source_path: String,
    pub source_sha256: String,
    pub confirmed_at: String,
    pub valid_until: Option<String>,
    pub countries: Vec<String>,
    pub employers: Vec<String>,
    pub polarity: String,
    pub superseded_by: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerQuestion {
    pub question_key: String,
    pub country: Option<String>,
    pub employer: Option<String>,
    pub polarity: String,
    pub as_of: String,
    pub answer: Answer,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormField {
    pub id: String,
    pub prompt: String,
    pub expected: Value,
    pub observed: Value,
    pub source_kind: String,
    pub source_path: Option<String>,
    pub source_sha256: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Upload {
    pub path: String,
    pub sha256: String,
    pub observed_name: String,
    pub expected_name: Option<String>,
    pub observed_present: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Form {
    pub vacancy_url: String,
    pub page_revision: String,
    pub readback_revision: String,
    pub observed_at: String,
    pub fields: Vec<FormField>,
    pub uploads: Vec<Upload>,
    pub parsed_claim_changes: Vec<String>,
    pub completeness_confirmed: Option<bool>,
}
fn parsed<T: serde::de::DeserializeOwned>(data: &Value) -> Result<T> {
    serde_json::from_value(data.clone()).map_err(|_| {
        HarnessError::validation("Observation does not satisfy its published typed contract")
    })
}
fn required(values: &[&str]) -> Result<()> {
    if values
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 20000)
    {
        return Err(HarnessError::validation(
            "Required observation identifiers must be nonempty and bounded",
        ));
    }
    Ok(())
}
fn source(root: &Path, path: &str, sha: &str) -> Result<bool> {
    let p = storage::safe_path(
        root,
        path,
        &["context", "applications", "content", "output"],
        &[".md", ".json", ".pdf", ".txt"],
    )?;
    Ok(storage::digest(&fs::read(p)?) == sha)
}
pub fn coverage(data: &Value) -> Result<Value> {
    let c: Coverage = parsed(data)?;
    required(&[&c.source, &c.observed_at, &c.requested_period])?;
    if c.pages_read > 10000 || c.failed_reads.len() > 1000 || c.exclusions.len() > 1000 {
        return Err(HarnessError::validation("Bound coverage observations"));
    }
    let complete = c.pagination_exhausted && c.failed_reads.is_empty();
    let mut gaps = Vec::new();
    if !c.pagination_exhausted {
        gaps.push("pagination not exhausted");
    }
    if !c.failed_reads.is_empty() {
        gaps.push("failed reads remain");
    }
    Ok(
        json!({"complete":complete,"coverage":c,"missing":gaps,"authority":"observed_scope_only","next":"Never claim all mail/roles were checked beyond this recorded period and exclusions."}),
    )
}
pub fn mail(root: &Path, data: &Value) -> Result<Value> {
    let m: Mail = parsed(data)?;
    required(&[&m.provider, &m.thread_id, &m.observed_at])?;
    if m.message_ids.is_empty() || m.message_ids.len() > 1000 || m.candidate_records.len() > 100 {
        return Err(HarnessError::validation("Bound thread/message attribution"));
    }
    let ids = m.message_ids.iter().collect::<BTreeSet<_>>();
    if ids.len() != m.message_ids.len() || m.message_ids.iter().any(|id| id.trim().is_empty()) {
        return Err(HarnessError::validation(
            "Message identities must be unique and nonempty",
        ));
    }
    if m.replied_to_message_ids.len() > 1000
        || m.replied_to_message_ids.iter().any(|id| !ids.contains(id))
        || m.replied_to_message_ids
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != m.replied_to_message_ids.len()
        || m.latest_message_id
            .as_ref()
            .is_some_and(|id| !ids.contains(id))
    {
        return Err(HarnessError::validation(
            "Reply and latest-message identities must belong to the observed thread",
        ));
    }
    if let Some(requests) = &m.request_message_ids
        && (requests.len() > 1000
            || requests.iter().any(|id| !ids.contains(id))
            || requests.iter().collect::<BTreeSet<_>>().len() != requests.len())
    {
        return Err(HarnessError::validation(
            "Request identities must be unique observed message IDs",
        ));
    }
    let records = crate::pipeline::build(root)?.records;
    let supplied_url = m
        .vacancy_url
        .as_deref()
        .map(crate::domain::normal_url)
        .transpose()?;
    if let Some(id) = &m.posting_id {
        required(&[id])?;
    }
    let mut identity_checks = Vec::new();
    for p in &m.candidate_records {
        let file = storage::safe_path(root, p, &["applications"], &[".md"])?;
        if !file.is_file() {
            return Err(HarnessError::new(
                "missing_resource",
                "Attribution references a missing application",
            ));
        }
        let record = records.iter().find(|row| {
            row["record"]
                .as_str()
                .is_some_and(|name| p == &format!("applications/{name}"))
        });
        // Only canonical identity fields are evidence. A company name, filename or
        // an arbitrary URL elsewhere in the note cannot establish a posting match.
        let canonical_url = record.and_then(|row| row["vacancy_url"].as_str());
        let canonical_posting = record.and_then(|row| row["posting_id"].as_str());
        let url_match = supplied_url.as_deref().map(|url| {
            canonical_url
                .and_then(|stored| crate::domain::normal_url(stored).ok())
                .map(|stored| stored == url)
        });
        let posting_match = m
            .posting_id
            .as_deref()
            .map(|id| canonical_posting.map(|stored| stored == id));
        let conflict = url_match == Some(Some(false)) || posting_match == Some(Some(false));
        let missing = url_match == Some(None) || posting_match == Some(None);
        let matched = url_match == Some(Some(true)) || posting_match == Some(Some(true));
        identity_checks.push(json!({"record_path":p,"vacancy_url_matches":url_match.flatten(),"posting_id_matches":posting_match.flatten(),"conflict":conflict,"incomplete":missing,"exact":matched&&!conflict&&!missing}));
    }
    let match_state = if identity_checks.iter().any(|row| row["conflict"] == true) {
        "identity_conflict"
    } else if m.candidate_records.is_empty() {
        "unassigned"
    } else if identity_checks.len() == 1 && identity_checks[0]["exact"] == true {
        "proposed_exact_identity"
    } else {
        "ambiguous"
    };
    let requests_known = m.request_message_ids.is_some() && m.latest_message_id.is_some();
    let requests = m.request_message_ids.as_deref().unwrap_or_default();
    let unanswered_requests: Vec<_> = requests
        .iter()
        .filter(|id| !m.replied_to_message_ids.contains(id))
        .cloned()
        .collect();
    let contradictory = (m.requires_reply == Some(true) && requests.is_empty())
        || (m.requires_reply == Some(false) && !unanswered_requests.is_empty());
    let needs_review = match_state != "proposed_exact_identity"
        || m.requires_reply.is_none()
        || !requests_known
        || contradictory;
    // Receipt IDs never stand in for requests. Missing request/latest evidence
    // remains actionable rather than silently treating a mixed thread as resolved.
    let needs_attention = !unanswered_requests.is_empty()
        || (m.requires_reply != Some(false)
            && (!requests_known || contradictory || m.requires_reply.is_none()));
    Ok(
        json!({"thread":m,"attribution":match_state,"identity_checks":identity_checks,"request_evidence_complete":requests_known&&!contradictory,"unanswered_request_message_ids":unanswered_requests,"needs_review":needs_review,"needs_attention":needs_attention,"archive_permitted":false,"next":"Review canonical posting identity and the latest request before archive/status changes. Company-only evidence never establishes an exact vacancy."}),
    )
}

pub fn answer(root: &Path, data: &Value) -> Result<Value> {
    let q: AnswerQuestion = parsed(data)?;
    required(&[
        &q.question_key,
        &q.as_of,
        &q.answer.question_key,
        &q.answer.confirmed_at,
    ])?;
    if !["affirmative", "negated"].contains(&q.polarity.as_str())
        || !["affirmative", "negated"].contains(&q.answer.polarity.as_str())
    {
        return Err(HarnessError::validation(
            "Question polarity must be explicit",
        ));
    }
    let mut blockers = Vec::new();
    if q.question_key != q.answer.question_key {
        blockers.push("question property differs");
    }
    if q.answer.superseded_by.is_some() {
        blockers.push("answer superseded");
    }
    let as_of = chrono::NaiveDate::parse_from_str(&q.as_of, "%Y-%m-%d")
        .map_err(|_| HarnessError::validation("Answer evaluation date must be YYYY-MM-DD"))?;
    match crate::domain::iso_datetime(&q.answer.confirmed_at) {
        Ok((confirmed, _)) if confirmed.date() > as_of => {
            blockers.push("answer confirmed after evaluation date")
        }
        Err(_) => blockers.push("answer confirmation date uncertain"),
        _ => {}
    }
    if let Some(until) = &q.answer.valid_until {
        match chrono::NaiveDate::parse_from_str(until, "%Y-%m-%d") {
            Ok(date) if date < as_of => blockers.push("answer validity elapsed"),
            Err(_) => blockers.push("answer validity date uncertain"),
            _ => {}
        }
    }
    if !q.answer.countries.is_empty()
        && q.country
            .as_ref()
            .is_none_or(|country| !q.answer.countries.contains(country))
    {
        blockers.push("country scope does not match");
    }
    if !q.answer.employers.is_empty()
        && q.employer.as_ref().is_none_or(|employer| {
            !q.answer.employers.iter().any(|scope| {
                crate::domain::company_key(scope) == crate::domain::company_key(employer)
            })
        })
    {
        blockers.push("employer exception does not match");
    }
    if !source(root, &q.answer.source_path, &q.answer.source_sha256)? {
        blockers.push("answer source changed");
    }
    let value = if q.polarity != q.answer.polarity {
        match q.answer.value.as_bool() {
            Some(v) => json!(!v),
            None => {
                blockers.push("only explicit boolean answers can invert polarity");
                Value::Null
            }
        }
    } else {
        q.answer.value.clone()
    };
    Ok(
        json!({"reusable":blockers.is_empty(),"value":if blockers.is_empty(){value}else{Value::Null},"blockers":blockers,"source":{"path":q.answer.source_path,"sha256":q.answer.source_sha256,"confirmed_at":q.answer.confirmed_at},"authority":"supplied_scoped_answer","next":"Reuse an in-scope confirmed answer without asking again; missing scope remains unresolved."}),
    )
}
pub fn form(root: &Path, data: &Value) -> Result<Value> {
    let form: Form = parsed(data)?;
    required(&[
        &form.vacancy_url,
        &form.page_revision,
        &form.readback_revision,
        &form.observed_at,
    ])?;
    if form.fields.len() > 1000 || form.uploads.len() > 30 || form.parsed_claim_changes.len() > 1000
    {
        return Err(HarnessError::validation("Bound form observations"));
    }
    let mut issues = Vec::new();
    if form.completeness_confirmed == Some(false)
        || (form.fields.is_empty()
            && form.uploads.is_empty()
            && form.completeness_confirmed != Some(true))
    {
        issues.push(json!({"kind":"incomplete_observation","message":"No affirmative host completeness evidence supports this form observation"}));
    }
    if form.page_revision != form.readback_revision {
        issues.push(json!({"kind":"rerender","message":"Form references are stale; inspect current fields again"}));
    }
    let mut ids = BTreeSet::new();
    for field in &form.fields {
        required(&[&field.id, &field.prompt])?;
        if !ids.insert(&field.id) {
            return Err(HarnessError::validation("Field identifiers must be unique"));
        }
        if !["profile", "saved_answer", "drafted_narrative", "unresolved"]
            .contains(&field.source_kind.as_str())
        {
            return Err(HarnessError::validation(
                "Field source must identify profile, saved answer, drafted narrative or unresolved",
            ));
        }
        if field.source_kind == "unresolved" {
            issues.push(json!({"field":field.id,"kind":"unresolved"}));
        }
        if field.expected != field.observed {
            issues.push(json!({"field":field.id,"kind":"readback_mismatch"}));
        }
        if let (Some(path), Some(sha)) = (&field.source_path, &field.source_sha256) {
            if !source(root, path, sha)? {
                issues.push(json!({"field":field.id,"kind":"source_changed"}));
            }
        } else if field.source_kind != "unresolved" {
            issues.push(json!({"field":field.id,"kind":"missing_source"}));
        }
    }
    for upload in &form.uploads {
        if !upload.observed_present || !source(root, &upload.path, &upload.sha256)? {
            issues.push(json!({"upload":upload.path,"kind":"upload_unverified"}));
        }
        let expected_name = upload
            .expected_name
            .as_deref()
            .or_else(|| {
                Path::new(&upload.path)
                    .file_name()
                    .and_then(|name| name.to_str())
            })
            .unwrap_or("");
        if upload.observed_present
            && (expected_name.is_empty() || expected_name != upload.observed_name)
        {
            issues.push(json!({"upload":upload.path,"kind":"upload_filename_mismatch","expected_name":expected_name,"observed_name":upload.observed_name}));
        }
    }
    for change in &form.parsed_claim_changes {
        issues.push(json!({"kind":"parsed_cv_claim_change","detail":change}));
    }
    Ok(
        json!({"ready_for_preparation":issues.is_empty(),"issues":issues,"observation":{"vacancy_url":form.vacancy_url,"page_revision":form.page_revision,"observed_at":form.observed_at,"completeness_confirmed":form.completeness_confirmed},"submit_permitted":false,"next":"A matching readback supports packet preparation. It does not replace the separate begin/one-submit/outcome boundary."}),
    )
}
/// Batch related checks in one call; no inferred provider action or persistent side effect.
pub fn check(root: &Path, args: &Value) -> Result<Value> {
    let checks = args["checks"]
        .as_array()
        .ok_or_else(|| HarnessError::validation("Provide checks"))?;
    if checks.is_empty() || checks.len() > 20 {
        return Err(HarnessError::validation(
            "Batch 1–20 related workflow checks",
        ));
    }
    let mut result = Vec::new();
    for row in checks {
        let id = row["id"]
            .as_str()
            .ok_or_else(|| HarnessError::validation("Each check needs an id"))?;
        let kind = row["kind"]
            .as_str()
            .ok_or_else(|| HarnessError::validation("Each check needs a kind"))?;
        let data = match kind {
            "coverage" => coverage(&row["observation"]),
            "mail" => mail(root, &row["observation"]),
            "answer" => answer(root, &row["observation"]),
            "form" => form(root, &row["observation"]),
            _ => Err(HarnessError::validation("Unknown workflow check")),
        }?;
        result.push(json!({"id":id,"kind":kind,"data":data}));
    }
    Ok(json!({"checks":result,"canonical_changes":false,"external_actions":false}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_page_reads_cannot_claim_exhaustive_coverage() {
        let input = json!({"source":"fixture-mail","observed_at":"2026-01-01","requested_period":"January","pages_read":1,"pagination_exhausted":false,"failed_reads":["message-2"],"exclusions":[],"checkpoint":"next-page"});
        assert_eq!(coverage(&input).unwrap()["complete"], false);
    }
    #[test]
    fn ambiguous_company_and_mixed_thread_never_get_archive_permission() {
        let t = tempfile::tempdir().unwrap();
        let input = json!({"provider":"fixture","thread_id":"t1","message_ids":["receipt","invite"],"vacancy_url":null,"posting_id":null,"company":"Example","candidate_records":[],"requires_reply":true,"replied_to_message_ids":["receipt"],"observed_at":"2026-01-01"});
        let r = mail(t.path(), &input).unwrap();
        assert_eq!(r["needs_attention"], true);
        assert_eq!(r["attribution"], "unassigned");
        assert_eq!(r["archive_permitted"], false);
    }
    #[test]
    fn saved_boolean_answers_require_country_and_invert_explicit_polarity() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("context")).unwrap();
        fs::write(t.path().join("context/answer.md"), "User-confirmed fixture").unwrap();
        let sha = storage::digest(b"User-confirmed fixture");
        let mut q = json!({"question_key":"sponsorship_needed","country":"GB","employer":"Example","polarity":"negated","as_of":"2026-01-01","answer":{"question_key":"sponsorship_needed","value":false,"source_path":"context/answer.md","source_sha256":sha,"confirmed_at":"2026-01-01","valid_until":null,"countries":["GB"],"employers":[],"polarity":"affirmative","superseded_by":null}});
        assert_eq!(answer(t.path(), &q).unwrap()["value"], true);
        q["country"] = json!("CA");
        assert_eq!(answer(t.path(), &q).unwrap()["reusable"], false);
    }
    #[test]
    fn rerender_and_cv_parser_changes_block_preparation() {
        let t = tempfile::tempdir().unwrap();
        let f = json!({"vacancy_url":"https://example.test/job","page_revision":"1","readback_revision":"2","observed_at":"2026-01-01","fields":[{"id":"date","prompt":"Employment date","expected":"2022","observed":"2023","source_kind":"unresolved","source_path":null,"source_sha256":null}],"uploads":[],"parsed_claim_changes":["Employment row duplicated"]});
        let r = form(t.path(), &f).unwrap();
        assert_eq!(r["ready_for_preparation"], false);
        assert_eq!(r["submit_permitted"], false);
        assert_eq!(r["issues"].as_array().unwrap().len(), 4);
    }
    fn mail_fixture() -> (tempfile::TempDir, Value) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("applications")).unwrap();
        fs::write(dir.path().join("applications/example-engineer.md"), "# Example — Engineer\n<!-- tracking\n{\"stage\":\"applied\",\"checked\":\"2026-01-01\",\"next_action\":\"Inspect request\",\"company\":\"Example\",\"posting_id\":\"engineer-1\",\"vacancy_url\":\"https://example.test/jobs/engineer\"}\n-->\n").unwrap();
        let observation = json!({"provider":"fixture","thread_id":"thread-1","message_ids":["old-receipt","invite"],"request_message_ids":["invite"],"latest_message_id":"invite","vacancy_url":"https://example.test/jobs/engineer?utm_source=mail","posting_id":"engineer-1","company":"Example","candidate_records":["applications/example-engineer.md"],"requires_reply":true,"replied_to_message_ids":[],"observed_at":"2026-01-01"});
        (dir, observation)
    }
    #[test]
    fn exact_mail_identity_requires_every_supplied_identifier_to_match() {
        let (dir, mut input) = mail_fixture();
        assert_eq!(
            mail(dir.path(), &input).unwrap()["attribution"],
            "proposed_exact_identity"
        );
        input["posting_id"] = json!("different-role");
        let conflict = mail(dir.path(), &input).unwrap();
        assert_eq!(conflict["attribution"], "identity_conflict");
        assert_eq!(conflict["needs_review"], true);
        input["posting_id"] = json!("engineer-1");
        input["vacancy_url"] = json!("https://example.test/jobs/other");
        assert_eq!(
            mail(dir.path(), &input).unwrap()["attribution"],
            "identity_conflict"
        );
        input["vacancy_url"] = Value::Null;
        input["posting_id"] = Value::Null;
        assert_eq!(
            mail(dir.path(), &input).unwrap()["attribution"],
            "ambiguous"
        );
    }
    #[test]
    fn replying_to_latest_invite_resolves_request_without_replying_to_old_receipt() {
        let (dir, mut input) = mail_fixture();
        assert_eq!(mail(dir.path(), &input).unwrap()["needs_attention"], true);
        input["replied_to_message_ids"] = json!(["invite"]);
        let resolved = mail(dir.path(), &input).unwrap();
        assert_eq!(resolved["needs_attention"], false);
        assert_eq!(resolved["needs_review"], false);
        assert_eq!(resolved["archive_permitted"], false);
        input.as_object_mut().unwrap().remove("request_message_ids");
        let uncertain = mail(dir.path(), &input).unwrap();
        assert_eq!(uncertain["needs_attention"], true);
        assert_eq!(uncertain["request_evidence_complete"], false);
    }
    #[test]
    fn fabricated_request_or_reply_message_ids_are_rejected() {
        let (dir, mut input) = mail_fixture();
        input["request_message_ids"] = json!(["not-observed"]);
        assert!(mail(dir.path(), &input).is_err());
        input["request_message_ids"] = json!(["invite"]);
        input["latest_message_id"] = json!("not-observed");
        assert!(mail(dir.path(), &input).is_err());
        input["latest_message_id"] = json!("invite");
        input["replied_to_message_ids"] = json!(["not-observed"]);
        assert!(mail(dir.path(), &input).is_err());
    }
    #[test]
    fn future_or_unparseable_confirmation_cannot_be_reused() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("context")).unwrap();
        fs::write(dir.path().join("context/answer.md"), "Recorded answer").unwrap();
        let mut q = json!({"question_key":"rotation","country":null,"employer":null,"polarity":"affirmative","as_of":"2026-10-12","answer":{"question_key":"rotation","value":true,"source_path":"context/answer.md","source_sha256":storage::digest(b"Recorded answer"),"confirmed_at":"2027-01-01","valid_until":null,"countries":[],"employers":[],"polarity":"affirmative","superseded_by":null}});
        let future = answer(dir.path(), &q).unwrap();
        assert_eq!(future["reusable"], false);
        assert_eq!(future["value"], Value::Null);
        q["answer"]["confirmed_at"] = json!("last autumn");
        assert_eq!(answer(dir.path(), &q).unwrap()["reusable"], false);
        q["answer"]["confirmed_at"] = json!("2026-10-12T09:00:00Z");
        assert_eq!(answer(dir.path(), &q).unwrap()["reusable"], true);
    }
    #[test]
    fn empty_form_requires_affirmative_host_completeness_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let mut input = json!({"vacancy_url":"https://example.test/jobs/engineer","page_revision":"current","readback_revision":"current","observed_at":"2026-01-01","fields":[],"uploads":[],"parsed_claim_changes":[]});
        assert_eq!(
            form(dir.path(), &input).unwrap()["ready_for_preparation"],
            false
        );
        input["completeness_confirmed"] = json!(true);
        assert_eq!(
            form(dir.path(), &input).unwrap()["ready_for_preparation"],
            true
        );
        input["completeness_confirmed"] = json!(false);
        assert_eq!(
            form(dir.path(), &input).unwrap()["ready_for_preparation"],
            false
        );
    }
    #[test]
    fn upload_bytes_do_not_prove_the_host_selected_the_expected_filename() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("output")).unwrap();
        fs::write(dir.path().join("output/cv.pdf"), b"fixture cv").unwrap();
        let mut input = json!({"vacancy_url":"https://example.test/jobs/engineer","page_revision":"current","readback_revision":"current","observed_at":"2026-01-01","fields":[],"uploads":[{"path":"output/cv.pdf","sha256":storage::digest(b"fixture cv"),"observed_name":"old-cv.pdf","observed_present":true}],"parsed_claim_changes":[]});
        let wrong = form(dir.path(), &input).unwrap();
        assert_eq!(wrong["ready_for_preparation"], false);
        assert!(
            wrong["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["kind"] == "upload_filename_mismatch")
        );
        input["uploads"][0]["observed_name"] = json!("cv.pdf");
        assert_eq!(
            form(dir.path(), &input).unwrap()["ready_for_preparation"],
            true
        );
        input["uploads"][0]["expected_name"] = json!("candidate-cv.pdf");
        input["uploads"][0]["observed_name"] = json!("candidate-cv.pdf");
        assert_eq!(
            form(dir.path(), &input).unwrap()["ready_for_preparation"],
            true
        );
    }
}
