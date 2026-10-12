//! Bounded work intents and event history. Runs never grant external authority.
use super::*;

impl Engine {
    pub(super) fn active_run(&self, state: &Value, id: &str) -> Result<Value> {
        let run = state["runs"]
            .get(id)
            .ok_or_else(|| Error::new("invalid_state", "Unknown run"))?;
        if run["status"] != "running" {
            return Err(Error::new(
                "invalid_state",
                "Run is finished; start a new bounded run",
            ));
        }
        Ok(run.clone())
    }
    pub(super) fn start_run(&self, args: &Value, state: &mut Value) -> Result<Value> {
        let op = s(args, "operation")?;
        let scope = &args["scope"];
        self.plan(op, scope.clone())?;
        if op == "apply"
            && (scope["max_submissions"].as_u64().unwrap_or(0) == 0
                || scope["sources"].as_array().is_none_or(Vec::is_empty))
        {
            return Err(Error::new(
                "invalid_arguments",
                "Apply runs need an explicit positive submission cap and source scope",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let run = json!({"id":id,"operation":op,"scope":scope,"user_request":args["user_request"],"status":"running","started_at":domain::now(),"ledger_revision":self.revision(),"ledger_dirty":self.dirty(),"settings":self.settings()?});
        state["runs"][&id] = run.clone();
        Ok(json!({"run_id":id,"run":run}))
    }
    pub(super) fn get_run(&self, args: &Value, state: &Value) -> Result<Value> {
        let id = s(args, "run_id")?;
        let run = state["runs"]
            .get(id)
            .ok_or_else(|| Error::new("invalid_state", "Unknown run"))?;
        let attempts: Vec<Value> = state["attempts"]
            .as_object()
            .unwrap()
            .values()
            .filter(|a| a["run_id"] == id)
            .cloned()
            .collect();
        Ok(
            json!({"run":run,"attempts":attempts,"archive_actions":pending_mail_actions(state).into_iter().filter(|row|row["run_id"]==id).take(100).map(|row|daily::public_archive_row(&row)).collect::<Vec<_>>()}),
        )
    }
    pub(super) fn events(&self, args: &Value, state: &Value) -> Result<Value> {
        let rows: Vec<Value> = state["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| {
                (args.get("idempotency_key").is_none()
                    || args["idempotency_key"] == e["idempotency_key"])
                    && (args.get("run_id").is_none()
                        || args["run_id"] == e["run_id"]
                        || args["run_id"] == e["result"]["run_id"])
            })
            .cloned()
            .collect();
        let offset = n(args, "offset", 0);
        let limit = n(args, "limit", 10);
        Ok(
            json!({"events":rows.iter().skip(offset).take(limit).collect::<Vec<_>>(),"total_matches":rows.len()}),
        )
    }
    pub(super) fn finish_run(&self, args: &Value, state: &mut Value) -> Result<Value> {
        let id = s(args, "run_id")?;
        self.active_run(state, id)?;
        self.evidence(&args["evidence"])?;
        let active = state["attempts"].as_object().unwrap().values().any(|a| {
            a["run_id"] == id
                && SubmissionState::parse(&a["status"]).is_ok_and(SubmissionState::unresolved)
        });
        let unresolved_mail = pending_mail_actions(state)
            .iter()
            .any(|row| row["run_id"] == id && row["status"] != "queued");
        if args["status"] == "completed" && (active || unresolved_mail) {
            return Err(Error::new(
                "invalid_state",
                "Cannot complete a run with unresolved external action attempts",
            ));
        }
        for key in ["status", "summary", "evidence"] {
            state["runs"][id][key] = args[key].clone();
        }
        state["runs"][id]["finished_at"] = json!(domain::now());
        Ok(
            json!({"run_id":id,"status":args["status"],"summary":args["summary"],"evidence":args["evidence"]}),
        )
    }
}
