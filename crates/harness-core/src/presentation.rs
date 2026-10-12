//! Explicit host-selected imports and cited documents. No fact promotion or external actions.
use crate::{HarnessError, Result, storage, workspace};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_SOURCE_BYTES: usize = 24 * 1024 * 1024;
const MAX_SELECTION_BYTES: usize = 1024 * 1024;
const MAX_CITED_BYTES: usize = 64 * 1024 * 1024;
const MAX_GROUPS: usize = 100;

fn invalid(message: impl Into<String>) -> HarnessError {
    HarnessError::validation(message)
}
fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    // Refuse special files before open, then check the opened handle again.
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(invalid(
            "Select a regular file, not a symlink or special file",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("Select a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(invalid(format!(
            "Selected file exceeds the {maximum}-byte limit"
        )));
    }
    Ok(bytes)
}
fn publish_new(path: &Path, bytes: &[u8]) -> Result<()> {
    storage::reject_symlink_ancestors(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Output needs a parent directory"))?;
    fs::create_dir_all(parent)?;
    storage::reject_symlink_ancestors(path)?;
    let mut stage = tempfile::Builder::new()
        .prefix(".job-harness-source-")
        .tempfile_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        stage
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    stage.write_all(bytes)?;
    stage.as_file().sync_all()?;
    stage.persist_noclobber(path).map_err(|_| {
        HarnessError::new(
            "destination_exists",
            "Output publication refused; existing files are never replaced",
        )
    })?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
fn selected_workspace(root: &Path) -> Result<PathBuf> {
    storage::reject_symlink_ancestors(root)?;
    let root = root.canonicalize()?;
    workspace::assert_external_workspace(&root)?;
    workspace::settings(&root)?;
    Ok(root)
}

/// Preserve original bytes with portable provenance. This does not parse career facts.
pub fn import_source(root: &Path, input: &Path) -> Result<Value> {
    let root = selected_workspace(root)?;
    let filename = input
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| invalid("Supply a UTF-8 filename"))?;
    if filename.len() > 200
        || filename.starts_with('.')
        || filename
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '/' | '\\'))
    {
        return Err(invalid(
            "Use a portable visible filename of at most 200 bytes",
        ));
    }
    let extension = input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["pdf", "json", "md", "txt"].contains(&extension.as_str()) {
        return Err(invalid(
            "Source import accepts PDF, UTF-8 JSON, Markdown or text",
        ));
    }
    let bytes = read_bounded(input, MAX_SOURCE_BYTES)?;
    if bytes.is_empty() {
        return Err(invalid("An empty file is not a supplied source"));
    }
    if extension == "pdf" {
        if !bytes.starts_with(b"%PDF-") {
            return Err(invalid("A supplied PDF must have a PDF file header"));
        }
    } else {
        std::str::from_utf8(&bytes).map_err(|_| invalid("Text sources must be UTF-8"))?;
        if extension == "json" {
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|_| invalid("The supplied JSON is not syntactically valid"))?;
        }
    }
    let sha256 = storage::digest(&bytes);
    let relative = format!("sources/imports/{sha256}/{filename}");
    if !crate::bundle::allowed_path(&relative) {
        return Err(invalid(
            "Source filenames may not name credential, cache or bundle files",
        ));
    }
    let _guard = storage::WorkspaceLock::acquire(&root, true)?;
    let output = workspace::safe_path(&root, &relative)?;
    let provenance_relative = format!("{relative}.provenance.json");
    let sidecar = workspace::safe_path(&root, &provenance_relative)?;
    let existed = output.exists();
    if existed && read_bounded(&output, MAX_SOURCE_BYTES)? != bytes {
        return Err(HarnessError::new(
            "source_conflict",
            "Existing content-addressed source differs; preserve and inspect it",
        ));
    }
    let mut provenance_created = false;
    if sidecar.exists() {
        let old: Value = serde_json::from_slice(&read_bounded(&sidecar, MAX_SELECTION_BYTES)?)?;
        if !existed
            || old["schema_version"] != 1
            || old["sha256"] != sha256
            || old["path"] != relative
            || old["source_kind"] != "user_supplied"
            || old["bytes"] != bytes.len() as u64
        {
            return Err(HarnessError::new(
                "source_conflict",
                "Existing import provenance conflicts with the selected source",
            ));
        }
    } else {
        if !existed {
            publish_new(&output, &bytes)?;
        }
        let provenance = json!({"schema_version":1,"path":relative,"sha256":sha256,"bytes":bytes.len(),"original_filename":filename,"source_kind":"user_supplied","recorded_at":chrono::Utc::now().to_rfc3339(),"facts_extracted":false});
        publish_new(&sidecar, &storage::encoded(&provenance))?;
        provenance_created = true;
    }
    Ok(
        json!({"path":relative,"sha256":sha256,"bytes":bytes.len(),"provenance_path":provenance_relative,"created":!existed,"provenance_created":provenance_created,"source_kind":"user_supplied","facts_extracted":false,"primary_cv_changed":false}),
    )
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema_version: u32,
    title: String,
    status: Status,
    sections: Vec<Section>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approval: Option<Approval>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Draft,
    Approved,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Section {
    heading: String,
    text: String,
    citations: Vec<Citation>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Citation {
    path: String,
    sha256: String,
    start_line: usize,
    end_line: usize,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    approved_by: String,
    approved_at: String,
    evidence: String,
}

/// Templates intentionally contain no candidate facts and cannot be rendered until filled.
pub fn document_template(kind: &str) -> Result<Value> {
    let (title, headings) = match kind {
        "cover_letter" => ("Cover letter", vec!["Why this role", "Relevant experience"]),
        "application_answers" => ("Application answers", vec!["Answer"]),
        "interview_story" => (
            "Interview story",
            vec!["Situation", "Action", "Result", "Reflection"],
        ),
        "portfolio_case_study" => (
            "Portfolio case study",
            vec!["Problem", "Work", "Evidence", "Outcome"],
        ),
        "period_review" => (
            "Period review",
            vec![
                "Period and scope",
                "What happened",
                "What changed",
                "Open questions",
            ],
        ),
        _ => {
            return Err(invalid(
                "Choose cover_letter, application_answers, interview_story, portfolio_case_study or period_review",
            ));
        }
    };
    Ok(
        json!({"schema_version":1,"title":title,"status":"draft","sections":headings.into_iter().map(|heading|json!({"heading":heading,"text":"","citations":[]})).collect::<Vec<_>>()}),
    )
}
fn short_text(value: &str, maximum: usize, name: &str) -> Result<()> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "{name} must be nonblank supplied text of at most {maximum} characters, without control characters"
        )));
    }
    Ok(())
}
fn verify_selection(root: &Path, selection: &Selection) -> Result<usize> {
    if selection.schema_version != 1
        || selection.sections.is_empty()
        || selection.sections.len() > 24
    {
        return Err(invalid(
            "Use document schema_version 1 and 1–24 selected sections",
        ));
    }
    short_text(&selection.title, 300, "title")?;
    if matches!(selection.status, Status::Approved) {
        let approval = selection.approval.as_ref().ok_or_else(|| invalid("Approved documents require supplied approval: approved_by, approved_at and evidence"))?;
        short_text(&approval.approved_by, 300, "approval.approved_by")?;
        short_text(&approval.evidence, 4000, "approval.evidence")?;
        crate::domain::iso_datetime(&approval.approved_at).map_err(|_| {
            invalid("approval.approved_at must be a supplied ISO date or timestamp")
        })?;
    } else if selection.approval.is_some() {
        return Err(invalid(
            "A draft may not carry approval; either remove it or explicitly select approved status",
        ));
    }
    let mut total = 0;
    let mut checked = 0;
    let mut cited_bytes = 0;
    let mut sources: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for (index, section) in selection.sections.iter().enumerate() {
        short_text(&section.heading, 300, &format!("sections[{index}].heading"))?;
        let count = section.text.chars().count();
        total += count;
        if section.text.trim().is_empty() || count > 32_000 || total > 196_000 {
            return Err(invalid(format!(
                "sections[{index}].text needs supplied wording; blank templates are not ready to render (32,000 characters per section, 196,000 total)"
            )));
        }
        if section.citations.is_empty() || section.citations.len() > 20 {
            return Err(invalid(format!(
                "sections[{index}] needs 1–20 exact source citations"
            )));
        }
        for citation in &section.citations {
            let extension = Path::new(&citation.path)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !["md", "txt", "json"].contains(&extension.as_str()) {
                return Err(invalid(
                    "Line citations need Markdown, text or JSON; preserve PDFs and cite an attributed UTF-8 extract",
                ));
            }
            workspace::validate_relative_data(
                &citation.path,
                &["context", "applications", "sources"],
                None,
            )?;
            if !crate::bundle::allowed_path(&citation.path)
                || citation.path.starts_with("applications/operations/")
            {
                return Err(invalid(
                    "Citations must name source evidence, not credentials, caches or coordinator state",
                ));
            }
            if !sources.contains_key(&citation.path) {
                let source = workspace::safe_path(root, &citation.path)?;
                let bytes = read_bounded(&source, MAX_SOURCE_BYTES)?;
                cited_bytes += bytes.len();
                if cited_bytes > MAX_CITED_BYTES {
                    return Err(invalid(
                        "The selected document cites more than 64 MiB of source bytes; select a smaller document",
                    ));
                }
                let text = std::str::from_utf8(&bytes).map_err(|_| invalid("Line citations need UTF-8 text; preserve PDFs as originals and cite an attributed text extract"))?;
                sources.insert(
                    citation.path.clone(),
                    (storage::digest(&bytes), text.lines().count()),
                );
            }
            let (sha256, lines) = &sources[&citation.path];
            if citation.sha256 != *sha256
                || citation.start_line == 0
                || citation.end_line < citation.start_line
                || citation.end_line > *lines
            {
                return Err(HarnessError::new(
                    "stale_source",
                    format!(
                        "Citation hash or line range no longer matches {}",
                        citation.path
                    ),
                ));
            }
            checked += 1;
        }
    }
    Ok(checked)
}
fn escaped_heading(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('`', "\\`")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn source_link(root: &Path, output: &Path, relative: &str) -> String {
    let target = root.join(relative);
    let target: Vec<_> = target.components().collect();
    let parent: Vec<_> = output.parent().unwrap().components().collect();
    let shared = parent
        .iter()
        .zip(&target)
        .take_while(|(a, b)| a == b)
        .count();
    #[cfg(windows)]
    if target.first() != parent.first() {
        // A document saved on another drive cannot use a relative filesystem link.
        return url::Url::from_file_path(root.join(relative))
            .map(|url| url.to_string())
            .unwrap_or_else(|_| relative.to_owned());
    }
    let mut parts = vec!["..".to_owned(); parent.len() - shared];
    parts.extend(target.iter().skip(shared).map(|part| {
        url::form_urlencoded::byte_serialize(part.as_os_str().to_string_lossy().as_bytes())
            .collect::<String>()
            .replace('+', "%20")
    }));
    parts.join("/")
}
fn markdown(root: &Path, output: &Path, selection: &Selection) -> String {
    let mut body = format!("# {}\n\n", escaped_heading(&selection.title));
    if let Some(approval) = &selection.approval {
        body.push_str(&format!(
            "Status: approved by {} on {}.\n\n",
            escaped_heading(&approval.approved_by),
            approval.approved_at
        ));
    } else {
        body.push_str("Status: draft.\n\n");
    }
    for section in &selection.sections {
        body.push_str(&format!(
            "## {}\n\n{}\n\n",
            escaped_heading(&section.heading),
            section.text
        ));
        for citation in &section.citations {
            body.push_str(&format!(
                "- Source: [{} · lines {}–{}]({}#L{}-L{}) · SHA-256 `{}`\n",
                escaped_heading(&citation.path),
                citation.start_line,
                citation.end_line,
                source_link(root, output, &citation.path),
                citation.start_line,
                citation.end_line,
                citation.sha256
            ));
        }
        body.push('\n');
    }
    body.push_str("Citation verification checks exact bytes and line ranges. It does not independently establish the truth of the selected wording.\n");
    body
}

/// Render only selected supplied wording. Existing destinations and primary CVs are preserved.
pub fn render_document(root: &Path, input: &Path, output: &Path, format: &str) -> Result<Value> {
    let root = selected_workspace(root)?;
    let selection: Selection = serde_json::from_slice(&read_bounded(input, MAX_SELECTION_BYTES)?).map_err(|_| invalid("Invalid document selection: use schema_version, title, status, sections and optional approval"))?;
    if !["markdown", "json"].contains(&format) {
        return Err(invalid("Document format must be markdown or json"));
    }
    let raw_output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        root.join(output)
    };
    storage::reject_symlink_ancestors(&raw_output)?;
    let output = workspace::absolute_path(&raw_output)?;
    workspace::assert_external_workspace(&output)?;
    if output.starts_with(&root) && !output.strip_prefix(&root).unwrap().starts_with("output") {
        return Err(HarnessError::new(
            "path_denied",
            "Rendered workspace documents belong under output/; canonical context and coordinator paths are preserved",
        ));
    }
    let settings = workspace::settings(&root)?;
    if let Some(primary) = settings["primary_cv"].as_str() {
        let primary = workspace::safe_path(&root, primary)?;
        if output == primary
            || output.to_string_lossy().to_lowercase() == primary.to_string_lossy().to_lowercase()
        {
            return Err(HarnessError::new(
                "path_denied",
                "Render a separate document; the configured primary CV is preserved",
            ));
        }
    }
    if output.exists() {
        return Err(HarnessError::new(
            "destination_exists",
            "Choose a new output; existing documents are never replaced",
        ));
    }
    let _guard = storage::WorkspaceLock::acquire(&root, false)?;
    let checked = verify_selection(&root, &selection)?;
    let bytes = if format == "json" {
        storage::encoded(&serde_json::to_value(&selection)?)
    } else {
        markdown(&root, &output, &selection).into_bytes()
    };
    publish_new(&output, &bytes)?;
    Ok(
        json!({"output":output,"format":format,"status":selection.status,"bytes":bytes.len(),"sha256":storage::digest(&bytes),"citations_checked":checked,"verification":"exact_source_bytes_and_line_ranges_only","claims_independently_verified":false,"primary_cv_changed":false}),
    )
}

#[derive(Clone, Default, Serialize)]
struct Counts {
    total: usize,
    pending: usize,
    advanced: usize,
    closed_unknown: usize,
    rejected: usize,
    closed_other: usize,
    unclassified: usize,
    submitted_denominator: usize,
    submission_unknown: usize,
}
impl Counts {
    fn add(&mut self, category: &str, submitted: bool) {
        self.total += 1;
        self.submitted_denominator += usize::from(submitted);
        self.submission_unknown += usize::from(!submitted);
        match category {
            "pending" => self.pending += 1,
            "advanced" => self.advanced += 1,
            "closed_unknown" => self.closed_unknown += 1,
            "rejected" => self.rejected += 1,
            "closed_other" => self.closed_other += 1,
            _ => self.unclassified += 1,
        }
    }
}
fn groups(groups: BTreeMap<Option<String>, Counts>) -> Value {
    let total = groups.len();
    json!({"groups":groups.into_iter().take(MAX_GROUPS).map(|(value,counts)|json!({"value":value,"counts":counts})).collect::<Vec<_>>(),"total_groups":total,"has_more":total>MAX_GROUPS,"order":"label_ascending_not_performance"})
}
/// Descriptive recorded outcomes only. Absence and closed states are not inferred rejections.
pub fn outcomes(root: &Path) -> Result<Value> {
    let root = selected_workspace(root)?;
    let _guard = storage::WorkspaceLock::acquire(&root, false)?;
    let records = crate::pipeline::build(&root)?.records;
    let mut counts = Counts::default();
    let mut discovery = BTreeMap::<Option<String>, Counts>::new();
    let mut ats = BTreeMap::<Option<String>, Counts>::new();
    let mut structured = 0;
    let mut explicit_outcomes = 0;
    let mut closed_with_unknown_submission = 0;
    for row in records {
        let canonical = row["tracking_source"] == "structured";
        structured += usize::from(canonical);
        let outcome = row["outcome"].as_str().filter(|s| !s.trim().is_empty());
        explicit_outcomes += usize::from(outcome.is_some());
        let explicit_rejection = outcome.is_some_and(|s| {
            ["rejection", "rejected"].contains(&s.trim().to_ascii_lowercase().as_str())
        });
        let known_outcome = outcome.is_some_and(|s| {
            !["unknown", "outcome_unknown", "pending", "awaiting_outcome"]
                .contains(&s.trim().to_ascii_lowercase().as_str())
        });
        let category = if explicit_rejection {
            "rejected"
        } else if !canonical {
            "unclassified"
        } else {
            match row["stage"].as_str().unwrap_or("") {
                "saved" | "prepared" | "applied" => "pending",
                "introduction" | "screening" | "technical" | "final" | "offer" => "advanced",
                "closed" if known_outcome => "closed_other",
                "closed" => "closed_unknown",
                _ => "unclassified",
            }
        };
        let submitted = canonical
            && ([
                "applied",
                "introduction",
                "screening",
                "technical",
                "final",
                "offer",
            ]
            .contains(&row["stage"].as_str().unwrap_or(""))
                || row["submission_status"] == "submitted"
                || row["submitted"] == "true"
                || row["submitted_at"]
                    .as_str()
                    .is_some_and(|date| crate::domain::iso_datetime(date).is_ok()));
        if canonical && row["stage"] == "closed" && !submitted {
            closed_with_unknown_submission += 1;
        }
        counts.add(category, submitted);
        for (field, grouped) in [
            ("discovery_channel", &mut discovery),
            ("ats_host", &mut ats),
        ] {
            let value = canonical
                .then(|| row[field].as_str())
                .flatten()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned);
            grouped.entry(value).or_default().add(category, submitted);
        }
    }
    Ok(
        json!({"counts":counts,"count_unit":"recorded_vacancies","discovery_channel":groups(discovery),"ats_host":groups(ats),"coverage":{"structured_records":structured,"legacy_records":counts.total-structured,"recorded_outcome_values":explicit_outcomes,"closed_with_unknown_submission":closed_with_unknown_submission},"submitted_denominator_basis":"Only canonical applied/introduction/screening/technical/final/offer stages or explicit submission_status=submitted, submitted=true, or a valid submitted_at. Closed/outcome alone never implies submission. This is recorded local state, including direct recruiting progression, not live ATS confirmation.","live_providers_checked":false,"authority":"recorded_canonical_tracking_only","classification":{"pending":["saved","prepared","applied"],"advanced":["introduction","screening","technical","final","offer"],"rejected":"Only explicit tracking.outcome=rejected|rejection","closed_unknown":"Closed without a known recorded outcome","closed_other":"Closed with a known recorded non-rejection outcome","unclassified":"Needs-review or legacy prose; no progression inferred"},"attribution":"Discovery channel and ATS host are independent supplied tracking fields; absent attribution is null. URLs, referrers and source labels are not used to guess attribution."}),
    )
}

/// One bounded read batch for the human status view or an agent's compact JSON snapshot.
pub fn status_snapshot(root: &Path) -> Result<Value> {
    let engine = crate::Engine::open(root)?;
    let result = engine.call(
        "query_batch",
        &json!({"steps":[
            {"id":"health","tool":"bootstrap","arguments":{}},
            {"id":"attention","tool":"review_queue","arguments":{"limit":5}}
        ]}),
    );
    if result["ok"] != true {
        return Ok(result);
    }
    let reads = &result["data"]["results"];
    Ok(
        json!({"schema_version":2,"ok":true,"data":{"workspace":engine.root(),"health":reads[0]["result"]["data"]["health"],"attention":reads[1]["result"]["data"]},"error":null,"next":reads[0]["result"]["next"]}),
    )
}
fn terminal_text(text: &str, maximum: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(maximum)
        .collect()
}
/// Plain terminal presentation, never ANSI control sequences or a replacement tracker.
pub fn status_text(snapshot: &Value) -> Result<String> {
    if snapshot["ok"] != true {
        return Err(invalid("Status needs a successful read snapshot"));
    }
    let data = &snapshot["data"];
    let health = &data["health"];
    let queue = &data["attention"];
    let items = queue["items"]
        .as_array()
        .ok_or_else(|| invalid("Status snapshot lacks attention items"))?;
    let total = queue["page"]["total"].as_u64().unwrap_or(0);
    let mut text = format!(
        "Job Harness · Workspace\n\n  {}\n  Records {}  · Open runs {}\n  Recovery: {} unresolved attempts · {} begun/unknown\n  Pipeline views: {}\n\nNeeds attention · {} of {} shown\n",
        terminal_text(data["workspace"].as_str().unwrap_or("Unknown root"), 160),
        health["records"],
        health["unfinished_run_count"],
        health["unresolved_attempt_count"],
        queue["counts"]["unknown_submissions"],
        if health["pipeline_stale"] == true {
            "stale; rebuild explicitly"
        } else {
            "current"
        },
        items.len(),
        total
    );
    if items.is_empty() {
        text.push_str("  No recorded items currently need attention.\n");
    }
    for item in items {
        let title = [
            "company",
            "title",
            "subject",
            "record_path",
            "note_id",
            "attempt_id",
        ]
        .into_iter()
        .find_map(|field| item[field].as_str())
        .unwrap_or("Recorded work");
        let kind = item["kind"].as_str().unwrap_or("review").replace('_', " ");
        text.push_str(&format!(
            "  • {} · {}\n",
            terminal_text(&kind, 40),
            terminal_text(title, 100)
        ));
        if let Some(next) = item["next_action"].as_str() {
            text.push_str(&format!("    {}\n", terminal_text(next, 160)));
        }
    }
    let offset = queue["page"]["next_offset"].as_u64().unwrap_or(0);
    text.push_str(&format!("\nNext tool: review_queue {{\"offset\":{offset},\"limit\":5}}\nRecorded local state · live mailbox not checked.\n"));
    if queue["note_reviews_complete"] == false {
        text.push_str("Note-review coverage is partial; page list_notes for all retained notes.\n");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn windows_document_links_across_drives_use_an_absolute_file_url() {
        assert_eq!(
            source_link(
                Path::new(r"C:\Career"),
                Path::new(r"D:\Drafts\answer.md"),
                "context/project notes.md"
            ),
            "file:///C:/Career/context/project%20notes.md"
        );
    }
    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("career");
        workspace::initialize(&root).unwrap();
        (temp, root)
    }
    fn selection(root: &Path) -> Value {
        let source = b"Built a customer dashboard.\nUnselected private detail must stay out of the rendered document.\n";
        fs::write(root.join("context/example.md"), source).unwrap();
        json!({"schema_version":1,"title":"Selected example","status":"draft","sections":[{"heading":"Relevant experience","text":"Built a customer dashboard.","citations":[{"path":"context/example.md","sha256":storage::digest(source),"start_line":1,"end_line":1}]}]})
    }
    #[test]
    fn source_bytes_provenance_and_primary_selection_survive_import_replay_and_bundle() {
        let (temp, root) = fixture();
        let source = temp.path().join("original.pdf");
        let bytes = b"%PDF-1.4\nSynthetic supplied original, not a candidate CV.\n%%EOF\n";
        fs::write(&source, bytes).unwrap();
        let config = fs::read(root.join("config/career-ledger.json")).unwrap();
        let imported = import_source(&root, &source).unwrap();
        let relative = imported["path"].as_str().unwrap();
        assert_eq!(fs::read(root.join(relative)).unwrap(), bytes);
        assert_eq!(imported["created"], true);
        let sidecar = imported["provenance_path"].as_str().unwrap();
        let provenance_bytes = fs::read(root.join(sidecar)).unwrap();
        let provenance: Value = serde_json::from_slice(&provenance_bytes).unwrap();
        assert_eq!(provenance["source_kind"], "user_supplied");
        assert_eq!(provenance["facts_extracted"], false);
        assert!(provenance["recorded_at"].is_string());
        assert_eq!(import_source(&root, &source).unwrap()["created"], false);
        assert_eq!(fs::read(root.join(sidecar)).unwrap(), provenance_bytes);
        assert_eq!(
            fs::read(root.join("config/career-ledger.json")).unwrap(),
            config
        );
        let bundle = temp.path().join("private.jhb");
        crate::bundle::export_bundle(&root, &bundle, "workspace").unwrap();
        let restored = temp.path().join("restored");
        crate::bundle::import_bundle(&restored, &bundle).unwrap();
        assert_eq!(fs::read(restored.join(relative)).unwrap(), bytes);
        assert_eq!(fs::read(restored.join(sidecar)).unwrap(), provenance_bytes);
        fs::write(root.join(relative), b"Existing corrupt bytes").unwrap();
        assert_eq!(
            import_source(&root, &source).unwrap_err().code,
            "source_conflict"
        );
        assert_eq!(
            fs::read(root.join(relative)).unwrap(),
            b"Existing corrupt bytes"
        );
    }
    #[test]
    fn source_import_bounds_formats_and_symlink_destinations() {
        let (temp, root) = fixture();
        for (name, bytes) in [
            ("bad.json", &b"{broken"[..]),
            ("binary.md", &b"\xff"[..]),
            ("wrong.pdf", &b"text"[..]),
            ("credentials.json", &b"{}"[..]),
        ] {
            let file = temp.path().join(name);
            fs::write(&file, bytes).unwrap();
            assert!(import_source(&root, &file).is_err());
        }
        let huge = temp.path().join("large.txt");
        fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_SOURCE_BYTES as u64 + 1)
            .unwrap();
        assert!(import_source(&root, &huge).is_err());
        #[cfg(unix)]
        {
            let outside = temp.path().join("outside");
            fs::create_dir(&outside).unwrap();
            fs::create_dir_all(root.join("sources")).unwrap();
            std::os::unix::fs::symlink(&outside, root.join("sources/imports")).unwrap();
            let file = temp.path().join("safe.txt");
            fs::write(&file, b"Supplied text").unwrap();
            assert!(import_source(&root, &file).is_err());
            assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        }
    }
    #[test]
    fn document_renders_only_selected_text_and_rejects_stale_citations_and_overwrite() {
        let (temp, root) = fixture();
        let selected = selection(&root);
        let input = temp.path().join("selection.json");
        fs::write(&input, storage::encoded(&selected)).unwrap();
        let output = Path::new("output/letter.md");
        let result = render_document(&root, &input, output, "markdown").unwrap();
        assert_eq!(result["citations_checked"], 1);
        assert_eq!(result["claims_independently_verified"], false);
        let rendered = fs::read_to_string(root.join(output)).unwrap();
        assert!(rendered.contains("Built a customer dashboard."));
        assert!(!rendered.contains("Unselected private detail"));
        assert!(rendered.contains("../context/example.md#L1-L1"));
        assert_eq!(
            render_document(&root, &input, output, "markdown")
                .unwrap_err()
                .code,
            "destination_exists"
        );
        let before = fs::read(root.join(output)).unwrap();
        fs::write(root.join("context/example.md"), "Changed source.\n").unwrap();
        assert_eq!(
            render_document(&root, &input, Path::new("output/stale.md"), "markdown")
                .unwrap_err()
                .code,
            "stale_source"
        );
        assert!(!root.join("output/stale.md").exists());
        assert_eq!(fs::read(root.join(output)).unwrap(), before);
    }
    #[test]
    fn templates_missing_facts_approval_primary_and_checkout_destinations_are_explicit() {
        let (temp, root) = fixture();
        let input = temp.path().join("selection.json");
        for kind in [
            "cover_letter",
            "application_answers",
            "interview_story",
            "portfolio_case_study",
            "period_review",
        ] {
            fs::write(&input, storage::encoded(&document_template(kind).unwrap())).unwrap();
            let error = render_document(&root, &input, Path::new("output/blank.md"), "markdown")
                .unwrap_err();
            assert!(error.message.contains("blank templates"));
        }
        let mut selected = selection(&root);
        selected["status"] = json!("approved");
        fs::write(&input, storage::encoded(&selected)).unwrap();
        assert!(
            render_document(&root, &input, Path::new("output/unapproved.json"), "json")
                .unwrap_err()
                .message
                .contains("supplied approval")
        );
        selected["approval"] = json!({"approved_by":"Synthetic candidate","approved_at":"2026-10-12","evidence":"Reviewed selected wording against the cited source."});
        fs::write(&input, storage::encoded(&selected)).unwrap();
        let external = temp.path().join("selected.json");
        render_document(&root, &input, &external, "json").unwrap();
        let rendered: Value = serde_json::from_slice(&fs::read(&external).unwrap()).unwrap();
        assert_eq!(rendered, selected);
        let config_path = root.join("config/career-ledger.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        config["primary_cv"] = json!("output/primary.pdf");
        fs::write(&config_path, storage::encoded(&config)).unwrap();
        assert_eq!(
            render_document(&root, &input, Path::new("output/primary.pdf"), "json")
                .unwrap_err()
                .code,
            "path_denied"
        );
        let checkout = temp.path().join("generic");
        fs::create_dir_all(checkout.join("resources/schemas")).unwrap();
        fs::write(checkout.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(checkout.join("resources/schemas/tools.json"), "{}").unwrap();
        assert!(render_document(&root, &input, &checkout.join("private.md"), "markdown").is_err());
        assert!(!checkout.join("private.md").exists());
        let mut wrong_lines = selected.clone();
        wrong_lines["sections"][0]["citations"][0]["end_line"] = json!(999);
        fs::write(&input, storage::encoded(&wrong_lines)).unwrap();
        assert_eq!(
            render_document(&root, &input, Path::new("output/range.md"), "markdown")
                .unwrap_err()
                .code,
            "stale_source"
        );
    }
    #[test]
    fn outcomes_keep_missing_attribution_and_closed_states_distinct_from_rejections() {
        let (_temp, root) = fixture();
        for (name, extra) in [
            (
                "pending",
                json!({"stage":"applied","source":"Welcome to the Jungle","vacancy_url":"https://jobs.ashbyhq.com/example?utm_source=cord"}),
            ),
            (
                "advanced",
                json!({"stage":"technical","discovery_channel":"cord","ats_host":"ashby"}),
            ),
            (
                "rejected",
                json!({"stage":"closed","outcome":"rejected","submitted_at":"2026-10-10","discovery_channel":"cord","ats_host":"ashby"}),
            ),
            (
                "closed",
                json!({"stage":"closed","status_detail":"Rejected? Inspect the original message."}),
            ),
            (
                "unknown",
                json!({"stage":"closed","outcome":"outcome_unknown"}),
            ),
            ("withdrawn", json!({"stage":"closed","outcome":"withdrawn"})),
        ] {
            let mut tracking =
                json!({"stage":"saved","checked":"2026-10-12","next_action":"Inspect the source"});
            for (key, value) in extra.as_object().unwrap() {
                tracking[key] = value.clone();
            }
            fs::write(
                root.join(format!("applications/{name}.md")),
                format!(
                    "# Example {name}\n\n<!-- tracking\n{}\n-->\n",
                    serde_json::to_string(&tracking).unwrap()
                ),
            )
            .unwrap();
        }
        fs::write(
            root.join("applications/legacy.md"),
            "# Legacy example\n\nStatus: Applied\n",
        )
        .unwrap();
        let report = outcomes(&root).unwrap();
        assert_eq!(
            report["counts"],
            json!({"total":7,"pending":1,"advanced":1,"rejected":1,"closed_unknown":2,"closed_other":1,"unclassified":1,"submitted_denominator":3,"submission_unknown":4})
        );
        assert_eq!(
            report["discovery_channel"]["groups"][0]["value"],
            Value::Null
        );
        assert_eq!(
            report["discovery_channel"]["groups"][0]["counts"]["total"],
            5
        );
        assert_eq!(report["ats_host"]["groups"][0]["counts"]["total"], 5);
        assert_eq!(report["coverage"]["legacy_records"], 1);
        assert_eq!(report["coverage"]["closed_with_unknown_submission"], 3);
        assert_eq!(report["live_providers_checked"], false);
    }
    #[test]
    fn document_spaces_link_correctly_and_in_workspace_canonical_paths_are_denied() {
        let (temp, root) = fixture();
        let mut selected = selection(&root);
        fs::rename(
            root.join("context/example.md"),
            root.join("context/project notes.md"),
        )
        .unwrap();
        selected["sections"][0]["citations"][0]["path"] = json!("context/project notes.md");
        let input = temp.path().join("selection.json");
        fs::write(&input, storage::encoded(&selected)).unwrap();
        assert_eq!(
            render_document(&root, &input, Path::new("context/new-facts.md"), "markdown")
                .unwrap_err()
                .code,
            "path_denied"
        );
        assert!(!root.join("context/new-facts.md").exists());
        render_document(&root, &input, Path::new("output/space.md"), "markdown").unwrap();
        assert!(
            fs::read_to_string(root.join("output/space.md"))
                .unwrap()
                .contains("../context/project%20notes.md#L1-L1")
        );
    }
    #[test]
    fn status_is_bounded_read_only_and_terminal_control_safe() {
        let (_temp, root) = fixture();
        let state_path = root.join("applications/operations/state.json");
        let before = fs::read(&state_path).unwrap();
        let result = status_snapshot(&root).unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["data"]["health"]["records"], 0);
        assert_eq!(fs::read(&state_path).unwrap(), before);
        let text = status_text(&result).unwrap();
        assert!(text.contains("0 of 0 shown"));
        assert!(text.contains("Next tool: review_queue"));
        assert!(!text.contains('\u{1b}'));
        assert_eq!(
            terminal_text("unsafe\u{1b}[0m\nvalue", 30),
            "unsafe [0m value"
        );
    }
}
