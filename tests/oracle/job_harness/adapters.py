"""Provider-neutral boundary. Drivers implement capabilities; plans contain no selectors."""

import json
from typing import Protocol

from .ledger import CareerError, digest, encoded
from .pipeline import build

STAGES = {
    "saved": "saved",
    "prepared": "saved",
    "applied": "applied",
    "introduction": "in_process",
    "screening": "in_process",
    "technical": "in_process",
    "final": "in_process",
    "offer": "offer",
    "closed": "archived",
}


def plan_adapter(
    ledger, run_id, record_path, provider, capabilities, target_url=None, failure_code=None
):
    with ledger.reading():
        return _plan_adapter(
            ledger, run_id, record_path, provider, capabilities, target_url, failure_code
        )


def _plan_adapter(ledger, run_id, record_path, provider, capabilities, target_url, failure_code):
    ledger.run(run_id)
    path = ledger.record_path(record_path)
    row = next(r for r in build(ledger.root)[0] if r["record"] == path.name)
    if target_url:
        from .ledger import normal_url

        normal_url(target_url)
    failures = 0
    for e in reversed(ledger.state["events"]):
        result = e["result"]
        if result.get("record") != record_path:
            continue
        try:
            v = json.loads(result.get("summary", ""))
        except ValueError:
            continue
        if not isinstance(v, dict):
            continue
        if v.get("provider") != provider:
            continue
        if result.get("kind") == "reconciled" or v.get("failure_code") != failure_code:
            break
        if result.get("kind") == "exception":
            failures += 1
    stage = STAGES.get(row["stage"])
    actions = []
    note = f"{row.get('checked', 'Date unknown')}: {row['stage']}. {row.get('status_detail', row.get('next_action', 'See canonical record'))}"
    base = {
        "run_id": run_id,
        "record_path": record_path,
        "provider": provider,
        "target_url": target_url,
        "revision": digest(path.read_bytes()),
        "stage": stage,
        "note": note,
    }
    plan_id = digest(encoded(base))
    if stage and target_url and not failure_code:
        actions = [
            {
                "id": plan_id,
                "operation": "sync_stage_and_note",
                "desired_stage": stage,
                "note": note,
                "target_url": target_url,
                "record_path": record_path,
                "capability": "board.sync",
            }
        ]
    needed = ["board.sync"] if actions else ["browser.recover"] if failure_code else []
    return {
        "plan_id": plan_id,
        "record_revision": base["revision"],
        "provider": provider,
        "actions": actions,
        "missing_capabilities": sorted(set(needed) - set(capabilities)),
        "recent_failure_count": failures,
        "stop_retrying": bool(failure_code and failures >= 2),
        "recovery": "Stop repeated identical failures; preserve prepared work and inspect portal/manual recovery."
        if failures >= 2
        else "Inspect the supported upload/auth/verification controls; record the outcome once."
        if failure_code
        else "Read live stage and note once; change only differences and verify the resulting state.",
        "completion_tool": "record_adapter_result",
    }


def record_adapter_result(ledger, **arguments):
    return ledger.mutate(
        "record_adapter_result", arguments, lambda: _record_adapter_result(ledger, **arguments)
    )


def _record_adapter_result(
    ledger,
    run_id,
    record_path,
    provider,
    target_url,
    plan_id,
    outcome,
    evidence,
    idempotency_key,
    observed_stage=None,
    observed_note=None,
    failure_code=None,
):
    if outcome == "failed":
        if not failure_code:
            raise CareerError(
                "Failed adapter outcome needs a stable failure code", "invalid_arguments"
            )
        summary = json.dumps({"provider": provider, "failure_code": failure_code}, sort_keys=True)
        kind = "exception"
    else:
        plan = plan_adapter(ledger, run_id, record_path, provider, ["board.sync"], target_url)
        if plan_id != plan["plan_id"] or not plan["actions"]:
            raise CareerError("Plan is stale or has no executable board action", "adapter_mismatch")
        action = plan["actions"][0]
        if observed_stage != action["desired_stage"] or observed_note != action["note"]:
            raise CareerError(
                "Live board observation does not match the planned stage/note", "adapter_mismatch"
            )
        summary = json.dumps(
            {"provider": provider, "plan_id": plan_id, "observed_stage": observed_stage},
            sort_keys=True,
        )
        kind = "reconciled"
    return ledger.record_event(
        run_id=run_id,
        kind=kind,
        record=record_path,
        summary=summary,
        evidence=evidence,
        idempotency_key=idempotency_key,
    )


class BoardStateDriver(Protocol):
    """A host supplies this small interface using its chosen browser/API."""

    def read(self, target_url: str) -> dict: ...
    def set_stage(self, target_url: str, stage: str) -> None: ...
    def add_note(self, target_url: str, note: str) -> None: ...


def drive_board(harness, driver: BoardStateDriver, arguments: dict) -> dict:
    """One host-side operation: plan, diff, mutate differences, verify, record.

    Never called by local MCP automatically. The host authorizes/binds the driver.
    Exceptions after a write escape to the host; retry must begin with a live read.
    Driver read returns stage, notes (list of exact texts), and evidence (paths/URLs).
    """
    plan_result = harness.call("plan_adapter", arguments)
    if not plan_result["ok"]:
        return plan_result
    plan = plan_result["data"]
    if plan["missing_capabilities"] or not plan["actions"]:
        return {
            **plan_result,
            "ok": False,
            "error": {
                "code": "adapter_not_executable",
                "message": "No executable board action or required capability is missing.",
                "retryable": False,
                "recovery": "Resolve missing capabilities/record stage/destination before requesting a board sync.",
            },
            "next": [],
        }
    action = plan["actions"][0]
    url = action["target_url"]
    before = driver.read(url)
    changed = False
    if before["stage"] != action["desired_stage"]:
        driver.set_stage(url, action["desired_stage"])
        changed = True
    if action["note"] not in before["notes"]:
        driver.add_note(url, action["note"])
        changed = True
    after = driver.read(url) if changed else before
    observations = {"observed_note": action["note"]} if action["note"] in after["notes"] else {}
    return harness.call(
        "record_adapter_result",
        {
            "run_id": arguments["run_id"],
            "record_path": arguments["record_path"],
            "provider": arguments["provider"],
            "target_url": url,
            "plan_id": plan["plan_id"],
            "outcome": "verified",
            "observed_stage": after["stage"],
            **observations,
            "evidence": after["evidence"],
            "idempotency_key": "board:"
            + arguments["provider"]
            + ":"
            + plan["plan_id"]
            + ":"
            + digest(encoded(after["evidence"]))[:16],
        },
    )
