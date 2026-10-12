//! Bounded data-only composition; submit boundaries remain separate.
use super::*;

impl Engine {
    pub(super) fn batch(&self, args: &Value, readonly: bool) -> Result<(Value, Vec<Value>)> {
        let steps = args["steps"].as_array().unwrap();
        if domain::encoded(&args["steps"])?.len() > 250000 {
            return Err(Error::new("invalid_arguments", "Batch exceeds 250 KB"));
        }
        let specs = contracts::specs();
        let mut seen = BTreeSet::new();
        for step in steps {
            let id = s(step, "id")?;
            if !valid_identifier(id) || !seen.insert(id) {
                return Err(Error::new(
                    "invalid_arguments",
                    "Step IDs must be unique identifiers",
                ));
            }
            let tool = s(step, "tool")?;
            let _spec = specs
                .get(tool)
                .ok_or_else(|| Error::new("unknown_tool", "Unknown batch tool"))?;
            if ["query_batch", "execute_batch", "begin_submission"].contains(&tool) {
                return Err(Error::new(
                    "batch_boundary",
                    "Nested batches and begin_submission cannot be batched",
                ));
            }
            if readonly && contracts::is_read_only(tool) != Some(true) {
                return Err(Error::new(
                    "invalid_arguments",
                    "query_batch accepts read-only tools only",
                ));
            }
        }
        let mut results = vec![];
        let mut values = BTreeMap::new();
        for (index, step) in steps.iter().enumerate() {
            let id = s(step, "id")?;
            let tool = s(step, "tool")?;
            let value = match resolve_refs(&step["arguments"], &values) {
                Ok(args) => self.call(tool, &args),
                Err(error) => self.error_envelope(error),
            };
            results.push(json!({"id":id,"tool":tool,"result":value}));
            if value["ok"] != true {
                let next = value["next"].as_array().cloned().unwrap_or_default();
                return Ok((
                    json!({"results":results,"completed":index,"failed_step":id,"skipped":steps.iter().skip(index+1).map(|s|s["id"].clone()).collect::<Vec<_>>()}),
                    next,
                ));
            }
            values.insert(id.to_owned(), value);
        }
        Ok((
            json!({"results":results,"completed":results.len(),"failed_step":null,"skipped":[]}),
            vec![],
        ))
    }
}
