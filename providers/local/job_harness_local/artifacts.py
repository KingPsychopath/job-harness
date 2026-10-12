"""Explicit, bounded, version-pinned model installation."""
from __future__ import annotations

import hashlib
import http.client
import json
import os
import urllib.request
from pathlib import Path

from .catalog import CATALOG, approval_token, directory, model


def hashes(path: Path) -> tuple[int, str, str]:
    sha, md5, size = hashlib.sha256(), hashlib.md5(usedforsecurity=False), 0
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            size += len(chunk)
            sha.update(chunk)
            md5.update(chunk)
    return size, sha.hexdigest(), md5.hexdigest()


def verify(cache: Path, identifier: str) -> Path:
    if identifier not in CATALOG:
        from .profile_artifacts import verify as verify_profile
        return verify_profile(cache, identifier)
    spec = model(identifier)
    root = directory(cache, spec)
    try:
        manifest = json.loads((root / "manifest.json").read_text())
        path = root / spec["artifact"]
        size, sha, md5 = hashes(path)
    except (OSError, ValueError):
        raise ValueError("model_not_installed: run info, then an explicit download") from None
    if manifest.get("id") != identifier or manifest.get("revision") != spec["revision"] or size != spec["download_bytes"] or sha != manifest.get("sha256") or (spec["sha256"] and sha != spec["sha256"]) or (spec["md5"] and md5 != spec["md5"]):
        raise ValueError("model_integrity_error: artifact does not match its reviewed identity")
    return path


def download(cache: Path, identifier: str, approved: str) -> dict:
    if identifier not in CATALOG:
        from .profile_artifacts import download as download_profile
        return download_profile(cache, identifier, approved)
    spec = model(identifier)
    if approved != approval_token(spec):
        raise ValueError("download_not_approved: read info and pass its current --approve token")
    root = directory(cache, spec)
    root.mkdir(parents=True, exist_ok=True)
    target = root / spec["artifact"]
    if target.exists():
        try:
            verify(cache, identifier)
            return {"id": identifier, "revision": spec["revision"], "already_installed": True}
        except ValueError:
            # Repair interrupted publication without redownloading valid pinned bytes.
            size, sha, md5 = hashes(target)
            if size == spec["download_bytes"] and sha == spec["sha256"] and (not spec["md5"] or md5 == spec["md5"]):
                manifest = {"id": identifier, "revision": spec["revision"], "download_bytes": size, "sha256": sha, "md5": md5, "source": spec["source"], "license": spec["license"]}
                repaired = root / f"manifest.{os.getpid()}.part"
                repaired.write_text(json.dumps(manifest, indent=2) + "\n")
                os.replace(repaired, root / "manifest.json")
                return {**manifest, "installed": True, "recovered_manifest": True}
            raise ValueError("model_integrity_error: cached bytes differ; remove this selected model directory before downloading again") from None
    part = root / f"{spec['artifact']}.{os.getpid()}.part"
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        request = urllib.request.Request(spec["source"], headers={"User-Agent": "job-harness-local/0.1"})
        with opener.open(request, timeout=30) as response, part.open("xb") as stream:
            total = 0
            while chunk := response.read(1024 * 1024):
                total += len(chunk)
                if total > spec["download_bytes"]:
                    raise ValueError("model_integrity_error: artifact exceeds reviewed download size")
                stream.write(chunk)
            stream.flush()
            os.fsync(stream.fileno())
        size, sha, md5 = hashes(part)
        if size != spec["download_bytes"] or (spec["sha256"] and sha != spec["sha256"]) or (spec["md5"] and md5 != spec["md5"]):
            raise ValueError("model_integrity_error: downloaded size or digest differs")
        os.replace(part, target)
        manifest = {"id": identifier, "revision": spec["revision"], "download_bytes": size, "sha256": sha, "md5": md5, "source": spec["source"], "license": spec["license"]}
        temp = root / f"manifest.{os.getpid()}.part"
        temp.write_text(json.dumps(manifest, indent=2) + "\n")
        os.replace(temp, root / "manifest.json")
        return {**manifest, "installed": True}
    except (OSError, urllib.error.URLError, http.client.HTTPException):
        raise ValueError("download_failed: no completed installation; retry explicit download") from None
    finally:
        part.unlink(missing_ok=True)
