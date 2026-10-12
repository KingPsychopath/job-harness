"""Compact metadata for the native CLI/dashboard, derived from reviewed catalogs."""
import json
from pathlib import Path

from .catalog import CATALOG


def summary():
    root = Path(__file__).resolve().parent.parent
    alternatives = json.loads((root / "model-profiles.json").read_text())
    profiles = []
    for name, entry in CATALOG.items():
        profiles.append({"id": name, "status": "local_sdk_verified", "capabilities": entry["capabilities"], "download_bytes": entry["download_bytes"], "working_memory_bytes": entry["working_memory_bytes"], "hardware": entry["hardware"], "runtime": entry["runtime"], "revision": entry["revision"], "model_card": entry["model_card"], "license": entry["license"], "license_basis": entry["license_basis"], "unknowns": entry["unknowns"], "measured": {"scope": "fictional native HTTP fixture; combined worker peak about 1.88 GiB, not a per-model RAM requirement", "report": "examples/models/benchmark-2026-10-12.json"}})
    for entry in alternatives["profiles"]:
        profiles.append({"id": entry["id"], "status": entry["status"], "capabilities": entry["capabilities"], "download_bytes": entry["download_bytes"], "working_memory_bytes": entry["working_memory_bytes"], "hardware": entry.get("hardware", "Kev author-supported MLX on Apple Silicon; task RAM/latency unmeasured here") if entry["id"].startswith("kev") else "9B local compatible inference or separately selected hosted API; not trialled", "runtime": entry["runtime"], "component_revisions": {component["role"]: component["revision"] for component in entry["components"]}, "model_card": "https://huggingface.co/" + entry["components"][0]["repository"], "license": entry["components"][0]["license"], "unknowns": entry["unknowns"], "measured": entry.get("measured")})
    return {"schema_version": 1, "required": False, "automatic_downloads": False, "metadata_verified": alternatives["metadata_verified"], "profiles": profiles}


if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    (root / "job_harness_local" / "model_profiles.json").write_text((root / "model-profiles.json").read_text())
    path = root / "summary.json"
    path.write_text(json.dumps(summary(), indent=2) + "\n")
