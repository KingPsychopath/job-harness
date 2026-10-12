"""Build a private process view from canonical vacancy records; no network or sync."""

import json
import re
from collections import Counter
from datetime import datetime
from pathlib import Path

from .errors import CareerError

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "applications"
STAGES = [
    "saved",
    "prepared",
    "applied",
    "introduction",
    "screening",
    "technical",
    "final",
    "offer",
    "closed",
    "needs-review",
]
BLOCK = re.compile(r"<!-- tracking\n(.*?)\n-->", re.S)


RESERVED_NAMES = {
    "README.md",
    "shortlist.md",
    "template.md",
    "pipeline.md",
    "approved-answers.md",
    "platform-profiles.md",
}


def is_vacancy_filename(name):
    """Legacy-only discovery filter; structured blocks are authoritative."""
    return not (
        name in RESERVED_NAMES
        or re.match(r"\d{4}-", name)
        or name.startswith(("email-", "search-", "rejection-review-"))
        or re.search(r"-answers(?:-|\.)", name)
    )


def classify(text):
    text = text.replace("**", "")
    current = re.findall(r"^\s*(?:- )?Current status[^:]*:\s*(.+)$", text, re.M | re.I)
    stage = re.findall(r"^\s*(?:- )?(?:Stage|Status):\s*(.+)$", text, re.M | re.I)
    if not stage:
        stage = re.findall(r"^- ((?:Submitted|Applied) \d.*)$", text, re.M)
    value = current[-1] if current else stage[0] if stage else ""
    if re.search(r"^- Outcome: rejection\.", text, re.M) or re.search(
        r"conversation shows .*declined the request", text, re.I
    ):
        return "closed", value or "Explicit rejection outcome"
    v = value.lower()
    if "unassigned rejection" in v:
        return "needs-review", value
    if any(x in v for x in ["rejected", "closed", "unavailable", "withdrawn"]):
        return "closed", value
    if any(x in v for x in ["unverified", "unknown", "undefined", "incomplete"]):
        return "needs-review", value
    if "technical interview" in v:
        return "technical", value
    if any(x in v for x in ["invited", "introductory", "introduced", "interest"]):
        return "introduction", value
    if any(x in v for x in ["submitted", "applied", "application/message sent"]):
        return "applied", value
    if any(x in v for x in ["prepared", "preparing", "drafted", "blocked"]):
        return "prepared", value
    if any(x in v for x in ["researching", "saved"]):
        return "saved", value
    return "needs-review", value or "No structured stage yet"


def build(root=ROOT):
    root = Path(root).resolve()
    app = root / "applications"
    records = []
    for path in sorted(app.glob("*.md")):
        if not path.resolve().is_relative_to(root):
            raise CareerError(f"{path.name}: vacancy record escapes the ledger")
        text = path.read_text()
        title = re.search(r"^# (.+)$", text, re.M)
        matches = BLOCK.findall(text)
        if not matches and re.search(r"<!--\s*tracking\b(?![ \t-]+example\b)", text, re.I):
            raise CareerError(f"{path.name}: malformed tracking block")
        if matches and path.name in RESERVED_NAMES:
            raise CareerError(f"{path.name}: reserved non-vacancy file contains a tracking block")
        if matches and not title:
            raise CareerError(f"{path.name}: tracking block without '# ' title")
        if not matches and (not is_vacancy_filename(path.name) or not title):
            continue
        if len(matches) > 1:
            raise CareerError(f"{path.name}: duplicate tracking block")
        if matches:
            try:
                data = json.loads(matches[0])
            except ValueError as error:
                raise CareerError(f"{path.name}: invalid tracking JSON") from error
            if not isinstance(data, dict):
                raise CareerError(f"{path.name}: tracking must be an object")
            for key, value in data.items():
                if not isinstance(value, str) and not (key == "posting_id" and value is None):
                    raise CareerError(
                        f"{path.name}: {key} must be a string"
                        + (" or null" if key == "posting_id" else "")
                    )
            if (
                data.get("stage") not in STAGES
                or not data.get("next_action")
                or not data.get("checked")
            ):
                raise CareerError(f"{path.name}: missing/invalid tracking fields")
            try:
                datetime.fromisoformat(data["checked"])
                if data.get("scheduled_start"):
                    datetime.fromisoformat(data["scheduled_start"])
            except ValueError as error:
                raise CareerError(f"{path.name}: invalid tracking date") from error
            for key in ["brief", "record_evidence"]:
                if data.get(key):
                    linked = (app / data[key]).resolve()
                    if not linked.is_relative_to(root) or not linked.is_file():
                        raise CareerError(f"{path.name}: missing/out-of-scope {key}")
            if data.get("schedule_state") and data["schedule_state"] not in {
                "unbooked",
                "proposed",
                "availability-sent",
                "booked",
                "completed",
                "awaiting-outcome",
                "handled-unconfirmed",
            }:
                raise CareerError(f"{path.name}: invalid schedule state")
            if data.get("scheduled_start"):
                when = datetime.fromisoformat(data["scheduled_start"])
                if when.tzinfo is None or data.get("schedule_state") != "booked":
                    raise CareerError(
                        f"{path.name}: scheduled time needs timezone and booked state"
                    )
            data["tracking_source"] = "structured"
        else:
            stage, summary = classify(text)
            data = {
                "stage": stage,
                "status_detail": summary,
                "next_action": "Read latest record before acting; legacy prose is not a live status check.",
                "tracking_source": "legacy-prose",
            }
        records.append(
            {
                "record": path.name,
                "title": title.group(1),
                **{k: v for k, v in data.items() if k not in {"record", "title"}},
            }
        )
    counts = Counter(r["stage"] for r in records)
    lines = [
        "# Application pipeline",
        "",
        "Generated from canonical vacancy records. Update the record, then call Job Harness `rebuild_pipeline`; do not edit this view. No live board, email or calendar sync.",
        "",
        "Structured entries retain their attributed review dates. Legacy prose can be stale; use the linked record before taking action. Applied includes direct requests/profile sharing, not only ATS receipts. Proposed times are never booked times.",
        "",
        "| Stage | Records |",
        "|---|---:|",
    ]
    lines += [f"| {s} | {counts[s]} |" for s in STAGES]
    lines += [
        "",
        "## Active conversations and interviews",
        "",
        "| Role | Stage / scheduling | Next action | Booking route | Brief |",
        "|---|---|---|---|---|",
    ]
    active = [
        r
        for r in records
        if r["tracking_source"] == "structured"
        and r["stage"] in {"introduction", "screening", "technical", "final", "offer"}
    ]
    for r in active:
        scheduling = (
            r.get("scheduled_start") or r.get("proposed_time") or r.get("schedule_state", "unknown")
        )
        brief = f"[Prep]({r['brief']})" if r.get("brief") else "Needed"
        booking = (
            f"[Employer calendar]({r['booking_url']})"
            if r.get("booking_url")
            else "Email / no employer link saved"
        )
        lines.append(
            f"| [{r['title']}]({r['record']}) | {r['stage']}: {scheduling} | {r['next_action']} | {booking} | {brief} |"
        )
    for s in STAGES:
        subset = [r for r in records if r["stage"] == s]
        if subset:
            lines += ["", f"## {s.capitalize()}", ""]
            for r in subset:
                detail = r.get("status_detail") or r.get("schedule_state") or "See record"
                lines.append(f"- [{r['title']}]({r['record']}) — {detail}")
    lines += [
        "",
        "## Maintain and prepare",
        "",
        "Use the [tracking and briefing workflow](../docs/application-pipeline.md). Interview notes remain under `context/interviews/`; company briefs are coaching under `applications/briefs/`. Neither changes career facts or the CV.",
        "",
    ]
    return records, "\n".join(lines)
