//! Rebuildable relational search. Canonical files and the submission journal remain authoritative.
use crate::{HarnessError, Result, pipeline, storage};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
use walkdir::WalkDir;

const MAX_DOCUMENT: u64 = 2_000_000;
const MAX_DOCUMENTS: usize = 20_000;
fn database_error(_: rusqlite::Error) -> HarnessError {
    HarnessError::new(
        "index_error",
        "Search index unavailable; canonical records are unchanged. Rebuild the index.",
    )
}
fn open(root: &Path) -> Result<Connection> {
    storage::reject_symlink_ancestors(&root.join(".harness/index.sqlite"))?;
    let directory = root.join(".harness");
    fs::create_dir_all(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    }
    let db = Connection::open(directory.join("index.sqlite")).map_err(database_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            directory.join("index.sqlite"),
            fs::Permissions::from_mode(0o600),
        )?;
    }
    db.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(database_error)?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA secure_delete=ON; PRAGMA foreign_keys=ON;
      CREATE TABLE IF NOT EXISTS documents(path TEXT PRIMARY KEY, kind TEXT NOT NULL, sha256 TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS vacancies(path TEXT PRIMARY KEY REFERENCES documents(path) ON DELETE CASCADE, stage TEXT NOT NULL, data TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS links(source TEXT NOT NULL REFERENCES documents(path) ON DELETE CASCADE, target TEXT NOT NULL, relation TEXT NOT NULL, PRIMARY KEY(source,target,relation));
      CREATE INDEX IF NOT EXISTS links_target ON links(target);
      CREATE INDEX IF NOT EXISTS vacancy_stage ON vacancies(stage);
      CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5(path UNINDEXED,title,body,tokenize='unicode61');").map_err(database_error)?;
    Ok(db)
}
fn kind(path: &str) -> &'static str {
    if path.starts_with("context/interviews/") {
        "interview-note"
    } else if path.starts_with("applications/briefs/") || path.starts_with("docs/") {
        "coaching"
    } else if path.contains("approved-answers") || path.contains("application-decisions") {
        "attributed-answers"
    } else if path.starts_with("context/") {
        "evidence"
    } else if path.starts_with("content/") {
        "cv-selection"
    } else if path.starts_with("config/") {
        "configuration"
    } else {
        "application-record"
    }
}
/// Intake provenance stays distinguishable from career facts, even after review.
pub fn source_kind(root: &Path, path: &str) -> String {
    if let Some(tail) = path.strip_prefix("context/intake/") {
        let id = tail.split('/').next().unwrap_or_default();
        let metadata_path = format!("context/intake/{id}/metadata.json");
        let metadata = storage::safe_path(root, &metadata_path, &["context"], &[".json"])
            .ok()
            .and_then(|p| fs::read(p).ok())
            .filter(|b| b.len() < 1_000_000)
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        let original_kind = metadata
            .as_ref()
            .and_then(|v| v["source_kind"].as_str())
            .filter(|s| {
                [
                    "personal_notes",
                    "transcript",
                    "provider_summary",
                    "generated_summary",
                ]
                .contains(s)
            })
            .unwrap_or("unknown-source");
        let category = if path.contains("/proposals/") {
            "generated-proposal".to_owned()
        } else if path.contains("/reviews/") {
            format!("reviewed:{original_kind}")
        } else if path.ends_with("/original.md") {
            original_kind.to_owned()
        } else {
            "intake-metadata".into()
        };
        if metadata.as_ref().is_some_and(|v| v["archived"] == true) {
            format!("archived:{category}")
        } else {
            category
        }
    } else {
        kind(path).into()
    }
}
fn canonical_paths(root: &Path) -> Result<(Vec<String>, Vec<Value>)> {
    let mut paths = Vec::new();
    let mut skipped = Vec::new();
    for directory in ["context", "content", "applications", "docs", "config"] {
        let base = root.join(directory);
        if !base.exists() {
            continue;
        }
        for entry in WalkDir::new(base).follow_links(false) {
            let entry = entry.map_err(|_| {
                HarnessError::new("index_error", "Could not enumerate canonical records")
            })?;
            if !entry.file_type().is_file()
                || !matches!(
                    entry.path().extension().and_then(|x| x.to_str()),
                    Some("md" | "json")
                )
            {
                continue;
            }
            let rel = match storage::portable_relative(root, entry.path()) {
                Ok(path) => path,
                Err(_) => {
                    // Preserve the observed filename in the coverage report instead
                    // of rewriting a literal backslash into a different source.
                    let observed = entry
                        .path()
                        .strip_prefix(root)
                        .map_err(|_| HarnessError::validation("Invalid index path"))?
                        .to_string_lossy();
                    skipped.push(json!({"path":observed,"reason":"nonportable source path"}));
                    if skipped.len() + paths.len() > MAX_DOCUMENTS {
                        return Err(HarnessError::validation(
                            "Index exceeds twenty thousand source documents",
                        ));
                    }
                    continue;
                }
            };
            if rel.starts_with("applications/operations/")
                || matches!(
                    rel.as_str(),
                    "applications/pipeline.md" | "applications/pipeline.json"
                )
            {
                continue;
            }
            paths.push(rel);
            if paths.len() + skipped.len() > MAX_DOCUMENTS {
                return Err(HarnessError::validation(
                    "Index exceeds twenty thousand source documents",
                ));
            }
        }
    }
    paths.sort();
    skipped.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok((paths, skipped))
}
fn relative_link(source: &str, target: &str) -> Option<String> {
    let target = target.split('#').next()?.trim_matches(['<', '>']);
    if target.is_empty()
        || target.contains("://")
        || target.starts_with('/')
        || target.contains('\\')
    {
        return None;
    }
    let mut parts = source.split('/').collect::<Vec<_>>();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            x => parts.push(x),
        }
    }
    Some(parts.join("/"))
}
/// Refresh only changed document rows in one database transaction. Never edits source files.
/// Callers coordinating mutations already hold the workspace lock.
pub fn refresh(root: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let (paths, mut skipped) = canonical_paths(&root)?;
    let mut db = open(&root)?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(database_error)?;
    let old = {
        let mut stmt = tx
            .prepare("SELECT path,sha256,kind FROM documents")
            .map_err(database_error)?;
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (row.get::<_, String>(1)?, row.get::<_, String>(2)?),
            ))
        })
        .map_err(database_error)?
        .collect::<std::result::Result<std::collections::BTreeMap<_, _>, _>>()
        .map_err(database_error)?
    };
    let mut changed = 0;
    let mut seen = BTreeSet::new();
    let mut fingerprints = Vec::new();
    let link_rx = regex::Regex::new(r"\]\(([^)]+)\)").expect("static link pattern");
    for path in &paths {
        let full = storage::safe_path(
            &root,
            path,
            &["context", "content", "applications", "docs", "config"],
            &[".md", ".json"],
        )?;
        use std::io::Read;
        let bounded_read = || -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            fs::File::open(full)?
                .take(MAX_DOCUMENT + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        };
        let bytes = match bounded_read() {
            Ok(bytes) => bytes,
            Err(_) => {
                skipped.push(json!({"path":path,"reason":"unreadable source"}));
                continue;
            }
        };
        if bytes.len() > MAX_DOCUMENT as usize {
            skipped.push(json!({"path":path,"reason":"source exceeds two MB text indexing limit"}));
            continue;
        }
        let hash = storage::digest(&bytes);
        if std::str::from_utf8(&bytes).is_err() {
            skipped.push(json!({"path":path,"reason":"not UTF-8 text"}));
            continue;
        }
        seen.insert(path.clone());
        let classification = source_kind(&root, path);
        fingerprints.push(format!("{path}\0{hash}\0{classification}\n"));
        if old.get(path) == Some(&(hash.clone(), classification.clone())) {
            continue;
        }
        let body = String::from_utf8(bytes)
            .map_err(|_| HarnessError::new("index_error", "Source document is not UTF-8"))?;
        let title = body
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .unwrap_or(path)
            .to_owned();
        tx.execute("INSERT INTO documents VALUES (?1,?2,?3,?4,?5) ON CONFLICT(path) DO UPDATE SET kind=excluded.kind,sha256=excluded.sha256,title=excluded.title,body=excluded.body",params![path,classification,hash,title,body]).map_err(database_error)?;
        tx.execute("DELETE FROM search WHERE path=?1", [path])
            .map_err(database_error)?;
        tx.execute(
            "INSERT INTO search(path,title,body) VALUES(?1,?2,?3)",
            params![path, title, body],
        )
        .map_err(database_error)?;
        tx.execute("DELETE FROM links WHERE source=?1", [path])
            .map_err(database_error)?;
        for captures in link_rx.captures_iter(&body) {
            if let Some(target) = relative_link(path, &captures[1]) {
                tx.execute(
                    "INSERT OR IGNORE INTO links VALUES(?1,?2,'cites')",
                    params![path, target],
                )
                .map_err(database_error)?;
            }
        }
        // Typed intake relationships join notes to projects/applications without guessing identity.
        if path.starts_with("context/intake/")
            && path.ends_with("/metadata.json")
            && let Ok(metadata) = serde_json::from_str::<Value>(&body)
        {
            let mut targets = metadata["links"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|link| link["path"].as_str())
                .map(|p| (p, "linked-to"))
                .collect::<Vec<_>>();
            if let Some(original) = metadata["original"]["path"].as_str() {
                targets.push((original, "original"));
            }
            for (target, relation) in targets {
                if seen.contains(target)
                    || storage::safe_path(
                        &root,
                        target,
                        &["context", "applications"],
                        &[".md", ".json"],
                    )
                    .is_ok_and(|p| p.is_file())
                {
                    tx.execute(
                        "INSERT OR IGNORE INTO links VALUES(?1,?2,?3)",
                        params![path, target, relation],
                    )
                    .map_err(database_error)?;
                }
            }
        }
        changed += 1;
    }
    let mut removed = 0;
    for path in old.keys().filter(|p| !seen.contains(*p)) {
        tx.execute("DELETE FROM documents WHERE path=?1", [path])
            .map_err(database_error)?;
        tx.execute("DELETE FROM search WHERE path=?1", [path])
            .map_err(database_error)?;
        removed += 1;
    }
    // Parse tracking only if any canonical source changed; stable views reuse materialized joins.
    if changed > 0 || removed > 0 || old.is_empty() {
        let view = pipeline::build(&root);
        if view.is_err() {
            skipped.push(json!({"path":"applications/","reason":"tracking projection unavailable; inspect canonical records"}));
        }
        tx.execute("DELETE FROM vacancies", [])
            .map_err(database_error)?;
        for row in view.map(|p| p.records).unwrap_or_default() {
            let path = format!(
                "applications/{}",
                row["record"].as_str().unwrap_or_default()
            );
            if !seen.contains(&path) {
                continue;
            }
            tx.execute(
                "INSERT INTO vacancies VALUES(?1,?2,?3)",
                params![
                    path,
                    row["stage"].as_str().unwrap_or("needs-review"),
                    serde_json::to_string(&row)?
                ],
            )
            .map_err(database_error)?;
            for field in ["brief", "record_evidence"] {
                if let Some(link) = row[field].as_str().filter(|s| !s.is_empty()) {
                    let Some(target) = relative_link(&path, link) else {
                        skipped.push(json!({"path":path,"reason":"invalid relationship path"}));
                        continue;
                    };
                    tx.execute(
                        "INSERT OR IGNORE INTO links VALUES(?1,?2,?3)",
                        params![path, target, field],
                    )
                    .map_err(database_error)?;
                }
            }
        }
    }
    let generation = storage::digest(fingerprints.join("").as_bytes());
    if changed > 0 || removed > 0 || old.is_empty() {
        tx.execute("INSERT INTO metadata VALUES('generation',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[&generation]).map_err(database_error)?;
    }
    tx.commit().map_err(database_error)?;
    if removed > 0 {
        db.execute_batch("INSERT INTO search(search) VALUES('optimize'); VACUUM;")
            .map_err(database_error)?;
    }
    Ok(
        json!({"schema_version":1,"documents":seen.len(),"changed":changed,"removed":removed,"skipped":skipped,"authority":"derived","index_generation":generation,"stale":false,"database":".harness/index.sqlite"}),
    )
}
pub fn overview(root: &Path) -> Result<Value> {
    let refreshed = refresh(root)?;
    let db = open(root)?;
    let documents: i64 = db
        .query_row("SELECT count(*) FROM documents", [], |r| r.get(0))
        .map_err(database_error)?;
    let links: i64 = db
        .query_row("SELECT count(*) FROM links", [], |r| r.get(0))
        .map_err(database_error)?;
    let mut statement = db
        .prepare("SELECT stage,count(*) FROM vacancies GROUP BY stage ORDER BY stage")
        .map_err(database_error)?;
    let stages = statement
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, json!(r.get::<_, i64>(1)?)))
        })
        .map_err(database_error)?
        .collect::<std::result::Result<serde_json::Map<_, _>, _>>()
        .map_err(database_error)?;
    Ok(
        json!({"schema_version":1,"authority":"derived","index_generation":refreshed["index_generation"],"stale":false,"documents":documents,"relationships":links,"stages":stages,"skipped":refreshed["skipped"],"coverage_complete":refreshed["skipped"].as_array().is_some_and(Vec::is_empty)}),
    )
}
pub fn search(root: &Path, query: &str, limit: usize) -> Result<Value> {
    if query.len() > 500 || limit == 0 || limit > 100 {
        return Err(HarnessError::validation(
            "Use a query up to 500 bytes and a limit from 1 to 100",
        ));
    }
    let refreshed = refresh(root)?;
    let db = open(root)?;
    let terms = query
        .split_whitespace()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    if terms.is_empty() {
        return Ok(
            json!({"schema_version":1,"query":query,"matches":[],"authority":"derived","index_generation":refreshed["index_generation"],"stale":false,"skipped":refreshed["skipped"],"coverage_complete":refreshed["skipped"].as_array().is_some_and(Vec::is_empty)}),
        );
    }
    let mut stmt=db.prepare("SELECT d.path,d.kind,d.sha256,d.title,snippet(search,2,'','', ' … ',24) FROM search JOIN documents d ON d.path=search.path WHERE search MATCH ?1 AND d.kind NOT LIKE 'archived:%' ORDER BY rank LIMIT ?2").map_err(database_error)?;
    let matches=stmt.query_map(params![terms,limit as i64],|r|Ok(json!({"path":r.get::<_,String>(0)?,"kind":r.get::<_,String>(1)?,"sha256":r.get::<_,String>(2)?,"title":r.get::<_,String>(3)?,"excerpt":r.get::<_,String>(4)?}))).map_err(database_error)?.collect::<std::result::Result<Vec<_>,_>>().map_err(database_error)?;
    Ok(
        json!({"schema_version":1,"query":query,"matches":matches,"authority":"derived","index_generation":refreshed["index_generation"],"stale":false,"skipped":refreshed["skipped"],"coverage_complete":refreshed["skipped"].as_array().is_some_and(Vec::is_empty),"next":"Read source records and verify attribution before using any claim."}),
    )
}

/// Bounded two-hop materialized relationships. Canonical files are still verified by callers.
pub fn related_paths(root: &Path, seeds: &[String], limit: usize) -> Result<Vec<String>> {
    if seeds.len() > 100 || limit == 0 || limit > 100 {
        return Err(HarnessError::validation(
            "Bound relationship traversal to one hundred sources/results",
        ));
    }
    refresh(root)?;
    let db = open(root)?;
    let mut paths = BTreeSet::new();
    let mut statement=db.prepare("SELECT target FROM links WHERE source=?1 UNION SELECT source FROM links WHERE target=?1").map_err(database_error)?;
    let mut frontier = seeds.to_vec();
    for _ in 0..2 {
        let mut next = Vec::new();
        for seed in frontier {
            for row in statement
                .query_map([&seed], |r| r.get::<_, String>(0))
                .map_err(database_error)?
            {
                let path = row.map_err(database_error)?;
                if !seeds.contains(&path) && paths.insert(path.clone()) {
                    next.push(path);
                    if paths.len() >= limit {
                        return Ok(paths.into_iter().collect());
                    }
                }
            }
        }
        frontier = next;
    }
    Ok(paths.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn nonportable_filename_is_reported_without_becoming_another_source() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("context/fictional")).unwrap();
        let nonportable = root.path().join("context").join("fictional\\notes.md");
        fs::write(&nonportable, "# Nonportable\nwrongsourceuniquemarker\n").unwrap();
        fs::write(
            root.path().join("context/fictional/notes.md"),
            "# Real nested source\nrightsourceuniquemarker\n",
        )
        .unwrap();
        let result = search(root.path(), "rightsourceuniquemarker", 5).unwrap();
        assert_eq!(result["coverage_complete"], false);
        assert_eq!(
            result["skipped"][0],
            json!({"path":r"context/fictional\notes.md","reason":"nonportable source path"})
        );
        assert_eq!(result["matches"][0]["path"], "context/fictional/notes.md");
        assert!(
            search(root.path(), "wrongsourceuniquemarker", 5).unwrap()["matches"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fs::read_to_string(nonportable).unwrap(),
            "# Nonportable\nwrongsourceuniquemarker\n"
        );
    }
    #[test]
    fn index_is_incremental_rebuildable_and_separates_coaching() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("context")).unwrap();
        fs::create_dir_all(t.path().join("applications/briefs")).unwrap();
        fs::write(
            t.path().join("context/facts.md"),
            "# Evidence\nDatabase storage [brief](../applications/briefs/demo.md)\n",
        )
        .unwrap();
        fs::write(
            t.path().join("applications/briefs/demo.md"),
            "# Practice\nHypothetical database question\n",
        )
        .unwrap();
        assert_eq!(refresh(t.path()).unwrap()["changed"], 2);
        assert_eq!(refresh(t.path()).unwrap()["changed"], 0);
        let rows = search(t.path(), "database", 10).unwrap();
        assert_eq!(rows["matches"].as_array().unwrap().len(), 2);
        assert!(
            rows["matches"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["kind"] == "coaching")
        );
        fs::remove_file(t.path().join("context/facts.md")).unwrap();
        assert_eq!(refresh(t.path()).unwrap()["removed"], 1);
        fs::remove_file(t.path().join(".harness/index.sqlite")).unwrap();
        assert_eq!(refresh(t.path()).unwrap()["documents"], 1);
    }
    #[test]
    fn query_syntax_cannot_inject_sql_or_fts() {
        let t = tempfile::tempdir().unwrap();
        assert!(search(t.path(), "\" OR drop table", 3).is_ok());
        assert!(search(t.path(), "x", 101).is_err());
    }
    #[test]
    fn partial_bad_source_does_not_hide_good_evidence_and_removed_text_is_erased() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("context")).unwrap();
        fs::write(
            t.path().join("context/good.md"),
            "# Good\nuniquedeletionmarker\n",
        )
        .unwrap();
        fs::write(t.path().join("context/bad.md"), [0xff, 0xfe]).unwrap();
        let result = search(t.path(), "uniquedeletionmarker", 5).unwrap();
        assert_eq!(result["matches"].as_array().unwrap().len(), 1);
        assert_eq!(result["coverage_complete"], false);
        assert_eq!(result["skipped"][0]["path"], "context/bad.md");
        fs::remove_file(t.path().join("context/good.md")).unwrap();
        refresh(t.path()).unwrap();
        let bytes = fs::read(t.path().join(".harness/index.sqlite")).unwrap();
        assert!(
            !bytes
                .windows(b"uniquedeletionmarker".len())
                .any(|s| s == b"uniquedeletionmarker")
        );
    }
    #[test]
    fn simultaneous_rebuilders_commit_one_consistent_projection() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("context")).unwrap();
        fs::write(t.path().join("context/one.md"), "# One\nconcurrent\n").unwrap();
        // Create the database once; both refreshers then contend for an IMMEDIATE transaction.
        refresh(t.path()).unwrap();
        std::thread::scope(|scope| {
            let first = scope.spawn(|| refresh(t.path()).unwrap());
            let second = scope.spawn(|| refresh(t.path()).unwrap());
            assert_eq!(first.join().unwrap()["documents"], 1);
            assert_eq!(second.join().unwrap()["documents"], 1);
        });
        assert_eq!(
            search(t.path(), "concurrent", 5).unwrap()["matches"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
