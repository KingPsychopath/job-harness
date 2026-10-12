"""Versioned public contracts. Core domain state is deliberately not the wire format."""

from copy import deepcopy

from .ledger import SCOPE, obj
from .ledger import SPECS as DOMAIN_SPECS

T = {"type": "string"}
B = {"type": "boolean"}
N = {"type": "integer", "minimum": 0}
NULL = {"type": "null"}
J = {
    "type": "object"
}  # Exact user answers/configuration and nested event payloads are extensible data.


def arr(item, maximum=100):
    return {"type": "array", "items": item, "maxItems": maximum}


def nullable(item):
    return {"anyOf": [item, NULL]}


PAGE = obj({"total": N, "offset": N, "limit": N, "next_offset": nullable(N)})
ROW = obj(
    {
        "record_path": T,
        "title": T,
        "stage": T,
        "checked": T,
        "next_action": T,
        "company": T,
        "posting_id": nullable(T),
        "vacancy_url": T,
        "source": T,
        "status_detail": T,
        "tracking_source": T,
    },
    ["record_path", "title", "stage"],
)
ARTIFACT = obj({"path": T, "sha256": T, "original_path": T})
PACKET = obj(
    {
        "record_path": T,
        "company": T,
        "vacancy_url": T,
        "source": T,
        "posting_id": nullable(T),
        "cv": ARTIFACT,
        "answers": ARTIFACT,
        "source_record": ARTIFACT,
        "ledger_revision": T,
        "ledger_dirty": nullable(B),
        "answer_source_sha256": J,
    }
)
ATTEMPT = obj(
    {
        "attempt_id": T,
        "run_id": T,
        "packet_id": T,
        "record_path": T,
        "company": T,
        "vacancy_url": T,
        "status": T,
        "summary": T,
        "reserved_at": T,
        "began_at": T,
        "resolved_at": T,
    },
    ["attempt_id", "run_id", "record_path", "company", "status"],
)
RUN = obj(
    {
        "run_id": T,
        "operation": T,
        "scope": J,
        "status": T,
        "started_at": T,
        "finished_at": T,
        "summary": T,
    },
    ["run_id", "operation", "scope", "status"],
)
STEP = obj({"tool": T, "arguments": J, "reason": T})
ERROR = obj({"code": T, "message": T, "retryable": B, "recovery": T})
EVENT = obj(
    {
        "idempotency_key": T,
        "tool": T,
        "at": T,
        "run_id": nullable(T),
        "summary": T,
        "record_path": T,
    },
    ["idempotency_key", "tool", "at"],
)
PLAN = obj(
    {
        "operation": T,
        "scope": J,
        "skill": T,
        "result": T,
        "shared_reads": arr(T),
        "settings": J,
        "ledger": T,
        "authority": T,
    }
)
ROUTE = obj({"operation": T, "scope": J, "skill": T, "result": T, "shared_reads": arr(T)})
HEALTH = obj(
    {
        "records": N,
        "legacy_records": N,
        "needs_review_count": N,
        "unfinished_run_count": N,
        "unresolved_attempt_count": N,
        "pipeline_stale": B,
        "unfinished_runs": arr(RUN),
        "unresolved_attempts": arr(ATTEMPT),
        "page": PAGE,
    }
)
CAPABILITIES = arr(T, 30)
ADAPTER_ACTION = obj(
    {
        "id": T,
        "operation": T,
        "desired_stage": T,
        "note": T,
        "target_url": T,
        "record_path": T,
        "capability": T,
    },
    ["id", "operation", "record_path", "capability"],
)
ADAPTER_PLAN = obj(
    {
        "plan_id": T,
        "record_revision": T,
        "provider": T,
        "actions": arr(ADAPTER_ACTION, 2),
        "missing_capabilities": CAPABILITIES,
        "recent_failure_count": N,
        "stop_retrying": B,
        "recovery": T,
        "completion_tool": T,
    }
)

DATA = {
    "list_operations": obj(
        {"operations": arr(obj({"operation": T, "skill": T, "result": T})), "shared_reads": arr(T)}
    ),
    "plan_workflow": PLAN,
    "read_record": obj(
        {
            "path": T,
            "origin": {"type": "string", "enum": ["ledger", "toolkit"]},
            "sha256": T,
            "text": T,
        }
    ),
    "find_application": obj({"matches": arr(ROW), "page": PAGE}),
    "get_approved_answer": obj({"matches": arr(obj({"path": T, "text": T})), "page": PAGE}),
    "check_duplicate": obj(
        {
            "blocked": B,
            "exact_matches": arr(ROW),
            "company_matches": arr(ROW),
            "blocking_records": arr(ROW),
            "blocking_attempts": arr(ATTEMPT),
            "counts": J,
            "truncated": B,
        }
    ),
    "start_run": obj({"run_id": T, "run": RUN}),
    "get_run": obj({"run": RUN, "attempts": arr(ATTEMPT), "page": PAGE}),
    "list_events": obj({"events": arr(EVENT), "page": PAGE}),
    "finish_run": obj({"run_id": T, "status": T, "summary": T, "evidence": arr(T, 1000)}),
    "record_event": obj(
        {
            "run_id": T,
            "kind": T,
            "summary": T,
            "evidence": arr(T, 1000),
            "record_path": T,
            "tracking": J,
        },
        ["run_id", "kind", "summary", "evidence"],
    ),
    "update_application": obj(
        {
            "run_id": T,
            "kind": T,
            "summary": T,
            "evidence": arr(T, 1000),
            "record_path": T,
            "tracking": J,
        },
        ["run_id", "kind", "summary", "evidence"],
    ),
    "create_application": obj({"run_id": T, "record_path": T, "tracking": J}),
    "save_receipt": obj({"run_id": T, "artifact": ARTIFACT}),
    "prepare_packet": obj({"run_id": T, "packet_id": T, "packet": PACKET}),
    "reserve_submission": obj(
        {"run_id": T, "attempt_id": T, "packet_id": T, "status": T, "record_path": T}
    ),
    "begin_submission": obj(
        {
            "run_id": T,
            "attempt_id": T,
            "status": T,
            "ready_for_single_submit": B,
            "packet": PACKET,
            "replayed": B,
        },
        ["run_id", "attempt_id", "status", "ready_for_single_submit", "packet"],
    ),
    "finish_submission": obj(
        {"run_id": T, "attempt_id": T, "status": T, "record_path": T, "evidence": arr(T, 1002)}
    ),
    "cancel_reservation": obj({"run_id": T, "attempt_id": T, "status": T, "summary": T}),
    "reconcile_tracker": HEALTH,
    "rebuild_pipeline": obj({"records": N, "pipeline": T}),
    "bootstrap": obj(
        {
            "contract_version": T,
            "ledger": T,
            "settings": J,
            "plan": nullable(ROUTE),
            "health": HEALTH,
            "available_capabilities": CAPABILITIES,
            "missing_capabilities": CAPABILITIES,
            "configuration_issues": arr(obj({"code": T, "path": nullable(T), "message": T})),
            "cli_fallback": nullable(obj({"command": T, "args": arr(T, 20), "cwd": T})),
            "paths": J,
        }
    ),
    "get_packet": obj({"packet_id": T, "packet": PACKET}),
    "inspect_tools": obj(
        {
            "tools": arr(
                obj(
                    {
                        "name": T,
                        "description": T,
                        "read_only": B,
                        "inputSchema": J,
                        "outputSchema": J,
                    },
                    ["name", "description", "read_only"],
                )
            )
        }
    ),
    "prepare_application": obj(
        {"run_id": T, "packet_id": T, "attempt_id": T, "status": T, "packet": PACKET}
    ),
    "plan_adapter": ADAPTER_PLAN,
    "record_adapter_result": obj(
        {"run_id": T, "kind": T, "summary": T, "evidence": arr(T, 1000), "record_path": T}
    ),
}


def envelope(data):
    return obj(
        {
            "schema_version": {"type": "integer", "enum": [2]},
            "ok": B,
            "data": nullable(data),
            "error": nullable(ERROR),
            "next": arr(STEP, 5),
        }
    )


SPECS = deepcopy(DOMAIN_SPECS)
PAGING = {"offset": N, "limit": {"type": "integer", "minimum": 1, "maximum": 50}}
for name in [
    "find_application",
    "get_approved_answer",
    "get_run",
    "list_events",
    "reconcile_tracker",
]:
    SPECS[name]["inputSchema"]["properties"].update(PAGING)
SPECS["bootstrap"] = {
    "description": "Start here: one read-only call returns settings, task routing, recovery summary, missing capabilities and CLI fallback.",
    "inputSchema": obj(
        {
            "operation": deepcopy(
                DOMAIN_SPECS["plan_workflow"]["inputSchema"]["properties"]["operation"]
            ),
            "scope": SCOPE,
            "capabilities": CAPABILITIES,
        },
        [],
    ),
}
SPECS["get_packet"] = {
    "description": "Read hash-verified frozen artifact paths and packet metadata by ID.",
    "inputSchema": obj({"packet_id": T}),
}
SPECS["inspect_tools"] = {
    "description": "Discover compact tool descriptions, or fetch exact schemas for up to 10 named tools. Use deferred local tools through query_batch/execute_batch.",
    "inputSchema": obj(
        {
            "names": {"type": "array", "items": T, "minItems": 1, "maxItems": 10},
            "include_schemas": B,
        },
        [],
    ),
}
SPECS["prepare_application"] = {
    "description": "Freeze and reserve one application with one call. Does not begin or submit; begin_submission remains a separate one-shot boundary.",
    "inputSchema": deepcopy(DOMAIN_SPECS["prepare_packet"]["inputSchema"]),
}
SPECS["plan_adapter"] = {
    "description": "Build one provider-neutral board-sync or portal-recovery plan. Pure plan; driver uses its own UI/API and returns verified observations.",
    "inputSchema": obj(
        {
            "run_id": T,
            "record_path": T,
            "provider": T,
            "target_url": T,
            "capabilities": CAPABILITIES,
            "failure_code": T,
        },
        ["run_id", "record_path", "provider", "capabilities"],
    ),
}
SPECS["record_adapter_result"] = {
    "description": "Record one evidence-backed live adapter result. Verified stage/note must match a current plan; failure codes feed bounded recovery guidance.",
    "inputSchema": obj(
        {
            "run_id": T,
            "record_path": T,
            "provider": T,
            "target_url": T,
            "plan_id": T,
            "outcome": {"type": "string", "enum": ["verified", "failed"]},
            "evidence": {"type": "array", "items": T, "minItems": 1, "maxItems": 20},
            "idempotency_key": T,
            "observed_stage": T,
            "observed_note": T,
            "failure_code": T,
        },
        [
            "run_id",
            "record_path",
            "provider",
            "target_url",
            "plan_id",
            "outcome",
            "evidence",
            "idempotency_key",
        ],
    ),
}
# Batches contain data, never shell/JavaScript. Resolved arguments are validated against the child contract.
BATCH_ITEM = obj({"id": T, "tool": T, "arguments": J})
BATCH_INPUT = obj({"steps": {"type": "array", "items": BATCH_ITEM, "minItems": 1, "maxItems": 20}})
BATCH_RESULT = obj({"id": T, "tool": T, "result": envelope(J)})
for name, read_only in [("query_batch", True), ("execute_batch", False)]:
    SPECS[name] = {
        "description": (
            "Batch up to 20 read-only calls."
            if read_only
            else "Execute up to 20 ordered local steps, committing each separately. Stops on first error; earlier successes remain durable. No begin_submission or nested batches."
        )
        + ' Chain exact {$ref:"step.data.field"} values from prior successful steps. Return per-step envelopes; no code execution.',
        "inputSchema": deepcopy(BATCH_INPUT),
    }
    DATA[name] = obj(
        {
            "results": arr(BATCH_RESULT, 20),
            "completed": N,
            "failed_step": nullable(T),
            "skipped": arr(T, 20),
        }
    )
for name, spec in SPECS.items():
    spec["outputSchema"] = envelope(DATA[name])
    spec.setdefault(
        "annotations",
        {
            "readOnlyHint": name
            in {"bootstrap", "get_packet", "plan_adapter", "query_batch", "inspect_tools"},
            "destructiveHint": False,
            "idempotentHint": True,
            "openWorldHint": False,
        },
    )


# Bound all declared input strings. Extensible arguments are validated after reference resolution.
def bound_strings(schema):
    if schema.get("type") == "string":
        schema.setdefault("minLength", 1)
        schema.setdefault("maxLength", 20000)
    for child in schema.get("properties", {}).values():
        bound_strings(child)
    if "items" in schema:
        bound_strings(schema["items"])


for spec in SPECS.values():
    spec["inputSchema"] = deepcopy(spec["inputSchema"])
    if "evidence" in spec["inputSchema"].get("properties", {}):
        spec["inputSchema"]["properties"]["evidence"].setdefault("maxItems", 1000)
    bound_strings(spec["inputSchema"])
SPECS["get_run"]["description"] = (
    "Read compact run and paged submission attempts for recovery; use list_events(run_id) for history."
)
