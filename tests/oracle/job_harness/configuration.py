"""Structural schemas for personal configuration, separate from attributed facts."""

TEXT = {"type": "string", "minLength": 1, "maxLength": 20000}


def obj(properties, required):
    return {
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": False,
    }


LEDGER_SCHEMA = obj(
    {
        "schema_version": {"type": "integer", "enum": [1]},
        "timezone": TEXT,
        "primary_cv": {"anyOf": [TEXT, {"type": "null"}]},
        "preferred_browser": {"anyOf": [TEXT, {"type": "null"}]},
        "answer_sources": {"type": "array", "items": TEXT, "minItems": 1, "maxItems": 20},
    },
    ["schema_version"],
)
OPERATIONS = [
    "review_inbox",
    "tidy_inbox",
    "discover",
    "apply",
    "schedule",
    "prepare_interview",
    "update_knowledge",
    "reconcile",
]
WORKFLOW_SCHEMA = obj(
    {
        "schema_version": {"type": "integer", "enum": [1]},
        "shared_reads": {"type": "array", "items": TEXT, "maxItems": 30},
        "workflows": obj(
            {op: obj({"skill": TEXT, "result": TEXT}, ["skill", "result"]) for op in OPERATIONS},
            OPERATIONS,
        ),
    },
    ["schema_version", "shared_reads", "workflows"],
)
