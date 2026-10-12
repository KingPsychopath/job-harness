//! Optional capability selection is a plan, never a hidden installer or permission grant.
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::Path,
};

pub const IDS: &[&str] = &[
    "mcp",
    "acp",
    "models",
    "retrieval",
    "decisions",
    "cv",
    "desktop",
];

pub fn catalog() -> Value {
    json!({
        "schema_version":1,
        "base":{"name":"Career workspace","included":["scoped answers and evidence","application coordination and recovery","exact search","source-preserving notes","private backup and restore","CLI, MCP and local dashboard"],"runtime":"Native executable; no Python, model, network or VM required","download":"Choose your platform's release archive; exact sizes and SHA256 are published with the release"},
        "optional":[
            {"id":"mcp","name":"Use your agent","purpose":"Connect these tools to an MCP-capable host","install":"No extra harness download","access":"Your host's browser, mail and calendar sessions","next":"jh connect mcp"},
            {"id":"acp","name":"Launch an agent","purpose":"Hand a task and fixed workspace to your chosen ACP agent","install":"Install/authenticate that agent separately","access":"Agent-owned credentials and host policy","next":"jh acp --help"},
            {"id":"models","name":"Bring a model","purpose":"Use a compatible local or remote generative endpoint","install":"Choose your existing runtime or provider; no bundled model","access":"Remote HTTPS and an injected named key, or local loopback","next":"jh agent run --help"},
            {"id":"retrieval","name":"Find related context","purpose":"Add semantic retrieval alongside exact search","install":"Optional native provider plus selected Gemma weights (157 MiB)","access":"Explicitly selected source text only; model/hardware details in context models","next":"jh context models"},
            {"id":"decisions","name":"Review typed suggestions","purpose":"Ask bounded questions about selected text; abstentions remain visible","install":"Optional native provider plus Laya weights (647 MiB); other model profiles have separate requirements","access":"Selected text only; no unattended email archive or submission","next":"jh context models"},
            {"id":"cv","name":"Create a CV","purpose":"Compile an evidence-backed selected CV; an existing PDF also works","install":"Optional Typst executable and chosen fonts; no model required","access":"Selected approved content and cited evidence","next":"jh cv --help"},
            {"id":"desktop","name":"Use a desktop window","purpose":"Open the same local workspace in an optional native window","install":"Separate desktop package and platform webview dependencies; CLI stays independent","access":"Local loopback only; external connector access remains in your host","next":"See apps/desktop/README.md"}
        ],
        "policy":"Selecting a capability does not download software, contact a provider or grant an external action.",
        "details":"https://github.com/KingPsychopath/job-harness/blob/main/docs/INSTALL.md"
    })
}

pub fn plan(selected: &[String]) -> Result<Value> {
    if let Some(id) = selected.iter().find(|id| !IDS.contains(&id.as_str())) {
        bail!("Unknown capability {id}. Run jh capabilities for the available choices.");
    }
    let catalog = catalog();
    let choices: Vec<_> = catalog["optional"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| selected.iter().any(|id| entry["id"] == id.as_str()))
        .cloned()
        .collect();
    Ok(
        json!({"base":catalog["base"],"selected":choices,"policy":catalog["policy"],"downloads_started":false,"external_access_granted":false}),
    )
}

pub fn prompt() -> Result<Vec<String>> {
    let catalog = catalog();
    eprintln!("What would you like to do? The base toolkit is always included.");
    for (index, item) in catalog["optional"].as_array().unwrap().iter().enumerate() {
        eprintln!(
            "  {}. {} — {}",
            index + 1,
            item["name"].as_str().unwrap(),
            item["purpose"].as_str().unwrap()
        );
    }
    eprint!("Choose numbers separated by spaces, or Enter for the base: ");
    io::stderr().flush()?;
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        bail!("Setup cancelled: no choice was received. No workspace or settings were changed.");
    }
    line.split_whitespace()
        .map(|word| {
            let index = word
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=IDS.len()).contains(n))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Use the displayed numbers. No workspace or settings were changed."
                    )
                })?;
            Ok(IDS[index - 1].to_string())
        })
        .collect()
}

pub fn setup(root: &Path, selected: &[String], preview: bool) -> Result<Value> {
    let mut plan = plan(selected)?;
    plan["workspace"] = json!(root);
    if preview {
        plan["configured"] = json!(false);
        return Ok(plan);
    }
    let created = !root.exists() || (root.is_dir() && root.read_dir()?.next().is_none());
    if created {
        job_harness_core::workspace::initialize(root)?;
    }
    let settings = super::host::configure(root)?;
    plan["workspace"] = settings["workspace"].clone();
    plan["created"] = json!(created);
    plan["configured"] = json!(true);
    plan["next"] = json!(["jh status", "jh connect mcp"]);
    Ok(plan)
}

pub fn demo(root: &Path) -> Result<Value> {
    job_harness_core::workspace::initialize(root)?;
    let project = root.join("context/projects");
    std::fs::create_dir_all(&project)?;
    job_harness_core::storage::atomic_write(&project.join("dashboard.md"), "# Atlas dashboard — fictional demo\n\nSource: invented example for Job Harness, not a real candidate.\n\nBuilt a customer dashboard and worked with support to improve usability.\nInvestigated a slow database query and compared the query plan before changing the index.\nNo measured improvement is claimed.\n".as_bytes())?;
    for (slug, company, role, stage, action) in [
        (
            "northstar",
            "Northstar",
            "Senior Product Engineer",
            "saved",
            "Review the engineering scope",
        ),
        (
            "cedar",
            "Cedar Health",
            "Software Engineer",
            "prepared",
            "Review selected answers",
        ),
        (
            "orbit",
            "Orbit Labs",
            "Platform Engineer",
            "applied",
            "Await a verified employer response",
        ),
        (
            "loom",
            "Loom Systems",
            "Senior Full Stack Engineer",
            "screening",
            "Prepare a sourced project story",
        ),
    ] {
        let tracking = json!({"company":company,"stage":stage,"checked":"2026-10-12","vacancy_url":format!("https://example.com/{slug}"),"source":"Fictional demo","next_action":action});
        let content = format!(
            "# {company} — {role}\n\n<!-- tracking\n{tracking}\n-->\n\nFictional vacancy for a local demo. No application was submitted.\n"
        );
        job_harness_core::storage::atomic_write(
            &root.join(format!("applications/{slug}.md")),
            content.as_bytes(),
        )?;
    }
    let engine = job_harness_core::Engine::open(root)?;
    let note = engine.call("intake_note", &json!({"text":"# Interview notes — fictional demo\nDiscussed the dashboard project and database query investigation.\nTechnical round next week; exact date has not been supplied.\n","source_kind":"personal_notes","source_label":"Fictional screening notes","links":[{"kind":"application","path":"applications/loom.md"},{"kind":"project","path":"context/projects/dashboard.md"}],"idempotency_key":"demo:note"}));
    if note["ok"] != true {
        bail!(
            "Could not preserve fictional demo note: {}",
            note["error"]["code"]
        );
    }
    job_harness_core::pipeline::write(root)?;
    job_harness_core::index::refresh(root)?;
    let search = job_harness_core::index::search(root, "database", 5)?;
    Ok(
        json!({"workspace":root.canonicalize()?,"fictional":true,"external_actions":false,"search":search,"note":note["data"],"next":["jh --root PATH status","jh --root PATH workspace search database","jh --root PATH gui"]}),
    )
}

pub fn text(data: &Value) -> String {
    let mut output = String::from(
        "Job Harness\n\nThe native base includes your workspace, exact search, notes, recovery and agent tools.\n",
    );
    if let Some(workspace) = data["workspace"].as_str() {
        output.push_str(&format!("Workspace: {workspace}\n"));
    }
    let choices = data.get("selected").or_else(|| data.get("optional"));
    if let Some(items) = choices.and_then(Value::as_array) {
        for item in items {
            output.push_str(&format!(
                "\n{} ({})\n  {}\n  {}\n  Next: {}\n",
                item["name"].as_str().unwrap(),
                item["id"].as_str().unwrap(),
                item["purpose"].as_str().unwrap(),
                item["install"].as_str().unwrap(),
                item["next"].as_str().unwrap()
            ));
        }
    }
    output.push_str("\nNo optional software was downloaded. Credentials and action permissions stay with your host.\n");
    if data["configured"] == true {
        output.push_str("Run jh status, then jh connect mcp to wire your agent.\n");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_choice_and_preview_have_no_filesystem_effects() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("new");
        assert!(setup(&root, &["invented".into()], false).is_err());
        let result = setup(&root, &["retrieval".into(), "mcp".into()], true).unwrap();
        assert_eq!(result["selected"].as_array().unwrap().len(), 2);
        assert_eq!(result["downloads_started"], false);
        assert!(!root.exists());
    }
    #[test]
    fn catalog_always_includes_offline_base_and_separate_access() {
        let result = plan(&[]).unwrap();
        assert!(
            result["base"]["runtime"]
                .as_str()
                .unwrap()
                .contains("no Python")
        );
        assert_eq!(result["external_access_granted"], false);
        assert!(result["selected"].as_array().unwrap().is_empty());
    }
    #[test]
    fn fictional_demo_is_searchable_and_refuses_existing_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("demo");
        let result = demo(&root).unwrap();
        assert_eq!(result["fictional"], true);
        assert_eq!(result["external_actions"], false);
        assert!(result["search"].to_string().contains("dashboard.md"));
        assert!(demo(&root).is_err());
    }
}
