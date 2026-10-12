"""Explicit pinned Kev installation; metadata-only alternatives are never downloaded."""
import hashlib
import http.client
import json
import os
from pathlib import Path
import urllib.request

from .catalog import approval_token, directory
from .profile_data import profile


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def selected_files(spec):
    if spec["id"] != "kev-0.8b":
        raise ValueError("metadata_only: this alternative has no implemented download or inference lifecycle")
    for component in spec["components"]:
        for item in component["files"]:
            yield component["role"], item


def verify(cache, identifier):
    spec = profile(identifier)
    root = directory(cache, spec)
    plan = list(selected_files(spec))
    try:
        allowed = {root / role / item["path"] for role, item in plan}
        for role in {role for role, _ in plan}:
            for candidate in (root / role).rglob("*"):
                if candidate.is_symlink() or (candidate.is_file() and candidate not in allowed):
                    raise ValueError()
        manifest = json.loads((root / "manifest.json").read_text())
        if manifest.get("id") != identifier or manifest.get("revision") != spec["revision"]:
            raise ValueError()
        for role, item in plan:
            path = root / role / item["path"]
            if not item["sha256"] or path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
                raise ValueError()
    except (OSError, ValueError):
        raise ValueError("model_integrity_error: selected multi-file model is missing or differs from reviewed hashes; run an explicit approved download to recover") from None
    return root


def download(cache, identifier, approved):
    spec = profile(identifier)
    if approved != approval_token(spec):
        raise ValueError("download_not_approved: review info and pass its current --approve token")
    root = directory(cache, spec)
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    recovered = 0
    for role, item in selected_files(spec):
        target = root / role / item["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.is_file() and target.stat().st_size == item["bytes"] and digest(target) == item["sha256"]:
            recovered += 1
            continue
        part = target.parent / f"{target.name}.{os.getpid()}.part"
        try:
            with opener.open(urllib.request.Request(item["url"], headers={"User-Agent": "job-harness-local/0.1"}), timeout=30) as response, part.open("xb") as stream:
                total = 0
                while chunk := response.read(1024 * 1024):
                    total += len(chunk)
                    if total > item["bytes"]:
                        raise ValueError("model_integrity_error: artifact exceeds reviewed size")
                    stream.write(chunk)
                stream.flush()
                os.fsync(stream.fileno())
            if part.stat().st_size != item["bytes"] or digest(part) != item["sha256"]:
                raise ValueError("model_integrity_error: reviewed artifact digest differs")
            os.replace(part, target)
        except (OSError, urllib.error.URLError, http.client.HTTPException):
            raise ValueError("download_failed: completed pinned files remain resumable; retry the explicit approved download") from None
        finally:
            part.unlink(missing_ok=True)
    manifest = {"id": identifier, "revision": spec["revision"], "component_revisions": {component["role"]: component["revision"] for component in spec["components"]}, "download_bytes": spec["download_bytes"], "files_verified": sum(1 for _ in selected_files(spec))}
    temporary = root / f"manifest.{os.getpid()}.part"
    temporary.write_text(json.dumps(manifest, indent=2) + "\n")
    os.replace(temporary, root / "manifest.json")
    verify(cache, identifier)
    return {**manifest, "installed": True, "resumed_files": recovered}
