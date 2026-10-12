"""One callable API for MCP, CLI, agents and GUIs. No model or connector SDK."""

import json
import sys
import threading
from copy import deepcopy

from .contracts import SPECS
from .ledger import ROOT, CareerError, Ledger, digest, encoded, validate

REQUIRED_CAPABILITIES = {
    "review_inbox": ["mail.search", "mail.read"],
    "tidy_inbox": ["mail.search", "mail.read", "mail.modify"],
    "discover": ["web.read"],
    "apply": ["web.read", "browser.form", "browser.upload", "browser.submit"],
    "schedule": ["mail.read", "calendar.read", "calendar.book"],
    "prepare_interview": ["web.read"],
    "update_knowledge": [],
    "reconcile": [],
}


def record(row):
    keep = {
        "title",
        "stage",
        "checked",
        "next_action",
        "company",
        "posting_id",
        "vacancy_url",
        "source",
        "status_detail",
        "tracking_source",
    }
    return {
        "record_path": row.get("path") or "applications/" + row["record"],
        **{k: v for k, v in row.items() if k in keep},
    }


def attempt(row):
    keep = {
        "run_id",
        "packet_id",
        "company",
        "vacancy_url",
        "status",
        "summary",
        "reserved_at",
        "began_at",
        "resolved_at",
    }
    return {
        "attempt_id": row["id"],
        "record_path": row["record"],
        **{k: v for k, v in row.items() if k in keep},
    }


def run(row):
    keep = {"operation", "scope", "status", "started_at", "finished_at", "summary"}
    out = {"run_id": row["id"], **{k: v for k, v in row.items() if k in keep}}
    if row["operation"] == "apply" and "max_submissions" in out["scope"]:
        out["scope"] = {
            **out["scope"],
            "max_attempts": out["scope"].get("max_attempts", out["scope"]["max_submissions"] * 3),
        }
    return out


def packet(row):
    return {"record_path": row["record"], **{k: v for k, v in row.items() if k != "record"}}


def page(rows, offset=0, limit=10):
    end = offset + limit
    return rows[offset:end], {
        "total": len(rows),
        "offset": offset,
        "limit": limit,
        "next_offset": end if end < len(rows) else None,
    }


ERROR_RECOVERY = {
    "invalid_arguments": "Correct arguments using the tool input schema.",
    "unknown_tool": "Discover tools/list or CLI schemas; use a listed tool.",
    "deferred_tool": "Inspect the specialised schema with inspect_tools, then run this local tool as a query_batch/execute_batch step, or use the full profile.",
    "path_denied": "Use a repository-relative path returned by a record tool.",
    "missing_resource": "Configure or create the missing ledger file; do not substitute another identity.",
    "pending_recovery": "Call rebuild_pipeline to finish the interrupted local update.",
    "recovery_conflict": "Inspect pending and current records; preserve both and reconcile deliberately.",
    "duplicate_blocked": "Read exact/employer records and live outcome; do not resubmit.",
    "answers_changed": "Cancel the unstarted reservation, refresh the form and use a new preparation key.",
    "submit_replay_blocked": "Inspect external outcome and resolve the existing attempt; do not submit again.",
    "cap_reached": "Finish this bounded run; do not silently increase its cap.",
    "artifact_changed": "Cancel only an unstarted reservation and use a new preparation key; reconcile a begun submission before any retry.",
    "idempotency_conflict": "Inspect the saved action. Changed input needs a new key only for a genuinely new action.",
    "invalid_reference": "Use an exact reference to a prior successful step, e.g. start.data.run_id.",
    "batch_boundary": "Use a separate begin_submission call immediately before one external submit.",
    "adapter_mismatch": "Inspect live provider state and resolve the mismatch; no verified sync was recorded.",
    "io_error": "Inspect local storage and recover pending writes before retrying.",
    "invalid_state": "Inspect the referenced record/run and reconcile its current state before continuing.",
    "batch_failed": "Inspect the failed step. Earlier steps remain committed; retry identical calls with the same keys only after resolving the failure.",
    "internal_contract_error": "Stop and report this tool contract defect; no extra external action is permitted.",
}


class Harness:
    def __init__(self, root, *, allow_uninitialized=False):
        self.ledger = Ledger(root)
        self.allow_uninitialized = allow_uninitialized
        self._lock = threading.RLock()

    def call(self, tool, arguments=None):
        with self._lock:
            return self._call(tool, arguments)

    def _call(self, tool, arguments=None):
        args = arguments if arguments is not None else {}
        try:
            if (
                not self.allow_uninitialized
                and not (self.ledger.root / "config/career-ledger.json").is_file()
            ):
                raise CareerError(
                    "Ledger is not initialized: choose the correct --root or run init for a new ledger",
                    "missing_resource",
                )
            if not isinstance(tool, str) or tool not in SPECS:
                raise CareerError("Unknown harness tool", "unknown_tool")
            validate(SPECS[tool]["inputSchema"], args)
            result, next_steps = self._invoke(tool, deepcopy(args))
            batch_failed = (
                tool in {"query_batch", "execute_batch"} and result["failed_step"] is not None
            )
            out = {
                "schema_version": 2,
                "ok": not batch_failed,
                "data": result,
                "error": {
                    "code": "batch_failed",
                    "message": "Batch stopped at " + result["failed_step"],
                    "retryable": False,
                    "recovery": ERROR_RECOVERY["batch_failed"],
                }
                if batch_failed
                else None,
                "next": next_steps,
            }
            try:
                validate(SPECS[tool]["outputSchema"], out, "result")
            except CareerError:
                raise CareerError(
                    "Generated result does not match its declared schema", "internal_contract_error"
                )
            return out
        except (CareerError, OSError, KeyError, ValueError, TypeError) as error:
            code = getattr(
                error,
                "code",
                "missing_resource"
                if isinstance(error, FileNotFoundError)
                else "io_error"
                if isinstance(error, OSError)
                else "invalid_state",
            )
            return {
                "schema_version": 2,
                "ok": False,
                "data": None,
                "error": {
                    "code": code,
                    "message": str(error)
                    if isinstance(error, CareerError)
                    else "Ledger resource/configuration could not be read or validated.",
                    "retryable": False,
                    "recovery": ERROR_RECOVERY.get(code, ERROR_RECOVERY["invalid_state"]),
                },
                "next": [
                    {
                        "tool": "rebuild_pipeline",
                        "arguments": {
                            "idempotency_key": "recover:"
                            + digest(
                                json.dumps(
                                    self.ledger.state.get("pending"), sort_keys=True
                                ).encode()
                            )[:16]
                        },
                        "reason": "Recover this exact pending local update; inspect source records if recovery conflicts.",
                    }
                ]
                if code == "pending_recovery"
                else [],
            }
        except Exception:
            return {
                "schema_version": 2,
                "ok": False,
                "data": None,
                "error": {
                    "code": "internal_contract_error",
                    "message": "Unexpected local tool failure; preserve state and report the defect.",
                    "retryable": False,
                    "recovery": ERROR_RECOVERY["internal_contract_error"],
                },
                "next": [],
            }

    def _invoke(self, tool, args):
        if tool == "inspect_tools":
            if args.get("include_schemas") and not args.get("names"):
                raise CareerError("Request named tools when fetching schemas", "invalid_arguments")
            rows = []
            for name in args.get("names", list(SPECS)):
                if name not in SPECS:
                    raise CareerError("Unknown requested tool", "unknown_tool")
                spec = SPECS[name]
                row = {
                    "name": name,
                    "description": spec["description"],
                    "read_only": spec["annotations"]["readOnlyHint"],
                }
                if args.get("include_schemas"):
                    row.update(
                        inputSchema=deepcopy(spec["inputSchema"]),
                        outputSchema=deepcopy(spec["outputSchema"]),
                    )
                rows.append(row)
            return {"tools": rows}, []
        if tool in {"query_batch", "execute_batch"}:
            result = self.batch(args["steps"], tool == "query_batch")
            next_steps = result["results"][-1]["result"]["next"] if result["failed_step"] else []
            return result, next_steps
        if tool == "bootstrap":
            plan = (
                self.ledger.dispatch(
                    "plan_workflow", {k: v for k, v in args.items() if k in {"operation", "scope"}}
                )
                if args.get("operation")
                else None
            )
            health = self.compact(
                "reconcile_tracker", self.ledger.dispatch("reconcile_tracker", {}), {}
            )
            supplied = sorted(set(args.get("capabilities", [])))
            missing = sorted(
                set(REQUIRED_CAPABILITIES.get(args.get("operation"), [])) - set(supplied)
            )
            settings = self.ledger.settings()
            required_paths = settings["answer_sources"] + (
                plan["shared_reads"] + [plan["skill"]] if plan else ["AGENTS.md"]
            )
            issues = [
                {
                    "code": "missing_source",
                    "path": path,
                    "message": "Supply this canonical source before acting on its facts/instructions.",
                }
                for path in dict.fromkeys(required_paths)
                if not self.ledger.read_path(path).is_file()
            ]
            if args.get("operation") == "apply":
                cv = settings["primary_cv"]
                if (
                    cv is None
                    or not self.ledger.path(
                        cv, {"applications", "output", "content"}, {".pdf"}
                    ).is_file()
                ):
                    issues.append(
                        {
                            "code": "missing_primary_cv",
                            "path": cv,
                            "message": "Configure primary_cv and supply the approved PDF before preparing an application.",
                        }
                    )
            result = {
                "contract_version": "2.0",
                "ledger": str(self.ledger.root),
                "settings": settings,
                "plan": {
                    k: plan[k] for k in ["operation", "scope", "skill", "result", "shared_reads"]
                }
                if plan
                else None,
                "health": health,
                "available_capabilities": supplied,
                "missing_capabilities": missing,
                "configuration_issues": issues,
                "cli_fallback": {
                    "command": sys.executable,
                    "args": ["-m", "job_harness", "--root", str(self.ledger.root), "call"],
                    "cwd": str(ROOT),
                }
                if sys.executable
                else None,
                "paths": {
                    "toolkit": str(ROOT),
                    "contract": "docs/job-harness-contract.md",
                    "onboarding": "docs/job-harness-onboarding.md",
                },
            }
            next_steps = []
            if health["pipeline_stale"]:
                from .pipeline import build

                next_steps.append(
                    {
                        "tool": "rebuild_pipeline",
                        "arguments": {
                            "idempotency_key": "repair:"
                            + digest(encoded(build(self.ledger.root)[0]))
                        },
                        "reason": "Regenerate stale local views.",
                    }
                )
            if plan:
                next_steps.append(
                    {
                        "tool": "read_record",
                        "arguments": {"path": plan["skill"]},
                        "reason": "Read the focused workflow.",
                    }
                )
            return result, next_steps
        if tool == "get_packet":
            return {
                "packet_id": args["packet_id"],
                "packet": packet(self.ledger.packet(args["packet_id"])),
            }, []
        if tool == "prepare_application":
            key = args.pop("idempotency_key")
            p = self.ledger.dispatch("prepare_packet", {**args, "idempotency_key": key + ":packet"})
            a = self.ledger.dispatch(
                "reserve_submission",
                {
                    "run_id": args["run_id"],
                    "packet_id": p["packet_id"],
                    "idempotency_key": key + ":reserve",
                },
            )["attempt"]
            with self.ledger.reading():
                a = deepcopy(self.ledger.state["attempts"][a["id"]])
                active = self.ledger.state["runs"][args["run_id"]]["status"] == "running"
            return {
                "run_id": args["run_id"],
                "packet_id": p["packet_id"],
                "attempt_id": a["id"],
                "status": a["status"],
                "packet": packet(p["packet"]),
            }, [
                {
                    "tool": "begin_submission",
                    "arguments": {"attempt_id": a["id"], "idempotency_key": key + ":begin"},
                    "reason": "Only at the external submit boundary; verify current authority/form first.",
                }
            ] if a["status"] == "reserved" else [
                {
                    "tool": "prepare_application",
                    "arguments": {
                        **args,
                        "idempotency_key": "replacement:"
                        + digest(encoded({"key": key, "attempt_id": a["id"]}))[:32],
                    },
                    "reason": "Only if the cancellation cause is resolved and this role is still in scope; otherwise stop. A new preparation still checks budget and duplicates.",
                }
            ] if a["status"] == "cancelled" and active else [
                {
                    "tool": "get_run",
                    "arguments": {"run_id": args["run_id"]},
                    "reason": "This existing attempt has progressed; inspect it without repeating Submit.",
                }
            ]
        if tool == "record_adapter_result":
            from .adapters import record_adapter_result

            return self.compact("record_event", record_adapter_result(self.ledger, **args), {}), []
        if tool == "plan_adapter":
            from .adapters import plan_adapter

            return plan_adapter(self.ledger, **args), []
        paging = {k: args.pop(k) for k in ["offset", "limit"] if k in args}
        if tool == "list_events":
            raw = self.ledger.dispatch(
                tool, {**args, "offset": paging.get("offset", 0), "limit": paging.get("limit", 10)}
            )
        else:
            raw = self.ledger.dispatch(tool, args)
        data = self.compact(tool, raw, paging)
        next_steps = []
        if tool == "start_run":
            next_steps = [
                {
                    "tool": "query_batch",
                    "arguments": {
                        "steps": [
                            {
                                "id": "answers",
                                "tool": "get_approved_answer",
                                "arguments": {"query": args["scope"].get("query", "application")},
                            }
                        ]
                    },
                    "reason": "Retrieve only relevant answers/records; read complete sources before acting.",
                }
            ]
        if tool == "prepare_packet":
            next_steps = [
                {
                    "tool": "reserve_submission",
                    "arguments": {
                        "run_id": raw["run_id"],
                        "packet_id": raw["packet_id"],
                        "idempotency_key": args["idempotency_key"] + ":reserve",
                    },
                    "reason": "Reserve after preparing the complete form.",
                }
            ]
        if tool == "begin_submission" and raw["ready_for_single_submit"]:
            # No executable external command is emitted: the driver owns authority and verification.
            next_steps = []
        return data, next_steps

    def compact(self, tool, raw, paging):
        offset, limit = paging.get("offset", 0), paging.get("limit", 10)
        if tool == "list_operations":
            return {
                "operations": [{"operation": k, **v} for k, v in raw["workflows"].items()],
                "shared_reads": raw["shared_reads"],
            }
        if tool in {"find_application", "get_approved_answer"}:
            rows = (
                [record(r) for r in raw["matches"]]
                if tool == "find_application"
                else raw["matches"]
            )
            rows, p = page(rows, offset, limit)
            return {"matches": rows, "page": p}
        if tool == "check_duplicate":
            result = {"blocked": raw["blocked"], "counts": {}, "truncated": False}
            for key in [
                "exact_matches",
                "company_matches",
                "blocking_records",
                "blocking_attempts",
            ]:
                rows = [attempt(r) if key == "blocking_attempts" else record(r) for r in raw[key]]
                result["counts"][key] = len(rows)
                result["truncated"] |= len(rows) > 50
                result[key] = rows[:50]
            return result
        if tool == "start_run":
            return {"run_id": raw["run_id"], "run": run(raw["run"])}
        if tool == "get_run":
            rows, p = page([attempt(a) for a in raw["attempts"]], offset, limit)
            return {"run": run(raw["run"]), "attempts": rows, "page": p}
        if tool == "list_events":
            # The domain has already selected the requested page; strip event payloads.
            rows = []
            for e in raw["events"]:
                v = {k: e[k] for k in ["idempotency_key", "tool", "at", "run_id"] if k in e}
                if e["result"].get("summary"):
                    v["summary"] = e["result"]["summary"]
                if e["result"].get("record"):
                    v["record_path"] = e["result"]["record"]
                rows.append(v)
            total = raw["total_matches"]
            end = offset + limit
            return {
                "events": rows,
                "page": {
                    "total": total,
                    "offset": offset,
                    "limit": limit,
                    "next_offset": end if end < total else None,
                },
            }
        if tool == "reconcile_tracker":
            rows, p = page([attempt(a) for a in raw["unresolved_attempts"]], offset, limit)
            return {
                "records": raw["records"],
                "legacy_records": raw["legacy_records"],
                "needs_review_count": len(raw["needs_review"]),
                "pipeline_stale": raw["pipeline_stale"],
                "unfinished_run_count": len(raw["unfinished_runs"]),
                "unresolved_attempt_count": len(raw["unresolved_attempts"]),
                "unfinished_runs": [run(r) for r in raw["unfinished_runs"]][:50],
                "unresolved_attempts": rows,
                "page": p,
            }
        if tool == "reserve_submission":
            with self.ledger.reading():
                a = deepcopy(self.ledger.state["attempts"][raw["attempt"]["id"]])
            return {
                "run_id": raw["run_id"],
                "attempt_id": a["id"],
                "packet_id": a["packet_id"],
                "status": a["status"],
                "record_path": a["record"],
            }
        data = {k: v for k, v in raw.items() if k not in {"note", "instruction", "plan"}}
        if "record" in data:
            data["record_path"] = data.pop("record")
        if "packet" in data:
            data["packet"] = packet(data["packet"])
        return data

    def batch(self, steps, readonly):
        if len(encoded(steps)) > 250000:
            raise CareerError("Batch exceeds 250 KB", "invalid_arguments")
        seen = set()
        # Reject invalid tool names/boundaries before making any local mutation.
        for s in steps:
            if not s["id"].isidentifier() or s["id"] in seen:
                raise CareerError("Step IDs must be unique identifiers", "invalid_arguments")
            seen.add(s["id"])
            if s["tool"] not in SPECS:
                raise CareerError("Unknown batch tool", "unknown_tool")
            if s["tool"] in {"query_batch", "execute_batch", "begin_submission"}:
                raise CareerError(
                    "Nested batches and begin_submission cannot be batched", "batch_boundary"
                )
            if readonly and not SPECS[s["tool"]]["annotations"]["readOnlyHint"]:
                raise CareerError("query_batch accepts read-only tools only", "invalid_arguments")
        results = []
        values = {}
        for index, s in enumerate(steps):
            try:
                args = resolve_refs(s["arguments"], values)
                value = self.call(s["tool"], args)
            except CareerError as error:
                value = {
                    "schema_version": 2,
                    "ok": False,
                    "data": None,
                    "error": {
                        "code": error.code,
                        "message": str(error),
                        "retryable": False,
                        "recovery": ERROR_RECOVERY.get(error.code, ERROR_RECOVERY["invalid_state"]),
                    },
                    "next": [],
                }
            results.append({"id": s["id"], "tool": s["tool"], "result": value})
            if not value["ok"]:
                return {
                    "results": results,
                    "completed": index,
                    "failed_step": s["id"],
                    "skipped": [x["id"] for x in steps[index + 1 :]],
                }
            values[s["id"]] = value
        return {"results": results, "completed": len(results), "failed_step": None, "skipped": []}


def resolve_refs(value, values):
    if isinstance(value, dict):
        if "$ref" in value:
            if set(value) != {"$ref"} or not isinstance(value["$ref"], str):
                raise CareerError("Malformed reference", "invalid_reference")
            parts = value["$ref"].split(".")
            try:
                item = values[parts[0]]
                for part in parts[1:]:
                    item = item[int(part)] if isinstance(item, list) else item[part]
                if len(parts) < 2:
                    raise KeyError("field required")
                return deepcopy(item)
            except (KeyError, ValueError, IndexError, TypeError):
                raise CareerError(
                    "Reference must identify a prior successful result field", "invalid_reference"
                )
        return {k: resolve_refs(v, values) for k, v in value.items()}
    if isinstance(value, list):
        return [resolve_refs(v, values) for v in value]
    return value
