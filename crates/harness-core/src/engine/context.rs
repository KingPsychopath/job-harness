//! A bounded evidence handoff: exact passages and supplied provenance, no interpretation.
use super::*;

impl Engine {
    pub(super) fn context_pack(&self, args: &Value) -> Result<Value> {
        let limit = n(args, "limit", 6);
        let task = s(args, "task")?;
        let query = args.get("query").and_then(Value::as_str).unwrap_or("");
        let mut candidates = vec![];
        let mut seen = BTreeSet::new();
        let mut gaps = vec![];
        let mut company_rows = BTreeMap::new();
        {
            let mut add = |path: String| {
                if seen.insert(path.clone()) {
                    candidates.push(path);
                }
            };
            if let Some(paths) = args.get("paths") {
                for path in string_array(paths)? {
                    add(path);
                }
            }
            if let Some(project) = args.get("project").and_then(Value::as_str)
                && project.starts_with("context/projects/")
            {
                // Explicit project scope is a seed, even when the query has no
                // lexical hits. Its linked intake is expanded below.
                self.safe_path(project, READ_ROOTS, &["md", "json"])?;
                add(project.to_owned());
            }
            if let Some(company) = args.get("company").and_then(Value::as_str) {
                for row in pipeline::build(&self.root)?.records {
                    let employer =
                        row.get("company")
                            .and_then(Value::as_str)
                            .unwrap_or_else(|| {
                                row["title"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .split('—')
                                    .next()
                                    .unwrap_or_default()
                                    .trim()
                            });
                    if domain::company_key(company) != domain::company_key(employer) {
                        continue;
                    }
                    let path = format!("applications/{}", s(&row, "record")?);
                    company_rows.insert(path.clone(), row.clone());
                    add(path);
                    if let Some(brief) = row.get("brief").and_then(Value::as_str) {
                        add(format!("applications/{brief}"));
                    }
                }
            }
            let mut searches = Vec::new();
            if let Some(project) = args.get("project").and_then(Value::as_str)
                && !project.starts_with("context/projects/")
            {
                searches.push(project);
            }
            if !query.trim().is_empty() && !searches.contains(&query) {
                searches.push(query);
            }
            for search_query in searches {
                match crate::index::search(&self.root, search_query, (limit * 3).min(100)) {
                    Ok(result) => {
                        for row in result["matches"].as_array().into_iter().flatten() {
                            if let Some(path) = row.get("path").and_then(Value::as_str) {
                                add(path.into());
                            }
                        }
                        if result["coverage_complete"] == false {
                            gaps.push(json!({"code":"partial_search_coverage","path":null,"message":"The derived search index skipped sources; this pack does not claim complete retrieval."}));
                        }
                        if result["stale"] == true {
                            gaps.push(json!({"code":"stale_index","path":null,"message":"The derived search index reports stale sources; selected passages are re-read and hashed."}));
                        }
                    }
                    Err(_) => gaps.push(json!({"code":"search_unavailable","path":null,"message":"The derived index was unavailable; explicit scoped sources are still read directly."})),
                }
            }
        }
        if candidates.is_empty() && !args.get("project").is_some_and(Value::is_string) {
            for path in string_array(&self.settings()?["answer_sources"])? {
                if seen.insert(path.clone()) {
                    candidates.push(path);
                }
            }
        }
        if !candidates.is_empty() {
            match crate::index::related_paths(&self.root,&candidates.iter().take(100).cloned().collect::<Vec<_>>(),(limit*3).min(100)) {
                Ok(paths)=>{for path in paths {if path.starts_with("context/intake/") && path.ends_with("/metadata.json") {continue;}
if seen.insert(path.clone()) {candidates.push(path);}}},
                Err(_)=>gaps.push(json!({"code":"relationships_unavailable","path":null,"message":"Derived source relationships were unavailable; explicit scoped sources remain readable."})),
            }
        }
        let selected_candidates = candidates.len();
        let mut sources = vec![];
        let terms = words(query);
        let mut remaining = 8_000usize;
        let requested = args.get("period");
        let start = requested
            .and_then(|period| period.get("start"))
            .and_then(Value::as_str)
            .map(domain::iso_datetime)
            .transpose()?
            .map(|(date, _)| date.date());
        let end = requested
            .and_then(|period| period.get("end"))
            .and_then(Value::as_str)
            .map(domain::iso_datetime)
            .transpose()?
            .map(|(date, _)| date.date());
        if start.zip(end).is_some_and(|(start, end)| start > end) {
            return Err(Error::new(
                "invalid_arguments",
                "Context period start must not follow its end",
            ));
        }
        let basis = requested
            .and_then(|period| period.get("basis"))
            .and_then(Value::as_str)
            .unwrap_or("occurred");
        for path in candidates {
            if sources.len() >= limit || remaining == 0 {
                break;
            }
            let target = match self.safe_path(&path, READ_ROOTS, &["md", "json"]) {
                Ok(target) => target,
                Err(_) => {
                    gaps.push(json!({"code":"source_denied","path":path,"message":"This source is outside the readable workspace allowlist."}));
                    continue;
                }
            };
            let bytes = match fs::read(&target) {
                Ok(bytes) if bytes.len() <= 1_000_000 => bytes,
                _ => {
                    gaps.push(json!({"code":"source_unavailable","path":path,"message":"The exact source is missing, unreadable or larger than one MB."}));
                    continue;
                }
            };
            let hash = domain::digest(&bytes);
            let content = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    gaps.push(json!({"code":"invalid_source_encoding","path":path,"message":"This source is not valid UTF-8 text."}));
                    continue;
                }
            };
            let kind = crate::index::source_kind(&self.root, &path);
            let mut provenance = json!({"source_kind":kind,"recorded_at":null,"occurred_at":null,"occurred_period":null,"valid_from":null,"valid_to":null,"supersedes":null,"source_group":null,"date_confidence":"unknown"});
            let mut stale = false;
            if path.starts_with("context/intake/") {
                let components: Vec<_> = path.split('/').collect();
                if components.len() >= 4 {
                    let meta_path = format!("context/intake/{}/metadata.json", components[2]);
                    if let Ok(target) = self.text_path(&meta_path)
                        && let Ok(bytes) = fs::read(target)
                        && let Ok(meta) = serde_json::from_slice::<Value>(&bytes)
                    {
                        for key in [
                            "recorded_at",
                            "occurred_at",
                            "occurred_period",
                            "valid_from",
                            "valid_to",
                            "supersedes",
                            "source_group",
                            "source_label",
                            "source_uri",
                        ] {
                            if let Some(value) = meta.get(key) {
                                provenance[key] = value.clone();
                            }
                        }
                        provenance["date_confidence"] = json!(if meta["occurred_at"].is_string() {
                            "supplied_exact"
                        } else if meta["occurred_period"].is_string() {
                            "supplied_approximate"
                        } else {
                            "unknown"
                        });
                        if meta["original"]["path"] == path && meta["original"]["sha256"] != hash {
                            stale = true;
                            gaps.push(json!({"code":"source_changed","path":path,"message":"The original bytes differ from their recorded intake hash."}));
                        }
                        if !meta["supersedes"].is_null() {
                            gaps.push(json!({"code":"supersession_supplied","path":path,"message":"Supersession is supplied metadata; review both sources before treating one as the current fact."}));
                        }
                    }
                }
            }
            if let Some(row) = company_rows.get(&path) {
                provenance["checked_at"] = row.get("checked").cloned().unwrap_or(Value::Null);
            }
            let date_field = if basis == "recorded" {
                "recorded_at"
            } else {
                "occurred_at"
            };
            let date = provenance[date_field]
                .as_str()
                .and_then(|value| domain::iso_datetime(value).ok())
                .map(|(date, _)| date.date());
            if start.is_some() || end.is_some() {
                if let Some(date) = date {
                    if start.is_some_and(|start| date < start) || end.is_some_and(|end| date > end)
                    {
                        continue;
                    }
                } else {
                    gaps.push(json!({"code":"period_membership_unknown","path":path,"message":"No exact supplied date supports this period filter. Rough labels are preserved and this source remains visible."}));
                }
            }
            if provenance["date_confidence"] == "unknown" {
                gaps.push(json!({"code":"unknown_occurrence_date","path":path,"message":"The occurrence date is unknown; recorded, checked and filesystem dates are not substitutes."}));
            }
            let lines: Vec<_> = content.lines().collect();
            let matching = if terms.is_empty() {
                0
            } else {
                lines
                    .iter()
                    .position(|line| {
                        terms
                            .iter()
                            .any(|term| domain::casefold(line).contains(term))
                    })
                    .unwrap_or(0)
            };
            let begin = matching.saturating_sub(2);
            let mut excerpt = String::new();
            let mut stop = begin;
            let cap = remaining.min(1_400);
            for line in lines.iter().skip(begin).take(16) {
                let needed = line.chars().count() + usize::from(!excerpt.is_empty());
                if excerpt.chars().count() + needed > cap {
                    if excerpt.is_empty() {
                        excerpt = line.chars().take(cap).collect();
                        stop += 1;
                    }
                    break;
                }
                if !excerpt.is_empty() {
                    excerpt.push('\n');
                }
                excerpt.push_str(line);
                stop += 1;
            }
            let partial = begin > 0
                || stop < lines.len()
                || excerpt.chars().count()
                    < lines
                        .get(begin..stop)
                        .unwrap_or_default()
                        .join("\n")
                        .chars()
                        .count();
            remaining = remaining.saturating_sub(excerpt.chars().count());
            sources.push(json!({"path":path,"sha256":hash,"start_line":if lines.is_empty(){0}else{begin+1},"end_line":stop,"excerpt":excerpt,"partial":partial,"stale":stale,"provenance":provenance}));
        }
        if selected_candidates > sources.len() {
            gaps.push(json!({"code":"bounded_or_filtered","path":null,"message":"The pack is bounded or filtered; it does not claim exhaustive workspace coverage."}));
        }
        if sources.is_empty() {
            gaps.push(json!({"code":"no_sources","path":null,"message":"No readable matching evidence was selected. Supply exact paths or a narrower query."}));
        }
        let read_refs:Vec<_>=sources.iter().map(|source|json!({"tool":"read_record","arguments":{"path":source["path"]},"sha256":source["sha256"]})).collect();
        let pack_identity = json!({"schema_version":1,"task":task,"query":query,"paths":args.get("paths").cloned().unwrap_or(Value::Null),"company":args.get("company").cloned().unwrap_or(Value::Null),"project":args.get("project").cloned().unwrap_or(Value::Null),"period":requested.cloned().unwrap_or(Value::Null),"sources":sources});
        let pack_sha256 = domain::digest(&domain::encoded(&pack_identity)?);
        Ok(
            json!({"pack_sha256":pack_sha256,"task":task,"query":query,"sources":sources,"gaps":gaps,"candidate_count":selected_candidates,"selected_count":sources.len(),"complete":false,"read_refs":read_refs,"period":requested.cloned().unwrap_or(Value::Null),"authority":"sourced_context_only"}),
        )
    }
}
