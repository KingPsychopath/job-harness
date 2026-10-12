"""Local career ledger tools. No network, browser, email or credential access."""

import fcntl
import hashlib
import json
import os
import re
import subprocess
import tempfile
import threading
import uuid
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit
from zoneinfo import ZoneInfo

from .configuration import LEDGER_SCHEMA, WORKFLOW_SCHEMA
from .errors import CareerError
from .pipeline import BLOCK, RESERVED_NAMES, STAGES, build

ROOT = Path(__file__).resolve().parents[1]
SCHEDULES = [
    "unbooked",
    "proposed",
    "availability-sent",
    "booked",
    "completed",
    "awaiting-outcome",
    "handled-unconfirmed",
]
KINDS = [
    "mail_reviewed",
    "mail_archived",
    "reply_sent",
    "booking_confirmed",
    "calendar_accepted",
    "knowledge_updated",
    "status_updated",
    "discovered",
    "exception",
    "reconciled",
]
TRACKING_KEYS = {
    "stage",
    "checked",
    "next_action",
    "status_detail",
    "schedule_state",
    "proposed_time",
    "scheduled_start",
    "booking_url",
    "contact",
    "brief",
    "record_evidence",
    "evidence",
    "company",
    "posting_id",
    "vacancy_url",
    "source",
}
READ_ROOTS = {"context", "content", "applications", "docs", "config", ".agents"}
READ_FILES = {"README.md", "AGENTS.md"}
ARTIFACT_ROOTS = {"applications", "output", "content"}


def now():
    return datetime.now(timezone.utc).isoformat()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode()


def atomic_write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(dir=path.parent, prefix=".career-tmp-")
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def normal_url(value):
    try:
        parts = urlsplit(value)
        parts.port  # Reject malformed hosts/ports before normalizing public references.
    except ValueError as error:
        raise CareerError(
            "URL: valid http(s) host and port required", "invalid_arguments"
        ) from error
    if (
        parts.scheme not in {"https", "http"}
        or not parts.hostname
        or parts.username
        or parts.password
    ):
        raise CareerError(
            "A public vacancy/evidence URL must use http(s), without credentials",
            "invalid_arguments",
        )
    query = [
        (k, v)
        for k, v in parse_qsl(parts.query)
        if not k.lower().startswith("utm_") and k.lower() not in {"source", "ref", "gh_src"}
    ]
    return urlunsplit(
        (
            parts.scheme.lower(),
            parts.netloc.lower(),
            parts.path.rstrip("/"),
            urlencode(sorted(query)),
            "",
        )
    )


def company_key(value):
    return "".join(c for c in value.casefold() if c.isalnum())


def validate(schema, value, label="arguments"):
    """The JSON Schema subset used by our published contracts, for inputs and outputs."""
    if "anyOf" in schema:
        for branch in schema["anyOf"]:
            try:
                validate(branch, value, label)
                return
            except CareerError:
                pass
        raise CareerError(f"{label}: no matching schema", "invalid_arguments")
    types = {
        "object": dict,
        "array": list,
        "string": str,
        "integer": int,
        "boolean": bool,
        "null": type(None),
    }
    kind = schema.get("type")

    def fail(message):
        raise CareerError(f"{label}: {message}", "invalid_arguments")

    if kind and (
        not isinstance(value, types[kind]) or (kind == "integer" and isinstance(value, bool))
    ):
        fail(f"expected {kind}")
    if "enum" in schema and value not in schema["enum"]:
        fail(f"expected one of {schema['enum']}")
    if kind == "object":
        for key in schema.get("required", []):
            if key not in value:
                fail(f"missing {key}")
        properties = schema.get("properties", {})
        if schema.get("additionalProperties") is False and set(value) - set(properties):
            fail(f"unknown fields {sorted(set(value) - set(properties))}")
        for key, item in value.items():
            if key in properties:
                validate(properties[key], item, f"{label}.{key}")
    if kind == "array":
        if not schema.get("minItems", 0) <= len(value) <= schema.get("maxItems", float("inf")):
            fail("item count out of range")
        for item in value:
            validate(schema.get("items", {}), item, label)
    if kind == "string" and schema.get("minLength", 0) > 0 and not value.strip():
        fail("must not be blank")
    if kind == "string" and not schema.get("minLength", 0) <= len(value) <= schema.get(
        "maxLength", 1000000
    ):
        fail("length out of range")
    if kind == "integer" and not schema.get("minimum", float("-inf")) <= value <= schema.get(
        "maximum", 10**20
    ):
        fail("out of range")


class Ledger:
    def __init__(self, root):
        self.root = Path(root).resolve()
        self.home = self.root / "applications/operations"
        self.state_path = self.home / "state.json"
        self.state = None
        self.changes = []
        self._write_owner = None

    def path(self, value, roots=READ_ROOTS, extensions={".md", ".json"}):
        rel = Path(value)
        if not rel.parts or rel.is_absolute() or ".." in rel.parts:
            raise CareerError("Use an allowlisted repository-relative path", "path_denied")
        path = (self.root / rel).resolve()
        if not path.is_relative_to(self.root) or (
            rel.parts[0] not in roots and str(rel) not in READ_FILES
        ):
            raise CareerError("Path outside the ledger allowlist", "path_denied")
        if path.suffix not in extensions:
            raise CareerError("Unsupported file type")
        return path

    def record_path(self, value, exists=True):
        path = self.path(value, {"applications"})
        records, _ = build(self.root)
        if exists and path.name not in {r["record"] for r in records}:
            raise CareerError(
                "Choose a canonical vacancy record, not a view, answer bank or report"
            )
        if path.parent != self.root / "applications" or path.suffix != ".md":
            raise CareerError("Vacancy records live directly under applications/")
        return path

    def read_path(self, value):
        """Private files win; only known generic aliases may point at toolkit assets."""
        from .workspace import GUIDES, skill_paths, source_file

        assets = ["docs/" + guide for guide in GUIDES] + skill_paths()
        if value in assets and Path(value).suffix in {".md", ".json"}:
            alias = self.root / value
            source = source_file(value)
            if not alias.exists() or alias.resolve() == source.resolve():
                return source
        return self.path(value)

    def load_state(self):
        if not self.state_path.exists():
            return {"schema_version": 1, "runs": {}, "attempts": {}, "events": []}
        state = json.loads(self.state_path.read_text())
        if state.get("schema_version") != 1:
            raise CareerError("Unsupported coordinator schema; do not reset its state")
        return state

    @contextmanager
    def locked(self):
        self.home.mkdir(parents=True, exist_ok=True)
        with (self.home / ".lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            self._write_owner = threading.get_ident()
            try:
                self.state = self.load_state()
                self.recover()
                self.changes = []
                yield
            finally:
                self._write_owner = None

    @contextmanager
    def reading(self, *, allow_pending=False):
        # A read inside our own write transaction must keep that snapshot and lock.
        if self._write_owner == threading.get_ident():
            yield
            return
        self.home.mkdir(parents=True, exist_ok=True)
        with (self.home / ".lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_SH)
            self.state = self.load_state()
            if self.state.get("pending") and not allow_pending:
                raise CareerError(
                    "Pending transaction: call rebuild_pipeline to recover the local update",
                    "pending_recovery",
                )
            yield

    def recover(self):
        pending = self.state.get("pending")
        if not pending:
            return
        for change in pending:
            path = self.path(change["path"], {"applications"})
            current = digest(path.read_bytes()) if path.exists() else None
            wanted = change["text"].encode()
            if current == digest(wanted):
                continue
            if current != change["before_sha256"]:
                raise CareerError(
                    "Interrupted update conflicts with a manual edit; reconcile pending transaction",
                    "recovery_conflict",
                )
            atomic_write(path, wanted)
        self.state.pop("pending")
        self.write_pipeline()
        atomic_write(self.state_path, encoded(self.state))

    def write_pipeline(self):
        records, markdown = build(self.root)
        atomic_write(self.root / "applications/pipeline.md", markdown.encode())
        atomic_write(
            self.root / "applications/pipeline.json",
            (
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "source": "canonical application Markdown records; generated, not live synced",
                        "records": records,
                    },
                    indent=2,
                )
                + "\n"
            ).encode(),
        )

    def commit(self):
        # The durable journal precedes record writes. Recovery finishes that exact update,
        # or stops if a human changed the record; it never repeats an external action.
        if self.changes:
            self.state["pending"] = self.changes
        atomic_write(self.state_path, encoded(self.state))
        if self.changes:
            self.recover()

    def change(self, path, text):
        if len(BLOCK.findall(text)) != 1:
            raise CareerError(
                "A vacancy update must preserve exactly one tracking block; escape quoted tracking markup",
                "invalid_arguments",
            )
        self.changes.append(
            {
                "path": str(path.relative_to(self.root)),
                "before_sha256": digest(path.read_bytes()) if path.exists() else None,
                "text": text,
            }
        )

    def run(self, run_id):
        if run_id not in self.state["runs"]:
            raise CareerError("Unknown run")
        run = self.state["runs"][run_id]
        if run["status"] != "running":
            raise CareerError("Run is finished; start a new bounded run")
        return run

    def evidence(self, items):
        for item in items:
            if item.startswith(("https://", "http://")):
                normal_url(item)
            elif not self.path(
                item, READ_ROOTS | {"output"}, {".md", ".json", ".pdf", ".jpg", ".png", ".txt"}
            ).is_file():
                raise CareerError(f"Missing evidence: {item}")

    def dispatch(self, name, args):
        if name not in SPECS:
            raise CareerError("Unknown career tool", "unknown_tool")
        validate(SPECS[name]["inputSchema"], args)
        method = getattr(self, name)
        if SPECS[name]["annotations"]["readOnlyHint"]:
            with self.reading(allow_pending=name == "read_record"):
                return method(**args)
        return self.mutate(name, args, lambda: method(**args))

    def mutate(self, name, args, action):
        """One local transaction; replay precedes fresh state validation."""
        with self.locked():
            key = args["idempotency_key"]
            request_hash = digest(encoded({"name": name, "args": args}))
            prior = next((e for e in self.state["events"] if e["idempotency_key"] == key), None)
            if prior:
                if prior["request_sha256"] != request_hash:
                    raise CareerError(
                        "Idempotency key reused with different input; inspect the prior action",
                        "idempotency_conflict",
                    )
                if name == "begin_submission":
                    return {
                        **prior["result"],
                        "replayed": True,
                        "status": self.state["attempts"][args["attempt_id"]]["status"],
                        "ready_for_single_submit": False,
                        "instruction": "This attempt already began. Inspect the external outcome; do not submit again.",
                    }
                if name == "rebuild_pipeline":
                    # Derived views may have become stale again since the first repair.
                    return self.rebuild_pipeline(**args)
                if name == "prepare_packet":
                    packet = prior["result"]["packet"]
                    sources = self.settings()["answer_sources"]
                    if set(sources) != set(packet["answer_source_sha256"]) or any(
                        digest(self.path(source).read_bytes())
                        != packet["answer_source_sha256"][source]
                        for source in sources
                    ):
                        raise CareerError(
                            "Canonical answers changed; cancel only an unstarted reservation and use a new preparation key",
                            "artifact_changed",
                        )
                    for field, source in [
                        ("cv", args["cv_path"]),
                        ("answers", args["answers_path"]),
                    ]:
                        current = self.path(
                            source, ARTIFACT_ROOTS, {".pdf", ".md", ".txt", ".json"}
                        ).read_bytes()
                        if digest(current) != prior["result"]["packet"][field]["sha256"]:
                            raise CareerError(
                                "Packet source bytes changed; cancel only an unstarted reservation and use a new preparation key",
                                "artifact_changed",
                            )
                return prior["result"]
            result = action()
            self.state["events"].append(
                {
                    "idempotency_key": key,
                    "request_sha256": request_hash,
                    "tool": name,
                    "at": now(),
                    "run_id": args.get("run_id"),
                    "result": result,
                }
            )
            self.commit()
            return result

    def list_operations(self):
        path = self.path("config/career-workflows.json")
        config = json.loads(
            (
                path if path.exists() else ROOT / "job_harness/defaults/career-workflows.json"
            ).read_text()
        )
        validate(WORKFLOW_SCHEMA, config, "workflow_configuration")
        return config

    def settings(self):
        settings = {
            "schema_version": 1,
            "timezone": "UTC",
            "primary_cv": None,
            "preferred_browser": None,
            "answer_sources": [
                "applications/approved-answers.md",
                "context/profile/application-decisions.md",
            ],
        }
        path = self.path("config/career-ledger.json")
        if path.exists():
            data = json.loads(path.read_text())
            validate(LEDGER_SCHEMA, data, "ledger_configuration")
            if (
                not isinstance(data, dict)
                or set(data) - set(settings)
                or data.get("schema_version") != 1
            ):
                raise CareerError("Invalid career-ledger configuration")
            settings.update(data)
        if not isinstance(settings["timezone"], str):
            raise CareerError("Configure an IANA timezone")
        if settings["timezone"] != "UTC":
            ZoneInfo(settings["timezone"])
        if settings["primary_cv"] is not None:
            if not isinstance(settings["primary_cv"], str):
                raise CareerError("Configure a repository-relative primary CV path")
            self.path(settings["primary_cv"], ARTIFACT_ROOTS, {".pdf"})
        if settings["preferred_browser"] is not None and not isinstance(
            settings["preferred_browser"], str
        ):
            raise CareerError("Browser preference must be a client-neutral string or null")
        if not isinstance(settings["answer_sources"], list) or not settings["answer_sources"]:
            raise CareerError("Configure at least one canonical answer source")
        for source in settings["answer_sources"]:
            if not isinstance(source, str):
                raise CareerError("Answer sources must be repository-relative text paths")
            self.path(source, {"context", "applications"}, {".md"})
        return settings

    def today(self):
        zone = self.settings()["timezone"]
        return datetime.now(timezone.utc if zone == "UTC" else ZoneInfo(zone)).date().isoformat()

    def plan_workflow(self, operation, scope=None):
        config = self.list_operations()
        if operation not in config["workflows"]:
            raise CareerError("Unknown workflow")
        return {
            "operation": operation,
            "scope": scope or {},
            **config["workflows"][operation],
            "shared_reads": list(
                dict.fromkeys(config["shared_reads"] + self.settings()["answer_sources"])
            ),
            "settings": self.settings(),
            "ledger": str(self.root),
            "authority": "User request controls scope. A plan or run is not external-action permission.",
        }

    def read_record(self, path):
        target = self.read_path(path)
        data = target.read_bytes()
        if len(data) > 1_000_000:
            raise CareerError("Record exceeds one MB; choose a narrower evidence record")
        return {
            "path": path,
            "origin": "ledger" if target.is_relative_to(self.root) else "toolkit",
            "sha256": digest(data),
            "text": data.decode(),
        }

    def find_application(self, query=None, stage=None):
        records, _ = build(self.root)
        words = query.casefold().split() if query else []
        found = []
        for record in records:
            if stage and record["stage"] != stage:
                continue
            path = self.root / "applications" / record["record"]
            text = path.read_text()
            if all(
                w in (record["title"] + " " + record["record"] + " " + text).casefold()
                for w in words
            ):
                found.append({**record, "path": str(path.relative_to(self.root))})
        return {
            "matches": found,
            "note": "Read complete matching records before acting; statuses are recorded, not live.",
        }

    def get_approved_answer(self, query):
        result = []
        for rel in self.settings()["answer_sources"]:
            text = self.path(rel).read_text()
            sections = re.split(r"(?=^## )", text, flags=re.M)
            for section in sections:
                if all(w in section.casefold() for w in query.casefold().split()):
                    result.append({"path": rel, "text": section})
        return {
            "matches": result,
            "note": "These are sourced answer bases; preserve scope and recheck changed circumstances.",
        }

    def check_duplicate(self, company, vacancy_url, posting_id=None, exclude_attempt=None):
        url = normal_url(vacancy_url)
        records, _ = build(self.root)
        same, exact = [], []
        for record in records:
            text = (self.root / "applications" / record["record"]).read_text()
            employer = record.get("company") or re.split(r"\s+[—–-]\s+", record["title"])[0]
            # Structured identity is authoritative, including parentheses and commas.
            try:
                urls = [normal_url(record["vacancy_url"])] if record.get("vacancy_url") else []
            except ValueError as error:
                raise CareerError(
                    f"{record['record']}: invalid vacancy_url; repair this record before duplicate checks"
                ) from error
            for candidate in re.findall(r'https?://[^\s<>\)\]"\,]+', text):
                try:
                    urls.append(normal_url(candidate.rstrip(".;")))
                except ValueError:
                    pass
            company_matches = company_key(company) == company_key(employer)
            identity_matches = url in urls or (
                posting_id
                and company_matches
                and (
                    posting_id == record.get("posting_id")
                    or posting_id.casefold() in text.casefold()
                    or posting_id.casefold() in record["record"].casefold()
                )
            )
            if company_matches or identity_matches:
                same.append(record)
            if identity_matches:
                exact.append(record)
        blocked_records = [r for r in exact if r["stage"] not in {"saved", "prepared"}]
        reservations = [
            a
            for a in self.state["attempts"].values()
            if a["id"] != exclude_attempt
            and (a["vacancy_url"] == url or company_key(a["company"]) == company_key(company))
            and a["status"] in {"reserved", "submitting", "outcome_unknown"}
        ]
        completed = [
            a
            for a in self.state["attempts"].values()
            if a["id"] != exclude_attempt and a["vacancy_url"] == url and a["status"] == "submitted"
        ]
        return {
            "blocked": bool(blocked_records or reservations or completed),
            "exact_matches": exact,
            "company_matches": same,
            "blocking_records": blocked_records,
            "blocking_attempts": reservations + completed,
            "note": "Read company records for aliases, limits and cooldowns. Unknown outcomes block retries; no automatic expiry.",
        }

    def start_run(self, operation, scope, user_request, idempotency_key):
        plan = self.plan_workflow(operation, scope)
        if operation == "apply" and (not scope.get("max_submissions") or not scope.get("sources")):
            raise CareerError(
                "Apply runs need an explicit positive submission cap and source scope"
            )
        run_id = str(uuid.uuid4())
        run = {
            "id": run_id,
            "operation": operation,
            "scope": scope,
            "user_request": user_request,
            "status": "running",
            "started_at": now(),
            "ledger_revision": self.revision(),
            "ledger_dirty": self.dirty(),
            "settings": self.settings(),
        }
        self.state["runs"][run_id] = run
        return {"run_id": run_id, "run": run, "plan": plan}

    def revision(self):
        p = subprocess.run(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], capture_output=True, text=True
        )
        return p.stdout.strip() if p.returncode == 0 else "unversioned"

    def dirty(self):
        p = subprocess.run(
            ["git", "-C", str(self.root), "status", "--porcelain"],
            capture_output=True,
            text=True,
        )
        return bool(p.stdout.strip()) if p.returncode == 0 else None

    def get_run(self, run_id):
        if run_id not in self.state["runs"]:
            raise CareerError("Unknown run")
        return {
            "run": self.state["runs"][run_id],
            "events": [
                e
                for e in self.state["events"]
                if e.get("run_id") == run_id or e["result"].get("run_id") == run_id
            ],
            "attempts": [a for a in self.state["attempts"].values() if a["run_id"] == run_id],
        }

    def list_events(self, idempotency_key=None, run_id=None, limit=100, offset=0):
        events = [
            e
            for e in self.state["events"]
            if (not idempotency_key or e["idempotency_key"] == idempotency_key)
            and (not run_id or e.get("run_id") == run_id or e["result"].get("run_id") == run_id)
        ]
        return {"events": events[offset : offset + limit], "total_matches": len(events)}

    def finish_run(self, run_id, status, summary, evidence, idempotency_key):
        run = self.run(run_id)
        self.evidence(evidence)
        active = [
            a
            for a in self.state["attempts"].values()
            if a["run_id"] == run_id
            and a["status"] in {"reserved", "submitting", "outcome_unknown"}
        ]
        if status == "completed" and active:
            raise CareerError("Cannot complete a run with unresolved submission attempts")
        run.update(status=status, finished_at=now(), summary=summary, evidence=evidence)
        return {"run_id": run_id, "status": status, "summary": summary, "evidence": evidence}

    def set_tracking(self, record, tracking, summary, evidence, event_key, correction_reason=None):
        path = self.record_path(record)
        text = path.read_text()
        match = BLOCK.search(text)
        old = (
            json.loads(match[1])
            if match
            else next(r for r in build(self.root)[0] if r["record"] == path.name)
        )
        if set(tracking) - TRACKING_KEYS:
            raise CareerError("Unknown tracking fields")
        merged = dict(old) if match else {k: v for k, v in old.items() if k in TRACKING_KEYS}
        merged.update(tracking)
        previous_booking = None
        if (
            "schedule_state" in tracking
            and tracking["schedule_state"] != "booked"
            and "scheduled_start" not in tracking
        ):
            previous_booking = merged.pop("scheduled_start", None)
        if tracking.get("schedule_state") in {
            "booked",
            "completed",
            "awaiting-outcome",
            "unbooked",
        }:
            merged.pop("proposed_time", None)
        if (
            not {"stage", "checked", "next_action"} <= merged.keys()
            or merged["stage"] not in STAGES
        ):
            raise CareerError("Tracking needs stage, checked and next_action")
        if merged.get("vacancy_url"):
            normal_url(merged["vacancy_url"])
        previous, stage = old["stage"], merged["stage"]
        ladder = STAGES[:8]
        regression = (previous == "closed" and stage != "closed") or (
            previous in ladder and stage in ladder and ladder.index(stage) < ladder.index(previous)
        )
        if regression and not correction_reason:
            raise CareerError(
                "Older evidence cannot regress a stage; provide an explicit sourced correction reason"
            )
        try:
            datetime.fromisoformat(merged["checked"])
        except ValueError as error:
            raise CareerError("tracking.checked: ISO date required", "invalid_arguments") from error
        if merged.get("schedule_state") and merged["schedule_state"] not in SCHEDULES:
            raise CareerError("Invalid schedule state")
        if merged.get("scheduled_start"):
            try:
                scheduled = datetime.fromisoformat(merged["scheduled_start"])
            except ValueError as error:
                raise CareerError(
                    "tracking.scheduled_start: ISO datetime required", "invalid_arguments"
                ) from error
            if merged.get("schedule_state") != "booked" or scheduled.tzinfo is None:
                raise CareerError(
                    "Booked time needs an explicit timezone and booked state", "invalid_arguments"
                )
        for field in ["brief", "record_evidence"]:
            if merged.get(field) and not self.path("applications/" + merged[field]).is_file():
                raise CareerError(f"Missing {field}")
        block = "<!-- tracking\n" + json.dumps(merged, indent=2, ensure_ascii=False) + "\n-->"
        if match:
            text = text[: match.start()] + block + text[match.end() :]
        else:
            first, rest = text.split("\n", 1)
            text = first + "\n\n" + block + "\n" + rest
        event_id = digest(event_key.encode())[:16]
        links = ", ".join(evidence)
        text += f"\n## Recorded update — {merged['checked']}\n\n{summary}\n\nEvidence: {links}\n"
        if previous_booking:
            text += f"\nPrevious booked time: {previous_booking}. Current schedule state: {merged['schedule_state']}.\n"
        if correction_reason:
            text += f"\nCorrection reason: {correction_reason}\n"
        text += f"\n<!-- career-event:{event_id} -->\n"
        self.change(path, text)
        return merged

    def record_event(
        self,
        run_id,
        kind,
        summary,
        evidence,
        idempotency_key,
        record=None,
        tracking=None,
        correction_reason=None,
    ):
        self.run(run_id)
        self.evidence(evidence)
        if tracking and not record:
            raise CareerError("A tracking update needs a canonical record")
        result = {"run_id": run_id, "kind": kind, "summary": summary, "evidence": evidence}
        if record:
            self.record_path(record)
            result["record"] = record
        if tracking:
            result["tracking"] = self.set_tracking(
                record, tracking, summary, evidence, idempotency_key, correction_reason
            )
        return result

    def update_application(
        self, run_id, record, tracking, summary, evidence, idempotency_key, correction_reason=None
    ):
        return self.record_event(
            run_id,
            "status_updated",
            summary,
            evidence,
            idempotency_key,
            record,
            tracking,
            correction_reason,
        )

    def create_application(
        self,
        run_id,
        record,
        company,
        role,
        vacancy_url,
        source,
        fit_reason,
        evidence,
        idempotency_key,
        posting_id=None,
    ):
        self.run(run_id)
        self.evidence(evidence)
        target = self.record_path(record, exists=False)
        if (
            target.exists()
            or not re.fullmatch(r"[a-z0-9][a-z0-9-]+\.md", target.name)
            or target.name in RESERVED_NAMES
        ):
            raise CareerError("Use a new lowercase company-role-posting filename")
        duplicates = self.check_duplicate(company, vacancy_url, posting_id)
        if duplicates["exact_matches"] or duplicates["blocking_attempts"]:
            raise CareerError("Vacancy already recorded; update its existing record")
        tracking = {
            "stage": "saved",
            "checked": self.today(),
            "next_action": "Review fit and prepare application",
            "company": company,
            "vacancy_url": vacancy_url,
            "source": source,
            "posting_id": posting_id,
        }
        text = f"# {company} — {role}\n\n<!-- tracking\n{json.dumps(tracking, indent=2, ensure_ascii=False)}\n-->\n\n"
        text += f"## Vacancy and fit\n\nSource URL: {vacancy_url}\n\n{fit_reason}\n\nEvidence: {', '.join(evidence)}\n"
        self.change(target, text)
        return {"run_id": run_id, "record": record, "tracking": tracking}

    def copy_artifact(self, source, folder):
        path = self.path(source, ARTIFACT_ROOTS, {".pdf", ".md", ".txt", ".json", ".jpg", ".png"})
        data = path.read_bytes()
        if len(data) > 30_000_000:
            raise CareerError("Artifact exceeds 30 MB")
        sha = digest(data)
        target = self.home / folder / (sha + path.suffix)
        if target.exists() and target.read_bytes() != data:
            raise CareerError("Immutable artifact hash mismatch")
        if not target.exists():
            atomic_write(target, data)
        return {"path": str(target.relative_to(self.root)), "sha256": sha, "original_path": source}

    def save_receipt(self, run_id, path, idempotency_key):
        self.run(run_id)
        return {"run_id": run_id, "artifact": self.copy_artifact(path, "receipts")}

    def prepare_packet(
        self,
        run_id,
        record,
        company,
        vacancy_url,
        source,
        cv_path,
        answers_path,
        idempotency_key,
        posting_id=None,
    ):
        run = self.run(run_id)
        if run["operation"] != "apply":
            raise CareerError("Submission packets require an apply run")
        self.record_path(record)
        if run["scope"].get("record") and record != run["scope"]["record"]:
            raise CareerError("Packet record outside this run")
        if run["scope"].get("company") and company_key(company) != company_key(
            run["scope"]["company"]
        ):
            raise CareerError("Packet company outside this run")
        if source not in run["scope"].get("sources", []):
            raise CareerError("Packet source outside this run")
        duplicate = self.check_duplicate(company, vacancy_url, posting_id)
        bound = [r for r in duplicate["exact_matches"] if r["record"] == Path(record).name]
        if not bound:
            raise CareerError(
                "Packet vacancy identity does not match its canonical record; record the identity first"
            )
        employer = bound[0].get("company") or re.split(r"\s+[—–-]\s+", bound[0]["title"])[0]
        if company_key(company) != company_key(employer):
            raise CareerError("Packet company does not match its canonical record")
        if duplicate["blocked"]:
            raise CareerError(
                "Duplicate or uncertain prior submission; reconcile it first", "duplicate_blocked"
            )
        cv = self.copy_artifact(cv_path, "packets")
        answers = self.copy_artifact(answers_path, "packets")
        if Path(cv_path).suffix != ".pdf" or Path(answers_path).suffix not in {
            ".md",
            ".txt",
            ".json",
        }:
            raise CareerError("Packet needs a PDF and an exact answer/message file")
        packet = {
            "record": record,
            "company": company,
            "vacancy_url": normal_url(vacancy_url),
            "source": source,
            "posting_id": posting_id,
            "cv": cv,
            "answers": answers,
            "source_record": self.copy_artifact(record, "packets"),
            "answer_source_sha256": {
                rel: digest(self.path(rel).read_bytes())
                for rel in self.settings()["answer_sources"]
            },
            "ledger_revision": self.revision(),
            "ledger_dirty": self.dirty(),
        }
        packet_id = digest(encoded(packet))
        atomic_write(self.home / "packets" / (packet_id + ".json"), encoded(packet))
        return {
            "run_id": run_id,
            "packet_id": packet_id,
            "packet": packet,
            "note": "Use these frozen artifacts; the model must still verify facts, fit and external-action authority.",
        }

    def packet(self, packet_id):
        if not re.fullmatch(r"[a-f0-9]{64}", packet_id):
            raise CareerError("Invalid packet ID")
        path = self.home / "packets" / (packet_id + ".json")
        packet = json.loads(path.read_text())
        if digest(encoded(packet)) != packet_id:
            raise CareerError("Packet was changed after preparation", "artifact_changed")
        for key in ["cv", "answers", "source_record"]:
            item = packet[key]
            if (
                digest(
                    self.path(
                        item["path"], ARTIFACT_ROOTS, {".pdf", ".md", ".txt", ".json"}
                    ).read_bytes()
                )
                != item["sha256"]
            ):
                raise CareerError(
                    "Submitted artifact was changed after preparation", "artifact_changed"
                )
        return packet

    def reserve_submission(self, run_id, packet_id, idempotency_key):
        run = self.run(run_id)
        packet = self.packet(packet_id)
        if run["operation"] != "apply" or run["scope"].get("dry_run", True):
            raise CareerError("Dry runs cannot reserve a real submission")
        if packet["source"] not in run["scope"]["sources"]:
            raise CareerError("Packet source outside this run")
        if run["scope"].get("record") and packet["record"] != run["scope"]["record"]:
            raise CareerError("Packet record outside this run")
        if run["scope"].get("company") and company_key(packet["company"]) != company_key(
            run["scope"]["company"]
        ):
            raise CareerError("Packet company outside this run")
        attempts = sum(a["run_id"] == run_id for a in self.state["attempts"].values())
        if attempts >= run["scope"].get("max_attempts", run["scope"]["max_submissions"] * 3):
            raise CareerError(
                "Total attempt cap reached, including negative/cancelled attempts", "cap_reached"
            )
        used = sum(
            a["run_id"] == run_id and a["status"] != "cancelled"
            for a in self.state["attempts"].values()
        )
        if used >= run["scope"]["max_submissions"]:
            raise CareerError("Submission cap reached", "cap_reached")
        if self.check_duplicate(packet["company"], packet["vacancy_url"], packet["posting_id"])[
            "blocked"
        ]:
            raise CareerError(
                "Submission already reserved, submitted or uncertain", "duplicate_blocked"
            )
        attempt = {
            "id": str(uuid.uuid4()),
            "run_id": run_id,
            "packet_id": packet_id,
            **packet,
            "status": "reserved",
            "reserved_at": now(),
        }
        self.state["attempts"][attempt["id"]] = attempt
        return {"run_id": run_id, "attempt": attempt}

    def begin_submission(self, attempt_id, idempotency_key):
        attempt = self.state["attempts"].get(attempt_id)
        if not attempt or attempt["status"] != "reserved":
            raise CareerError(
                "Only a reserved attempt can begin; never repeat a submitting/unknown attempt",
                "submit_replay_blocked",
            )
        self.run(attempt["run_id"])
        packet = self.packet(attempt["packet_id"])
        sources = self.settings()["answer_sources"]
        if set(sources) != set(packet["answer_source_sha256"]) or any(
            digest(self.path(source).read_bytes()) != packet["answer_source_sha256"][source]
            for source in sources
        ):
            raise CareerError(
                "Canonical answers changed; cancel the unstarted reservation, recheck answers and prepare a new packet",
                "answers_changed",
            )
        if self.check_duplicate(
            packet["company"], packet["vacancy_url"], packet["posting_id"], attempt_id
        )["blocked"]:
            raise CareerError("A later ledger update now blocks submission")
        attempt.update(status="submitting", began_at=now())
        return {
            "run_id": attempt["run_id"],
            "attempt_id": attempt_id,
            "status": "submitting",
            "ready_for_single_submit": True,
            "packet": packet,
            "instruction": "One external submit only. On interruption, inspect outcome before retrying.",
        }

    def finish_submission(
        self, attempt_id, outcome, summary, evidence, idempotency_key, receipt_path=None
    ):
        attempt = self.state["attempts"].get(attempt_id)
        if not attempt or attempt["status"] not in {"submitting", "outcome_unknown"}:
            raise CareerError("Only submitting/unknown attempts can be resolved")
        self.evidence(evidence)
        if outcome == "submitted" and not evidence:
            raise CareerError("Confirmed submission needs receipt evidence")
        if outcome == "not_submitted" and not evidence:
            raise CareerError("Negative resolution needs evidence, not elapsed time")
        current = next(
            (r for r in build(self.root)[0] if r["record"] == Path(attempt["record"]).name), None
        )
        if current is None:
            raise CareerError(
                "Missing canonical record "
                + attempt["record"]
                + "; restore or reconcile this exact linked record before resolving the attempt",
                "missing_resource",
            )
        receipt = self.copy_artifact(receipt_path, "receipts") if receipt_path else None
        attempt.update(
            status="cancelled" if outcome == "not_submitted" else outcome,
            resolved_at=now(),
            summary=summary,
            evidence=evidence,
            receipt=receipt,
        )
        stage = {
            "submitted": "applied",
            "outcome_unknown": "needs-review",
            "not_submitted": "prepared",
        }[outcome]
        packet_path = "applications/operations/packets/" + attempt["packet_id"] + ".json"
        tracking = {
            "stage": stage,
            "checked": self.today(),
            "status_detail": summary,
            "next_action": "Monitor for employer response"
            if outcome == "submitted"
            else "Resolve submission outcome before retrying"
            if outcome == "outcome_unknown"
            else "Prepare a new verified attempt",
            "company": attempt["company"],
            "vacancy_url": attempt["vacancy_url"],
            "posting_id": attempt["posting_id"],
            "source": attempt["source"],
            "record_evidence": "operations/packets/" + attempt["packet_id"] + ".json",
        }
        ladder = STAGES[:8]
        stronger = current["stage"] == "closed" or (
            current["stage"] in ladder
            and (
                ladder.index(current["stage"]) > ladder.index(stage)
                if stage in ladder
                else ladder.index(current["stage"]) > ladder.index("prepared")
            )
        )
        if stronger:
            # Resolve the attempt without replacing a newer interview or outcome.
            for field in ["stage", "next_action", "status_detail"]:
                if field in current:
                    tracking[field] = current[field]
        links = [packet_path] + evidence + ([receipt["path"]] if receipt else [])
        self.set_tracking(attempt["record"], tracking, summary, links, idempotency_key)
        return {
            "run_id": attempt["run_id"],
            "attempt_id": attempt_id,
            "status": attempt["status"],
            "record": attempt["record"],
            "evidence": links,
        }

    def cancel_reservation(self, attempt_id, summary, idempotency_key):
        attempt = self.state["attempts"].get(attempt_id)
        if not attempt or attempt["status"] != "reserved":
            raise CareerError(
                "Only an unstarted reservation can be cancelled without outcome evidence"
            )
        attempt.update(status="cancelled", summary=summary, resolved_at=now())
        return {
            "run_id": attempt["run_id"],
            "attempt_id": attempt_id,
            "status": "cancelled",
            "summary": summary,
        }

    def reconcile_tracker(self):
        records, markdown = build(self.root)
        pipeline = self.root / "applications/pipeline.md"
        json_view = self.root / "applications/pipeline.json"
        expected_json = {
            "schemaVersion": 1,
            "source": "canonical application Markdown records; generated, not live synced",
            "records": records,
        }
        return {
            "records": len(records),
            "legacy_records": sum(r["tracking_source"] == "legacy-prose" for r in records),
            "needs_review": [r for r in records if r["stage"] == "needs-review"],
            "pipeline_stale": not pipeline.exists()
            or pipeline.read_text() != markdown
            or not json_view.exists()
            or json_view.read_text() != json.dumps(expected_json, indent=2) + "\n",
            "unfinished_runs": [r for r in self.state["runs"].values() if r["status"] == "running"],
            "unresolved_attempts": [
                a
                for a in self.state["attempts"].values()
                if a["status"] in {"reserved", "submitting", "outcome_unknown"}
            ],
            "note": "Report only; no mailbox or board synchronization.",
        }

    def rebuild_pipeline(self, idempotency_key):
        self.write_pipeline()
        return {"records": len(build(self.root)[0]), "pipeline": "applications/pipeline.md"}


def obj(properties, required=None):
    return {
        "type": "object",
        "properties": properties,
        "required": list(properties) if required is None else required,
        "additionalProperties": False,
    }


S = {"type": "string", "minLength": 1, "maxLength": 20000}
A = {"type": "array", "items": S}
E = {"type": "array", "items": S, "minItems": 1}
SCOPE = obj(
    {
        "sources": A,
        "max_submissions": {"type": "integer", "minimum": 1, "maximum": 1000},
        "max_attempts": {"type": "integer", "minimum": 1, "maximum": 3000},
        "max_roles": {"type": "integer", "minimum": 1, "maximum": 1000},
        "dry_run": {"type": "boolean"},
        "since": S,
        "until": S,
        "query": S,
        "company": S,
        "record": S,
    },
    [],
)
TRACK = obj(
    {
        **{k: S for k in TRACKING_KEYS},
        "stage": {"type": "string", "enum": STAGES},
        "schedule_state": {"type": "string", "enum": SCHEDULES},
    },
    [],
)


def spec(description, properties, required=None, readonly=True):
    return {
        "description": description,
        "inputSchema": obj(properties, required),
        "annotations": {
            "readOnlyHint": readonly,
            "destructiveHint": False,
            "idempotentHint": True,
            "openWorldHint": False,
        },
    }


SPECS = {
    "list_operations": spec(
        "List the eight career workflows and their skills; no external actions.", {}
    ),
    "plan_workflow": spec(
        "Get a scoped workflow and required shared records before working.",
        {
            "operation": {
                "type": "string",
                "enum": [
                    "review_inbox",
                    "tidy_inbox",
                    "discover",
                    "apply",
                    "schedule",
                    "prepare_interview",
                    "update_knowledge",
                    "reconcile",
                ],
            },
            "scope": SCOPE,
        },
        ["operation"],
    ),
    "read_record": spec(
        "Read complete allowlisted ledger text with its hash; no arbitrary filesystem access.",
        {"path": S},
    ),
    "find_application": spec(
        "List/filter paged vacancy history by stage or optional company/role/URL/posting text. Read complete matching records before acting.",
        {"query": S, "stage": {"type": "string", "enum": STAGES}},
        [],
    ),
    "get_approved_answer": spec(
        "Retrieve sourced answer sections; preserve their attribution and limits.", {"query": S}
    ),
    "check_duplicate": spec(
        "Check exact vacancy history and outstanding company reservations. Read returned company records for limits.",
        {"company": S, "vacancy_url": S, "posting_id": S},
        ["company", "vacancy_url"],
    ),
    "start_run": spec(
        "Start a logged bounded workflow. This records user intent, not a permission grant.",
        {
            "operation": {
                "type": "string",
                "enum": [
                    "review_inbox",
                    "tidy_inbox",
                    "discover",
                    "apply",
                    "schedule",
                    "prepare_interview",
                    "update_knowledge",
                    "reconcile",
                ],
            },
            "scope": SCOPE,
            "user_request": S,
            "idempotency_key": S,
        },
        readonly=False,
    ),
    "get_run": spec(
        "Read a run, its events and submission attempts for recovery or handoff.", {"run_id": S}
    ),
    "list_events": spec(
        "Find processed action IDs across runs or read recent audit events before repeating work.",
        {
            "idempotency_key": S,
            "run_id": S,
            "offset": {"type": "integer", "minimum": 0},
            "limit": {"type": "integer", "minimum": 1, "maximum": 1000},
        },
        [],
    ),
    "finish_run": spec(
        "Finish with a concise digest and evidence; unresolved submissions cannot be completed.",
        {
            "run_id": S,
            "status": {"type": "string", "enum": ["completed", "partial", "blocked"]},
            "summary": S,
            "evidence": A,
            "idempotency_key": S,
        },
        readonly=False,
    ),
    "record_event": spec(
        "Log a verified mail/archive/reply/booking/knowledge event and optionally update a canonical vacancy. Deduplicate with source ID.",
        {
            "run_id": S,
            "kind": {"type": "string", "enum": KINDS},
            "summary": S,
            "evidence": E,
            "idempotency_key": S,
            "record": S,
            "tracking": TRACK,
            "correction_reason": S,
        },
        ["run_id", "kind", "summary", "evidence", "idempotency_key"],
        False,
    ),
    "update_application": spec(
        "Update the canonical tracking block with a dated event, evidence and automatic pipeline rebuild.",
        {
            "run_id": S,
            "record": S,
            "tracking": TRACK,
            "summary": S,
            "evidence": E,
            "idempotency_key": S,
            "correction_reason": S,
        },
        ["run_id", "record", "tracking", "summary", "evidence", "idempotency_key"],
        False,
    ),
    "create_application": spec(
        "Record a newly discovered vacancy as saved with attributed fit evidence.",
        {
            "run_id": S,
            "record": S,
            "company": S,
            "role": S,
            "vacancy_url": S,
            "source": S,
            "fit_reason": S,
            "evidence": E,
            "idempotency_key": S,
            "posting_id": S,
        },
        [
            "run_id",
            "record",
            "company",
            "role",
            "vacancy_url",
            "source",
            "fit_reason",
            "evidence",
            "idempotency_key",
        ],
        False,
    ),
    "save_receipt": spec(
        "Preserve exact receipt bytes in private content-addressed storage.",
        {"run_id": S, "path": S, "idempotency_key": S},
        readonly=False,
    ),
    "prepare_packet": spec(
        "Freeze the exact CV PDF and answers/message, vacancy identity and source revision before reserving.",
        {
            "run_id": S,
            "record": S,
            "company": S,
            "vacancy_url": S,
            "source": S,
            "cv_path": S,
            "answers_path": S,
            "posting_id": S,
            "idempotency_key": S,
        },
        [
            "run_id",
            "record",
            "company",
            "vacancy_url",
            "source",
            "cv_path",
            "answers_path",
            "idempotency_key",
        ],
        False,
    ),
    "reserve_submission": spec(
        "Atomically reserve a vacancy and company for a bounded non-dry apply run; never expires uncertain attempts.",
        {"run_id": S, "packet_id": S, "idempotency_key": S},
        readonly=False,
    ),
    "begin_submission": spec(
        "Mark one attempt submitting immediately before the external Submit. A crash now requires outcome reconciliation.",
        {"attempt_id": S, "idempotency_key": S},
        readonly=False,
    ),
    "finish_submission": spec(
        "Save confirmed/unknown/negative outcome and advance the record only with evidence. Unknown stays blocked.",
        {
            "attempt_id": S,
            "outcome": {
                "type": "string",
                "enum": ["submitted", "outcome_unknown", "not_submitted"],
            },
            "summary": S,
            "evidence": A,
            "receipt_path": S,
            "idempotency_key": S,
        },
        ["attempt_id", "outcome", "summary", "evidence", "idempotency_key"],
        False,
    ),
    "cancel_reservation": spec(
        "Cancel only a reservation that never began external submission.",
        {"attempt_id": S, "summary": S, "idempotency_key": S},
        readonly=False,
    ),
    "reconcile_tracker": spec(
        "Report legacy/uncertain records, stale pipeline and unfinished runs; changes need evidence.",
        {},
    ),
    "rebuild_pipeline": spec(
        "Recover pending local record writes and regenerate the pipeline under the shared lock.",
        {"idempotency_key": S},
        readonly=False,
    ),
}
