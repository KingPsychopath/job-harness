//! Optional, disposable passage/vector cache. Similarity is retrieval, never evidence or permission.
use crate::{HarnessError, Result, index, storage};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelIdentity {
    pub model: String,
    pub revision: String,
    pub provider: String,
    pub document_prefix: String,
    pub query_prefix: String,
    pub chunk_lines: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Passage {
    pub path: String,
    pub source_sha256: String,
    pub kind: String,
    pub line_start: usize,
    pub line_end: usize,
    pub text: String,
}
fn fault(_: rusqlite::Error) -> HarnessError {
    HarnessError::new(
        "semantic_index_error",
        "Optional semantic cache unavailable; exact search and canonical records remain usable.",
    )
}
fn open(root: &Path) -> Result<Connection> {
    let root = root.canonicalize()?;
    let path = root.join(".harness/semantic.sqlite");
    storage::reject_symlink_ancestors(&path)?;
    fs::create_dir_all(root.join(".harness"))?;
    let db = Connection::open(&path).map_err(fault)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join(".harness"), fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    db.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(fault)?;
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS config(id INTEGER PRIMARY KEY CHECK(id=1), identity TEXT NOT NULL, dimensions INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS passages(id INTEGER PRIMARY KEY,path TEXT NOT NULL,sha TEXT NOT NULL,kind TEXT NOT NULL,line_start INTEGER NOT NULL,line_end INTEGER NOT NULL,text TEXT NOT NULL,vector TEXT NOT NULL); CREATE INDEX IF NOT EXISTS passage_source ON passages(path);").map_err(fault)?;
    Ok(db)
}
/// Explicitly selected sources only. No whole-workspace upload or automatic model download.
pub fn passages(root: &Path, paths: &[String], chunk_lines: usize) -> Result<Vec<Passage>> {
    if paths.is_empty() || paths.len() > 100 || !(1..=100).contains(&chunk_lines) {
        return Err(HarnessError::validation(
            "Select 1–100 text sources and 1–100 lines per passage",
        ));
    }
    let root = root.canonicalize()?;
    let mut output = Vec::new();
    let mut total = 0usize;
    for relative in paths {
        let path = storage::safe_path(
            &root,
            relative,
            &["context", "applications", "content"],
            &[".md", ".json"],
        )?;
        let metadata = fs::metadata(&path)?;
        if metadata.len() > 1_000_000 {
            return Err(HarnessError::validation("Selected source exceeds one MB"));
        }
        let bytes = fs::read(path)?;
        total = total.saturating_add(bytes.len());
        if total > 16 * 1024 * 1024 {
            return Err(HarnessError::validation(
                "Selected text exceeds sixteen MiB",
            ));
        }
        let sha = storage::digest(&bytes);
        let text = String::from_utf8(bytes)
            .map_err(|_| HarnessError::validation("Select UTF-8 text sources"))?;
        let lines = text.lines().collect::<Vec<_>>();
        for (i, chunk) in lines.chunks(chunk_lines).enumerate() {
            let text = chunk.join("\n");
            if text.trim().is_empty() {
                continue;
            }
            if text.len() > 16 * 1024 {
                return Err(HarnessError::validation(
                    "A passage exceeds sixteen KiB; shorten source lines",
                ));
            }
            output.push(Passage {
                path: relative.clone(),
                source_sha256: sha.clone(),
                kind: index::source_kind(&root, relative),
                line_start: i * chunk_lines + 1,
                line_end: i * chunk_lines + chunk.len(),
                text,
            });
            if output.len() > 5000 {
                return Err(HarnessError::validation(
                    "Selected sources exceed five thousand passages",
                ));
            }
        }
    }
    Ok(output)
}
fn vector_ok(v: &[f32], dimensions: usize) -> bool {
    v.len() == dimensions
        && (1..=4096).contains(&dimensions)
        && v.iter().all(|x| x.is_finite())
        && v.iter().any(|x| *x != 0.0)
}
pub fn replace(
    root: &Path,
    identity: &ModelIdentity,
    passages: &[Passage],
    vectors: &[Vec<f32>],
) -> Result<Value> {
    if identity.model.trim().is_empty()
        || identity.revision.trim().is_empty()
        || identity.provider.trim().is_empty()
        || passages.is_empty()
        || passages.len() > 5000
        || passages.len() != vectors.len()
    {
        return Err(HarnessError::validation(
            "Bind a model revision and one finite vector to each selected passage",
        ));
    }
    let dimensions = vectors[0].len();
    if vectors.iter().any(|v| !vector_ok(v, dimensions)) {
        return Err(HarnessError::validation(
            "Embedding dimensions must match and every vector must be finite and nonzero",
        ));
    }
    // Recheck each source after remote computation, before publishing any cache snapshot.
    for passage in passages {
        let path = storage::safe_path(
            root,
            &passage.path,
            &["context", "applications", "content"],
            &[".md", ".json"],
        )?;
        let bytes = fs::read(path)?;
        if storage::digest(&bytes) != passage.source_sha256 {
            return Err(HarnessError::new(
                "source_changed",
                "Selected text changed while embeddings were generated; rebuild after reviewing the source.",
            ));
        }
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| HarnessError::validation("Selected passage source must be UTF-8"))?;
        let lines = source.lines().collect::<Vec<_>>();
        if passage.line_start == 0
            || passage.line_end < passage.line_start
            || passage.line_end > lines.len()
            || lines[passage.line_start - 1..passage.line_end].join("\n") != passage.text
        {
            return Err(HarnessError::new(
                "invalid_passage",
                "Passages must quote their exact source line range",
            ));
        }
    }
    let mut db = open(root)?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(fault)?;
    tx.execute("DELETE FROM passages", []).map_err(fault)?;
    tx.execute("INSERT INTO config VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET identity=excluded.identity,dimensions=excluded.dimensions",params![serde_json::to_string(identity)?,dimensions as u32]).map_err(fault)?;
    for (p, v) in passages.iter().zip(vectors) {
        tx.execute("INSERT INTO passages(path,sha,kind,line_start,line_end,text,vector) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![p.path,p.source_sha256,p.kind,p.line_start as u32,p.line_end as u32,p.text,serde_json::to_string(v)?]).map_err(fault)?;
    }
    tx.commit().map_err(fault)?;
    // Removed sources must not survive in reusable SQLite pages.
    db.execute_batch("VACUUM").map_err(fault)?;
    Ok(
        json!({"authority":"derived","model":identity,"dimensions":dimensions,"passages":passages.len(),"database":".harness/semantic.sqlite","canonical_changes":false}),
    )
}
/// Discard changed/deleted source text without asking a model to run again.
fn purge_stale(root: &Path, db: &mut Connection) -> Result<usize> {
    let records = {
        let mut statement = db
            .prepare("SELECT DISTINCT path,sha FROM passages")
            .map_err(fault)?;
        statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(fault)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(fault)?
    };
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(fault)?;
    let mut removed = 0;
    for (path, sha) in records {
        let current = storage::safe_path(
            root,
            &path,
            &["context", "applications", "content"],
            &[".md", ".json"],
        )
        .and_then(|p| Ok(storage::digest(&fs::read(p)?)))
        .ok();
        if current.as_ref() != Some(&sha) {
            removed += tx
                .execute(
                    "DELETE FROM passages WHERE path=?1 AND sha=?2",
                    params![path, sha],
                )
                .map_err(fault)?;
        } else {
            tx.execute(
                "UPDATE passages SET kind=?1 WHERE path=?2",
                params![index::source_kind(root, &path), path],
            )
            .map_err(fault)?;
        }
    }
    tx.commit().map_err(fault)?;
    if removed > 0 {
        db.execute_batch("VACUUM").map_err(fault)?;
    }
    Ok(removed)
}
pub fn inspect(root: &Path) -> Result<Value> {
    if !root.join(".harness/semantic.sqlite").exists() {
        return Ok(json!({"enabled":false,"required":false}));
    }
    let mut db = open(root)?;
    let purged = purge_stale(root, &mut db)?;
    let identity: Option<(String, u32)> = db
        .query_row(
            "SELECT identity,dimensions FROM config WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(fault)?;
    match identity {
        Some((model, dimensions)) => {
            let count: i64 = db
                .query_row("SELECT count(*) FROM passages", [], |r| r.get(0))
                .map_err(fault)?;
            Ok(
                json!({"enabled":true,"required":false,"model":serde_json::from_str::<Value>(&model)?,"dimensions":dimensions,"passages":count,"stale_passages_purged":purged}),
            )
        }
        None => Ok(json!({"enabled":false,"required":false})),
    }
}
use rusqlite::OptionalExtension;
pub fn search(
    root: &Path,
    identity: &ModelIdentity,
    query: &str,
    vector: &[f32],
    limit: usize,
) -> Result<Value> {
    if limit == 0 || limit > 100 || query.len() > 1000 {
        return Err(HarnessError::validation(
            "Use a bounded query and 1–100 results",
        ));
    }
    if !root.join(".harness/semantic.sqlite").exists() {
        return Err(HarnessError::new(
            "missing_semantic_index",
            "Exact search is available; explicitly index selected sources to enable semantic retrieval.",
        ));
    }
    let mut db = open(root)?;
    let purged = purge_stale(root, &mut db)?;
    let (stored, dimensions): (String, u32) = db
        .query_row(
            "SELECT identity,dimensions FROM config WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(fault)?;
    if serde_json::from_str::<ModelIdentity>(&stored)? != *identity
        || !vector_ok(vector, dimensions as usize)
    {
        return Err(HarnessError::new(
            "model_mismatch",
            "Model revision, prefixes, chunking and dimensions must match the indexed configuration; rebuild explicitly.",
        ));
    }
    let lexical = index::search(root, query, 100)?;
    let lexical_rank: BTreeMap<_, _> = lexical["matches"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r["path"].as_str().map(|p| (p.to_owned(), i)))
        .collect();
    let mut stmt = db
        .prepare("SELECT path,sha,kind,line_start,line_end,text,vector FROM passages")
        .map_err(fault)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, u32>(3)?,
                r.get::<_, u32>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(fault)?;
    let mut scored = Vec::new();
    let mut stale = 0;
    let norm = (vector.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>()).sqrt();
    let mut current_hashes = BTreeMap::new();
    for row in rows {
        let (path, sha, kind, start, end, cached_text, encoded) = row.map_err(fault)?;
        if kind.starts_with("archived:") {
            continue;
        }
        let current = current_hashes.entry(path.clone()).or_insert_with(|| {
            storage::safe_path(
                root,
                &path,
                &["context", "applications", "content"],
                &[".md", ".json"],
            )
            .and_then(|p| Ok(storage::digest(&fs::read(p)?)))
            .ok()
        });
        if current.as_ref() != Some(&sha) {
            stale += 1;
            continue;
        }
        let bytes = fs::read(storage::safe_path(
            root,
            &path,
            &["context", "applications", "content"],
            &[".md", ".json"],
        )?)?;
        if storage::digest(&bytes) != sha {
            stale += 1;
            continue;
        }
        let original = std::str::from_utf8(&bytes)
            .map_err(|_| HarnessError::validation("Cached source must remain UTF-8"))?;
        let lines = original.lines().collect::<Vec<_>>();
        if start == 0 || end < start || end as usize > lines.len() {
            return Err(HarnessError::new(
                "semantic_index_error",
                "Cached line anchors are invalid; rebuild the derived cache",
            ));
        }
        let text = lines[start as usize - 1..end as usize].join("\n");
        if text != cached_text {
            return Err(HarnessError::new(
                "semantic_index_error",
                "Cached passage differs from its source; rebuild the derived cache",
            ));
        }
        let v: Vec<f32> = serde_json::from_str(&encoded)?;
        if !vector_ok(&v, dimensions as usize) {
            return Err(HarnessError::new(
                "semantic_index_error",
                "Embedding cache is damaged; rebuild without changing canonical records.",
            ));
        }
        let denominator = norm * (v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>()).sqrt();
        let cosine = vector
            .iter()
            .zip(&v)
            .map(|(a, b)| f64::from(*a) * f64::from(*b))
            .sum::<f64>()
            / denominator;
        scored.push((cosine,json!({"path":path,"source_sha256":sha,"kind":kind,"line_start":start,"line_end":end,"text":text,"similarity":cosine})));
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut hybrid = scored
        .into_iter()
        .enumerate()
        .map(|(rank, (_, mut row))| {
            let score = 1.0 / (60 + rank + 1) as f64
                + lexical_rank
                    .get(row["path"].as_str().unwrap())
                    .map_or(0.0, |r| 1.0 / (60 + r + 1) as f64);
            row["retrieval_score"] = json!(score);
            (score, row)
        })
        .collect::<Vec<_>>();
    hybrid.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(
        json!({"query":query,"model":identity,"authority":"derived","matches":hybrid.into_iter().take(limit).map(|(_,r)|r).collect::<Vec<_>>(),"stale_passages_excluded":stale,"stale_passages_purged":purged,"ranking":"reciprocal-rank lexical + cosine","next":"Similarity finds passages. Read and assess the original; it does not establish a qualification or authorize an action."}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("context/projects")).unwrap();
        fs::write(
            t.path().join("context/projects/a.md"),
            "# Ownership\nBuilt a queue\n",
        )
        .unwrap();
        t
    }
    fn model() -> ModelIdentity {
        ModelIdentity {
            model: "fixture".into(),
            revision: "1".into(),
            provider: "local-fixture".into(),
            document_prefix: "".into(),
            query_prefix: "".into(),
            chunk_lines: 20,
        }
    }
    #[test]
    fn semantic_cache_requires_optin_provenance_and_fresh_sources() {
        let t = setup();
        let m = model();
        let p = passages(t.path(), &["context/projects/a.md".into()], 20).unwrap();
        replace(t.path(), &m, &p, &[vec![1.0, 0.0]]).unwrap();
        let result = search(t.path(), &m, "autonomy", &[1.0, 0.0], 5).unwrap();
        assert_eq!(result["matches"][0]["line_start"], 1);
        fs::write(t.path().join("context/projects/a.md"), "changed").unwrap();
        assert!(
            search(t.path(), &m, "autonomy", &[1.0, 0.0], 5).unwrap()["matches"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(replace(t.path(), &m, &p, &[vec![1.0, 0.0]]).is_err());
    }
    #[test]
    fn vectors_never_mix_revisions_dimensions_or_nonfinite_values() {
        let t = setup();
        let mut m = model();
        let p = passages(t.path(), &["context/projects/a.md".into()], 20).unwrap();
        assert!(replace(t.path(), &m, &p, &[vec![f32::NAN]]).is_err());
        replace(t.path(), &m, &p, &[vec![1.0, 0.0]]).unwrap();
        m.revision = "2".into();
        assert!(search(t.path(), &m, "queue", &[1.0, 0.0], 2).is_err());
        assert!(search(t.path(), &model(), "queue", &[1.0], 2).is_err());
        assert!(passages(t.path(), &["../secret.md".into()], 20).is_err());
    }
    #[test]
    fn removed_plaintext_is_purged_and_cache_tampering_is_not_a_source() {
        let t = setup();
        let p = passages(t.path(), &["context/projects/a.md".into()], 20).unwrap();
        replace(t.path(), &model(), &p, &[vec![1.0, 0.0]]).unwrap();
        let db = open(t.path()).unwrap();
        db.execute("UPDATE passages SET text='invented claim'", [])
            .unwrap();
        drop(db);
        assert!(search(t.path(), &model(), "queue", &[1.0, 0.0], 2).is_err());
        replace(t.path(), &model(), &p, &[vec![1.0, 0.0]]).unwrap();
        fs::remove_file(t.path().join("context/projects/a.md")).unwrap();
        let view = inspect(t.path()).unwrap();
        assert_eq!(view["stale_passages_purged"], 1);
        assert_eq!(view["passages"], 0);
        let data = fs::read(t.path().join(".harness/semantic.sqlite")).unwrap();
        assert!(
            !data
                .windows(b"Built a queue".len())
                .any(|s| s == b"Built a queue")
        );
    }
}
