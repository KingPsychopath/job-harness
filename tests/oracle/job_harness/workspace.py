"""Generic package resources and separate private workspace onboarding."""

import json
import tempfile
from pathlib import Path

from .ledger import ROOT
RESOURCES = Path(__file__).resolve().parents[3] / "resources"

GUIDES = ["career-operations.md", "job-harness-contract.md", "job-harness-onboarding.md"]
SKILLS = [
    "career-ops",
    "career-apply",
    "career-discover",
    "career-inbox-tidy",
    "career-interview-prep",
    "career-knowledge-update",
    "career-reconcile",
    "career-schedule",
    "application-email-review",
]


def skill_paths():
    paths = []
    for skill in SKILLS:
        folder = RESOURCES / "skills" / skill
        for path in sorted(folder.rglob("*")):
            if path.is_file() and "__pycache__" not in path.parts:
                paths.append(".agents/skills/" + str(path.relative_to(folder.parent)))
    return paths


def source_file(name):
    # Stable logical paths resolve only to generic, wheel-bundled resources.
    if name not in skill_paths() + ["docs/" + guide for guide in GUIDES]:
        raise ValueError("Unknown generic resource alias: " + str(name))
    if name.startswith(".agents/skills/"):
        name = "skills/" + name.removeprefix(".agents/skills/")
    elif name in ["docs/" + guide for guide in GUIDES]:
        name = "guides/" + name.removeprefix("docs/")
    source = RESOURCES / name
    ancestors = [source, *source.parents]
    if any(p.is_symlink() for p in ancestors if p != RESOURCES and RESOURCES in p.parents):
        raise ValueError("Toolkit refuses symlinked sources: " + name)
    if not source.resolve().is_relative_to(RESOURCES) or not source.is_file():
        raise ValueError("Missing/out-of-toolkit source: " + name)
    return source


def initialize(root):
    from .bundles import assert_external_workspace

    root = Path(root).resolve()
    assert_external_workspace(root)
    if root.exists() and any(root.iterdir()):
        raise ValueError("Initialize only a new/empty ledger; existing data is never replaced.")
    root.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=root.parent, prefix=".job-harness-") as temporary:
        data = _initialize(Path(temporary))
        Path(temporary).replace(root)
    return {**data, "ledger": str(root)}


def _initialize(root):
    root = Path(root).resolve()
    if root.exists() and any(root.iterdir()):
        raise ValueError("Initialize only a new/empty ledger; existing data is never replaced.")
    root.mkdir(parents=True, exist_ok=True)
    for name in ["applications/evidence", "context/profile", "content", "output", "config", "docs"]:
        (root / name).mkdir(parents=True, exist_ok=True)
    (root / "config/career-ledger.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "timezone": "UTC",
                "primary_cv": None,
                "preferred_browser": None,
                "answer_sources": [
                    "applications/approved-answers.md",
                    "context/profile/application-decisions.md",
                ],
            },
            indent=2,
        )
        + "\n"
    )
    files = {
        "AGENTS.md": "# Candidate ledger\n\nRead approved answers and attributed records before acting. Do not invent facts. User instructions control external scope. Record actual outcomes with evidence; never replay a submitting or unknown attempt. Do not store credentials.\n",
        "README.md": "# Private candidate ledger\n\nConfigure config/career-ledger.json, supply the primary CV and attributed context, then run Job Harness bootstrap. This directory contains personal data; keep it separate from the reusable toolkit.\n",
        "applications/approved-answers.md": "# Approved answers\n\nRecord each direct answer with date, source, question scope and limits. No demographic, salary, legal or health answer is assumed.\n",
        "context/profile/application-decisions.md": "# Application decisions\n\nRecord preferences with attribution and unresolved limits. Chat memory is not canonical.\n",
        "context/README.md": "# Career evidence\n\nKeep firsthand facts, recruiter feedback, coaching and hypotheses distinct. Cite source and date; preserve unknowns.\n",
        "docs/project.md": "# Career ledger\n\nOne canonical private source of facts, applications and attributable outcomes.\n",
        "docs/workflow.md": "# Evidence workflow\n\nUpdate attributed facts under context; vacancies and outcomes under applications. Baseline CV changes need candidate authority.\n\n## After an interview\nSave the dated note, link the vacancy, and keep recollections and coaching distinct from established facts.\n",
        "docs/application-pipeline.md": "# Tracking and preparation\n\nEdit canonical vacancy records, then rebuild the pipeline. Keep proposed, booked, completed and unknown separate. Briefs are coaching, never career facts.\n",
        "applications/briefs/template.md": "# Company brief\n\nBusiness, vacancy, sourced personal stories, opening, questions and unresolved facts.\n",
        ".gitignore": "applications/operations/.lock\napplications/operations/.career-tmp-*\n.scratch/\n__pycache__/\n",
    }
    for name, text in files.items():
        p = root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)
    from .ledger import Ledger

    Ledger(root).dispatch("rebuild_pipeline", {"idempotency_key": "onboarding:initial-pipeline"})
    return {
        "ledger": str(root),
        "configured": False,
        "next": [
            "Set timezone and repository-relative primary_cv path in config/career-ledger.json.",
            "Supply the CV and attributed facts/answers; no candidate defaults were copied.",
            "Wire available mail/browser/calendar capabilities and call bootstrap.",
        ],
    }
