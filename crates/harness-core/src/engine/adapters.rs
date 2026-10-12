//! Provider-neutral plans and verified board observations.
use super::*;

impl Engine {
    pub(super) fn plan_adapter(&self, args: &Value, state: &Value) -> Result<Value> {
        self.active_run(state, s(args, "run_id")?)?;
        let record = s(args, "record_path")?;
        let path = self.record_path(record, true)?;
        let row = pipeline::build(&self.root)?
            .records
            .into_iter()
            .find(|r| r["record"].as_str() == path.file_name().and_then(|x| x.to_str()))
            .ok_or_else(|| Error::new("missing_resource", "Missing canonical record"))?;
        if let Some(url) = args.get("target_url").and_then(Value::as_str) {
            domain::normal_url(url)?;
        }
        let provider = s(args, "provider")?;
        let mut failures = 0;
        for event in state["events"].as_array().unwrap().iter().rev() {
            let result = &event["result"];
            if result["record"] != record {
                continue;
            }
            let value = match result["summary"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
            {
                Some(v) if v.is_object() => v,
                _ => continue,
            };
            if value["provider"] != provider {
                continue;
            }
            if result["kind"] == "reconciled"
                || value.get("failure_code") != args.get("failure_code")
            {
                break;
            }
            if result["kind"] == "exception" {
                failures += 1;
            }
        }
        let stage = match row["stage"].as_str().unwrap_or_default() {
            "saved" | "prepared" => Some("saved"),
            "applied" => Some("applied"),
            "introduction" | "screening" | "technical" | "final" => Some("in_process"),
            "offer" => Some("offer"),
            "closed" => Some("archived"),
            _ => None,
        };
        let note = format!(
            "{}: {}. {}",
            row["checked"].as_str().unwrap_or("Date unknown"),
            row["stage"].as_str().unwrap_or_default(),
            row.get("status_detail")
                .or_else(|| row.get("next_action"))
                .and_then(Value::as_str)
                .unwrap_or("See canonical record")
        );
        let revision = domain::digest(&fs::read(path)?);
        let base = json!({"run_id":args["run_id"],"record_path":record,"provider":provider,"target_url":args.get("target_url").cloned().unwrap_or(Value::Null),"revision":revision,"stage":stage,"note":note});
        let plan_id = domain::digest(&domain::encoded(&base)?);
        let actions = if stage.is_some()
            && args.get("target_url").is_some()
            && args.get("failure_code").is_none()
        {
            vec![
                json!({"id":plan_id,"operation":"sync_stage_and_note","desired_stage":stage,"note":note,"target_url":args["target_url"],"record_path":record,"capability":"board.sync"}),
            ]
        } else {
            vec![]
        };
        let needed = if !actions.is_empty() {
            vec!["board.sync"]
        } else if args.get("failure_code").is_some() {
            vec!["browser.recover"]
        } else {
            vec![]
        };
        let supplied = string_array(&args["capabilities"])?;
        let missing: Vec<_> = needed
            .into_iter()
            .filter(|s| !supplied.iter().any(|p| p == s))
            .collect();
        Ok(
            json!({"plan_id":plan_id,"record_revision":revision,"provider":provider,"actions":actions,"missing_capabilities":missing,"recent_failure_count":failures,"stop_retrying":args.get("failure_code").is_some()&&failures>=2,"recovery":if failures>=2{"Stop repeated identical failures; preserve prepared work and inspect portal/manual recovery."}else if args.get("failure_code").is_some(){"Inspect the supported upload/auth/verification controls; record the outcome once."}else{"Read live stage and note once; change only differences and verify the resulting state."},"completion_tool":"record_adapter_result"}),
        )
    }
    pub(super) fn adapter_result(
        &self,
        args: &Value,
        state: &Value,
        changes: &mut Vec<Value>,
    ) -> Result<Value> {
        let failed = args["outcome"] == "failed";
        let summary = if failed {
            let code = s(args, "failure_code").map_err(|_| {
                Error::new(
                    "invalid_arguments",
                    "Failed adapter outcome needs a stable failure code",
                )
            })?;
            serde_json::to_string(&json!({"provider":args["provider"],"failure_code":code}))?
        } else {
            let mut plan_args = args.clone();
            plan_args["capabilities"] = json!(["board.sync"]);
            plan_args.as_object_mut().unwrap().remove("failure_code");
            let plan = self.plan_adapter(&plan_args, state)?;
            if args["plan_id"] != plan["plan_id"] || plan["actions"].as_array().unwrap().is_empty()
            {
                return Err(Error::new(
                    "adapter_mismatch",
                    "Plan is stale or has no executable board action",
                ));
            }
            let action = &plan["actions"][0];
            if args["observed_stage"] != action["desired_stage"]
                || args["observed_note"] != action["note"]
            {
                return Err(Error::new(
                    "adapter_mismatch",
                    "Live board observation does not match the planned stage/note",
                ));
            }
            serde_json::to_string(
                &json!({"provider":args["provider"],"plan_id":args["plan_id"],"observed_stage":args["observed_stage"]}),
            )?
        };
        let event = json!({"run_id":args["run_id"],"kind":if failed{"exception"}else{"reconciled"},"record":args["record_path"],"summary":summary,"evidence":args["evidence"],"idempotency_key":args["idempotency_key"]});
        self.record_event("record_event", &event, state, changes)
    }
}
