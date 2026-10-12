"""Reviewed public model metadata. No SDK imports, network, or weight downloads."""
from __future__ import annotations

import hashlib
import json
import os
import platform
from pathlib import Path

CATALOG = {
    "embeddinggemma2-text": {
        "id": "embeddinggemma2-text",
        "revision": "9be6e8b90982095dc05c2bd162e4b954ee4dbac7",
        "artifact": "embeddinggemma-2-text-270m.litertlm",
        "download_bytes": 164626432,
        "sha256": "2d079ee2f6f066b1f368e8d7c819f55214eaef1d0513b312321901f30ab286fb",
        "md5": None,
        "source": "https://huggingface.co/litert-community/embeddinggemma-2-text-270m-litert-lm/resolve/9be6e8b90982095dc05c2bd162e4b954ee4dbac7/embeddinggemma-2-text-270m.litertlm",
        "model_card": "https://huggingface.co/litert-community/embeddinggemma-2-text-270m-litert-lm",
        "license": "apache-2.0",
        "license_basis": "pinned model repository metadata and card; review its license before downloading",
        "capabilities": ["text_embeddings"],
        "parameters": 270000000,
        "dimensions": 768,
        "max_tokens": 2048,
        "runtime": "litert_lm.EmbeddingEngine",
        "working_memory_bytes": None,
        "hardware": "CPU; LiteRT-LM 0.18.0 wheels support macOS 12 arm64 and Linux x86_64/aarch64. This adapter selects CPU only.",
        "unknowns": ["RAM and latency depend on device and selected context length; measure locally", "CPU threads explicitly selected through LiteRT-LM; GPU/NPU paths untested"],
        "metadata_verified": "2026-10-12",
    },
    "laya-s256": {
        "id": "laya-s256",
        "revision": "gcs-1791231584156797",
        "artifact": "laya_s256.task",
        "download_bytes": 678477844,
        "sha256": "8cb730b89ef99cb5c98cdbc7936de79deab77623ce25dc0031eec9e25c64c24e",
        "md5": "7545ac021b8378f0a2967998eca71bbb",
        "source": "https://storage.googleapis.com/mediapipe-models/decision_maker/laya/float32/laya_s256/latest/laya_s256.task?generation=1791231584156797",
        "model_card": "https://huggingface.co/litert-community/laya-LiteRT",
        "license": "apache-2.0",
        "license_basis": "official guide's linked model card declares Apache-2.0; GCS does not provide separate license metadata",
        "capabilities": ["choice", "boolean", "nullable_boolean", "score", "triage"],
        "parameters": None,
        "dimensions": None,
        "max_tokens": 256,
        "runtime": "mediapipe.tasks.python.decision.decision_maker.DecisionMaker",
        "working_memory_bytes": None,
        "hardware": "CPU; MediaPipe 1.1.0 wheels support macOS arm64 and Linux glibc 2.28 x86_64/aarch64. No GPU or accelerator required.",
        "unknowns": ["Upstream guide's 149M claim differs from linked card's 421M/322M checkpoints; artifact parameter count is unverified", "RAM, latency and native CPU thread count must be measured", "SHA256 verified locally against the pinned GCS generation and MD5"],
        "metadata_verified": "2026-10-12",
    },
}


def model(identifier: str) -> dict:
    if identifier not in CATALOG:
        from .profile_data import profile
        return dict(profile(identifier))
    return dict(CATALOG[identifier])


def approval_token(spec: dict) -> str:
    keys = ("id", "revision", "artifact", "download_bytes", "sha256", "md5", "source", "model_card", "license", "license_basis", "hardware", "components")
    scope = {key: spec[key] for key in keys if key in spec}
    return hashlib.sha256(json.dumps(scope, sort_keys=True, separators=(",", ":")).encode()).hexdigest()[:16]


def default_cache() -> Path:
    return Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))) / "job-harness" / "models"


def directory(cache: Path, spec: dict) -> Path:
    return cache / spec["id"] / spec["revision"]


def info(identifier: str, cache: Path) -> dict:
    spec = model(identifier)
    spec["approval_token"] = approval_token(spec)
    spec["installed"] = (directory(cache, spec) / "manifest.json").is_file() and ("artifact" not in spec or (directory(cache, spec) / spec["artifact"]).is_file())
    if "components" in spec:
        spec["installed"] = spec["installed"] and all((directory(cache, spec) / component["role"] / item["path"]).is_file() for component in spec["components"] for item in component["files"])
    spec["local_hardware"] = {"system": platform.system(), "architecture": platform.machine(), "logical_cpus": os.cpu_count(), "effective_cpus": len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None, "ram_bytes": None}
    spec["automatic_download"] = False
    return spec
