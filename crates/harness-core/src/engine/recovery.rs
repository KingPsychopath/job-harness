//! Explicit, evidence-backed journal conflict recovery with preserved losing bytes.
use super::*;

impl Engine {
    pub fn pending_transaction(&self) -> Result<Value> {
        let _guard = storage::WorkspaceLock::acquire(&self.root, false)?;
        let state = self.load_state()?;
        let pending = state.get("pending").cloned().unwrap_or(Value::Null);
        let hash = domain::digest(&domain::encoded(&pending)?);
        let mut paths = vec![];
        if let Some(changes) = pending.as_array() {
            for change in changes {
                let rel = s(change, "path")?;
                let path = self.journal_path(rel)?;
                let current = if path.is_file() {
                    Some(domain::digest(&fs::read(path)?))
                } else {
                    None
                };
                paths.push(json!({"path":rel,"before_sha256":change["before_sha256"],"current_sha256":current,"journal_sha256":domain::digest(s(change,"text")?.as_bytes())}));
            }
        }
        Ok(
            json!({"pending":!pending.is_null(),"pending_sha256":hash,"paths":paths,"instruction":"Choose current or journal deliberately. A resolution preserves losing bytes and never repeats an external action."}),
        )
    }
    pub fn resolve_pending(
        &self,
        keep: &str,
        evidence: &[String],
        key: &str,
        expected_pending_sha256: &str,
    ) -> Result<Value> {
        if !["journal", "current"].contains(&keep) || evidence.is_empty() || key.trim().is_empty() {
            return Err(Error::new(
                "invalid_arguments",
                "Recovery requires journal/current, evidence and a nonblank idempotency key",
            ));
        }
        let _guard = storage::WorkspaceLock::acquire(&self.root, true)?;
        let mut state = self.load_state()?;
        let args = json!({"keep":keep,"evidence":evidence,"idempotency_key":key,"expected_pending_sha256":expected_pending_sha256});
        let request = domain::digest(&domain::encoded(
            &json!({"name":"resolve_pending","args":args}),
        )?);
        if let Some(prior) = state["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["idempotency_key"] == key)
        {
            if prior["request_sha256"] != request {
                return Err(Error::new(
                    "idempotency_conflict",
                    "Recovery key reused with different input",
                ));
            }
            return Ok(prior["result"].clone());
        }
        self.evidence(&json!(evidence))?;
        let pending = state
            .get("pending")
            .cloned()
            .ok_or_else(|| Error::new("invalid_state", "No pending transaction to resolve"))?;
        if domain::digest(&domain::encoded(&pending)?) != expected_pending_sha256 {
            return Err(Error::new(
                "recovery_conflict",
                "Pending transaction changed since inspection; inspect the exact current journal before resolving",
            ));
        }
        let pending = pending
            .as_array()
            .ok_or_else(|| Error::new("invalid_state", "Malformed pending transaction"))?;
        let mut seen = BTreeSet::new();
        let mut validated = vec![];
        for change in pending {
            let rel = s(change, "path")?;
            if !seen.insert(rel.to_owned()) {
                return Err(Error::new(
                    "invalid_state",
                    "Pending journal contains duplicate canonical paths",
                ));
            }
            let path = self.journal_path(rel)?;
            let wanted = s(change, "text")?.as_bytes().to_vec();
            let current = if path.exists() {
                Some(fs::read(&path)?)
            } else {
                None
            };
            validated.push((rel.to_owned(), path, wanted, current));
        }
        let mut copies = vec![];
        let mut replacement = vec![];
        for (rel, path, wanted, current) in validated {
            let losing = if keep == "journal" {
                current.as_deref()
            } else {
                Some(wanted.as_slice())
            };
            if let Some(bytes) = losing {
                let ext = Path::new(&rel)
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("txt");
                let copy = format!(
                    "applications/operations/conflicts/{}.{}",
                    domain::digest(bytes),
                    ext
                );
                storage::atomic_write(
                    &self.safe_path(&copy, &["applications"], &["md", "json", "txt"])?,
                    bytes,
                )?;
                copies.push(json!({"original_path":rel,"path":copy,"sha256":domain::digest(bytes),"kept":keep}));
            }
            if keep == "journal" {
                replacement.push(json!({"path":rel,"before_sha256":current.map(|bytes|domain::digest(&bytes)),"text":String::from_utf8(wanted).map_err(|_|Error::new("invalid_state","Pending text is not UTF-8"))?}));
            }
            let _ = path;
        }
        let unupdated_paths: Vec<Value> = if keep == "current" {
            pending
                .iter()
                .map(|change| change["path"].clone())
                .collect()
        } else {
            vec![]
        };
        let mut superseded_actions = vec![];
        if keep == "current" {
            let affected: BTreeSet<&str> = pending
                .iter()
                .filter_map(|change| change["path"].as_str())
                .collect();
            for event in state["events"].as_array_mut().unwrap().iter_mut().rev() {
                let named = event
                    .get("journal_paths")
                    .and_then(Value::as_array)
                    .is_some_and(|paths| {
                        paths
                            .iter()
                            .filter_map(Value::as_str)
                            .any(|path| affected.contains(path))
                    });
                let legacy = event.get("journal_paths").is_none()
                    && [
                        "create_application",
                        "update_application",
                        "record_event",
                        "finish_submission",
                    ]
                    .contains(&event["tool"].as_str().unwrap_or_default())
                    && event["result"]
                        .get("record")
                        .and_then(Value::as_str)
                        .is_some_and(|path| affected.contains(path));
                if named || legacy {
                    event["superseded_by"] = json!(key);
                    superseded_actions.push(event["idempotency_key"].clone());
                    break;
                }
            }
        }
        let result = json!({"kept":keep,"pending_sha256":expected_pending_sha256,"conflict_copies":copies,"unupdated_paths":unupdated_paths,"superseded_actions":superseded_actions,"evidence":evidence,"summary":"Explicit local conflict resolution; submission attempts and external outcomes remain unchanged."});
        state["events"].as_array_mut().unwrap().push(json!({"idempotency_key":key,"request_sha256":request,"tool":"resolve_pending","at":domain::now(),"run_id":null,"result":result}));
        if keep == "journal" {
            state["pending"] = json!(replacement);
            storage::commit_state(&self.root, &state)?;
            self.recover(&mut state)?;
        } else {
            state.as_object_mut().unwrap().remove("pending");
            storage::commit_state(&self.root, &state)?;
            let _ = self.write_pipeline();
            let _ = storage::rebuild_index(&self.root);
        }
        Ok(result)
    }
}
