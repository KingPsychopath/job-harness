//! Published schemas with the exact strict input semantics of the legacy engine.
use crate::{HarnessError, Result};
use serde_json::{Map, Value};
use std::sync::LazyLock;
pub static SPECS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../resources/schemas/tools.json"))
        .expect("embedded tool contracts")
});
pub fn specs() -> &'static Map<String, Value> {
    SPECS.as_object().expect("tool catalog")
}
pub fn catalog() -> Vec<Value> {
    specs()
        .iter()
        .map(|(name, spec)| {
            let mut spec = spec.clone();
            spec["name"] = name.as_str().into();
            spec
        })
        .collect()
}
pub fn ledger_schema() -> Value {
    serde_json::from_str(include_str!("../../../resources/schemas/workspace.json"))
        .expect("workspace schema")
}
pub fn workflow_schema() -> Value {
    serde_json::from_str(include_str!("../../../resources/schemas/workflows.json"))
        .expect("workflow schema")
}

/// The published subset counts Unicode code points and keeps integers distinct
/// from floats. Generic JSON Schema validation would accept 1.0 as an integer.
pub fn validate(schema: &Value, value: &Value) -> Result<()> {
    supported_schema(schema)?;
    validate_at(schema, value, "arguments")
}
fn invalid(path: &str, message: &str) -> HarnessError {
    HarnessError::validation(format!("{path}: {message}"))
}
fn validate_at(schema: &Value, value: &Value, path: &str) -> Result<()> {
    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
        if branches
            .iter()
            .any(|branch| validate_at(branch, value, path).is_ok())
        {
            return Ok(());
        }
        return Err(invalid(path, "no matching schema"));
    }
    let kind = schema.get("type").and_then(Value::as_str);
    let valid = match kind {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.as_number().is_some_and(|n| n.is_i64() || n.is_u64()),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        Some("null") => value.is_null(),
        None => true,
        _ => {
            return Err(HarnessError::new(
                "internal_contract_error",
                "Unsupported type in published schema",
            ));
        }
    };
    if !valid {
        return Err(invalid(
            path,
            &format!("expected {}", kind.unwrap_or("value")),
        ));
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return Err(invalid(path, "expected one of the declared values"));
    }
    match kind {
        Some("object") => {
            let object = value.as_object().unwrap();
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for name in required {
                    let name = name.as_str().ok_or_else(|| {
                        HarnessError::new(
                            "internal_contract_error",
                            "Invalid required property schema",
                        )
                    })?;
                    if !object.contains_key(name) {
                        return Err(invalid(path, &format!("missing {name}")));
                    }
                }
            }
            let properties = schema.get("properties").and_then(Value::as_object);
            if schema.get("additionalProperties") == Some(&Value::Bool(false))
                && object
                    .keys()
                    .any(|name| properties.is_none_or(|p| !p.contains_key(name)))
            {
                return Err(invalid(path, "unknown fields"));
            }
            if let Some(properties) = properties {
                for (name, value) in object {
                    if let Some(schema) = properties.get(name) {
                        validate_at(schema, value, &format!("{path}.{name}"))?;
                    }
                }
            }
        }
        Some("array") => {
            let values = value.as_array().unwrap();
            let length = values.len() as u64;
            if length < schema.get("minItems").and_then(Value::as_u64).unwrap_or(0)
                || schema
                    .get("maxItems")
                    .and_then(Value::as_u64)
                    .is_some_and(|max| length > max)
            {
                return Err(invalid(path, "item count out of range"));
            }
            if let Some(items) = schema.get("items") {
                for value in values {
                    validate_at(items, value, path)?;
                }
            }
        }
        Some("string") => {
            let text = value.as_str().unwrap();
            if let Some(pattern) = schema.get("pattern").and_then(Value::as_str)
                && !regex::Regex::new(pattern)
                    .map_err(|_| {
                        HarnessError::new(
                            "internal_contract_error",
                            "Invalid published string pattern",
                        )
                    })?
                    .is_match(text)
            {
                return Err(invalid(path, "does not match the published string pattern"));
            }
            let length = text.chars().count() as u64;
            let min = schema.get("minLength").and_then(Value::as_u64).unwrap_or(0);
            if min > 0 && text.trim().is_empty() {
                return Err(invalid(path, "must not be blank"));
            }
            if length < min
                || length
                    > schema
                        .get("maxLength")
                        .and_then(Value::as_u64)
                        .unwrap_or(1_000_000)
            {
                return Err(invalid(path, "length out of range"));
            }
        }
        Some("integer" | "number") => {
            let n = value
                .as_f64()
                .ok_or_else(|| invalid(path, "out of range"))?;
            if schema
                .get("minimum")
                .and_then(Value::as_f64)
                .is_some_and(|min| n < min)
                || schema
                    .get("maximum")
                    .and_then(Value::as_f64)
                    .is_some_and(|max| n > max)
            {
                return Err(invalid(path, "out of range"));
            }
        }
        _ => {}
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn integer_float_bool_are_distinct() {
        let schema = json!({"type":"integer","minimum":1});
        assert!(validate(&schema, &json!(1)).is_ok());
        assert!(validate(&schema, &json!(1.0)).is_err());
        assert!(validate(&schema, &json!(true)).is_err());
    }
    #[test]
    fn string_lengths_are_unicode_codepoints_and_blank_is_rejected() {
        let schema = json!({"type":"string","minLength":1,"maxLength":1});
        assert!(validate(&schema, &json!("😀")).is_ok());
        assert!(validate(&schema, &json!("  ")).is_err());
        assert!(validate(&schema, &json!("😀😀")).is_err());
    }
    #[test]
    fn published_schemas_remain_valid_json_schema() {
        for spec in specs().values() {
            jsonschema::validator_for(&spec["inputSchema"]).unwrap();
            jsonschema::validator_for(&spec["outputSchema"]).unwrap();
        }
    }
}

/// Capability authority is compiled code, never a missing or edited annotation.
pub fn is_read_only(name: &str) -> Option<bool> {
    match name {
        "list_operations"
        | "plan_workflow"
        | "read_record"
        | "find_application"
        | "get_approved_answer"
        | "check_duplicate"
        | "get_run"
        | "list_events"
        | "reconcile_tracker"
        | "bootstrap"
        | "get_packet"
        | "inspect_tools"
        | "query_batch"
        | "plan_adapter"
        | "search_workspace"
        | "review_queue"
        | "preview_packet"
        | "review_note"
        | "list_notes"
        | "context_pack"
        | "workflow_check"
        | "daily_check" => Some(true),
        "start_run"
        | "begin_mail_archive"
        | "daily_update"
        | "finish_run"
        | "record_event"
        | "update_application"
        | "create_application"
        | "save_receipt"
        | "prepare_packet"
        | "reserve_submission"
        | "begin_submission"
        | "finish_submission"
        | "cancel_reservation"
        | "rebuild_pipeline"
        | "prepare_application"
        | "execute_batch"
        | "record_adapter_result"
        | "intake_note"
        | "propose_note"
        | "accept_note"
        | "archive_note" => Some(false),
        _ => None,
    }
}
pub fn validate_catalog() -> Result<()> {
    for (name, spec) in specs() {
        let compiled = is_read_only(name).ok_or_else(|| {
            HarnessError::new(
                "internal_contract_error",
                "Catalog includes an unknown compiled capability",
            )
        })?;
        if spec["annotations"]["readOnlyHint"].as_bool() != Some(compiled) {
            return Err(HarnessError::new(
                "internal_contract_error",
                "Tool annotation disagrees with its compiled capability authority",
            ));
        }
        if !compiled
            && !spec["inputSchema"]["required"]
                .as_array()
                .is_some_and(|required| {
                    required.iter().any(|v| v == "idempotency_key") || name == "execute_batch"
                })
        {
            return Err(HarnessError::new(
                "internal_contract_error",
                "A mutating capability requires an idempotency key",
            ));
        }
        supported_schema(&spec["inputSchema"])?;
        supported_schema(&spec["outputSchema"])?;
    }
    supported_schema(&ledger_schema())?;
    supported_schema(&workflow_schema())?;
    Ok(())
}
fn supported_schema(schema: &Value) -> Result<()> {
    let object = schema.as_object().ok_or_else(|| {
        HarnessError::new(
            "internal_contract_error",
            "Published schemas must be objects",
        )
    })?;
    for (key, value) in object {
        match key.as_str() {
            "type" => {
                if !value.as_str().is_some_and(|kind| {
                    [
                        "object", "array", "string", "integer", "number", "boolean", "null",
                    ]
                    .contains(&kind)
                }) {
                    return Err(HarnessError::new(
                        "internal_contract_error",
                        "Published schema contains an unsupported type",
                    ));
                }
            }
            "anyOf" => {
                for branch in value.as_array().ok_or_else(|| {
                    HarnessError::new("internal_contract_error", "Invalid anyOf schema")
                })? {
                    supported_schema(branch)?;
                }
            }
            "properties" => {
                for child in value
                    .as_object()
                    .ok_or_else(|| {
                        HarnessError::new("internal_contract_error", "Invalid properties schema")
                    })?
                    .values()
                {
                    supported_schema(child)?;
                }
            }
            "items" => supported_schema(value)?,
            "additionalProperties" => {
                if !value.is_boolean() {
                    return Err(HarnessError::new(
                        "internal_contract_error",
                        "Only boolean additionalProperties is supported",
                    ));
                }
            }
            "pattern" => {
                regex::Regex::new(value.as_str().ok_or_else(|| {
                    HarnessError::new("internal_contract_error", "String pattern must be text")
                })?)
                .map_err(|_| {
                    HarnessError::new(
                        "internal_contract_error",
                        "Invalid published string pattern",
                    )
                })?;
            }
            "required" | "enum" | "minItems" | "maxItems" | "minLength" | "maxLength"
            | "minimum" | "maximum" | "description" | "title" | "$schema" => {}
            _ => {
                return Err(HarnessError::new(
                    "internal_contract_error",
                    format!("Unsupported schema keyword: {key}"),
                ));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod authority_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn every_published_schema_uses_the_authoritative_supported_subset() {
        validate_catalog().unwrap();
    }
    #[test]
    fn unsupported_validation_keywords_and_union_types_are_refused() {
        for schema in [
            json!({"type":["string","null"]}),
            json!({"type":"string","format":"uri"}),
            json!({"oneOf":[]}),
            json!({"$ref":"other"}),
        ] {
            assert_eq!(
                supported_schema(&schema).unwrap_err().code,
                "internal_contract_error"
            );
        }
    }
}
