"""Explicit private data transfers with path filters; not a secret-content scanner."""

import hashlib
import json
import os
import re
import stat
import tempfile
import unicodedata
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

from .configuration import LEDGER_SCHEMA
from .ledger import Ledger, validate
from .workspace import GUIDES, _initialize

FORMAT = "job-harness.workspace"
MAX_FILES = 10000
MAX_FILE_BYTES = 32 * 1024 * 1024
MAX_TOTAL_BYTES = 256 * 1024 * 1024
DATA_DIRS = {"applications", "context", "content", "config", "sources", "output", "fonts", "docs"}
DATA_FILES = {"AGENTS.md", "README.md", "resume.typ"}
EXCLUDED_PARTS = {".git", ".scratch", ".venv", "node_modules", "__pycache__"}


def allowed_path(name):
    if not isinstance(name, str) or "\\" in name or ":" in name.split("/")[0]:
        return False
    parts = name.split("/")
    if not parts or any(part in {"", ".", ".."} for part in parts):
        return False
    if any(
        part.casefold() in EXCLUDED_PARTS
        or part.casefold().startswith((".career-tmp-", ".job-harness-"))
        for part in parts
    ):
        return False
    base = parts[-1].lower()
    if (
        base in {".lock", ".ds_store"}
        or base.startswith(".env")
        or base.endswith((".key", ".pem", ".p12", ".jhb", ".har", ".kdbx"))
    ):
        return False
    if base.startswith("id_"):
        return False
    if base in {
        "credentials.json",
        "credentials",
        "cookies.json",
        "session.json",
        ".netrc",
        "cookies",
        "login data",
        "logins.json",
        "key4.db",
        "cookies.sqlite",
        "cookies-journal",
        "login data-journal",
        "local state",
        "web data",
        "storage-state.json",
        "storage_state.json",
    }:
        return False
    if name in {"docs/" + guide for guide in GUIDES}:
        return False
    return name in DATA_FILES or (parts[0] in DATA_DIRS and len(parts) > 1)


def assert_external_workspace(root):
    for parent in [root, *root.parents]:
        if (parent / "pyproject.toml").is_file() and (parent / "job_harness/service.py").is_file():
            raise ValueError(
                "Private workspaces and bundles must stay outside the Job Harness checkout."
            )


def _profile_paths(ledger):
    settings = ledger.settings()
    fixed = {"config/career-ledger.json", "config/career-workflows.json", "README.md", "AGENTS.md"}
    fixed.update(settings["answer_sources"])
    if settings["primary_cv"]:
        fixed.add(settings["primary_cv"])
    return fixed


def export_bundle(root, destination, kind="workspace"):
    root, destination = Path(root).resolve(), Path(destination).absolute()
    assert_external_workspace(destination)
    # Inode identity catches alternate capitalization on case-insensitive filesystems.
    if destination.parent.resolve().is_relative_to(root) or any(
        parent.exists() and parent.samefile(root)
        for parent in [destination.parent, *destination.parent.parents]
    ):
        raise ValueError("Write bundles outside the private workspace.")
    if kind not in {"workspace", "profile"}:
        raise ValueError("Bundle kind must be workspace or profile.")
    if destination.exists() or destination.is_symlink():
        raise ValueError("Choose a new bundle filename; existing files are never replaced.")
    if not (root / "config/career-ledger.json").is_file():
        raise ValueError("Choose an initialized private workspace.")
    ledger = Ledger(root)
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=destination.parent, prefix=".job-harness-bundle-")
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w+b") as stream, ledger.reading():
            ledger.settings()
            # Check canonical state before transferring it; unresolved attempts are preserved.
            ledger.reconcile_tracker()
            profile_files = _profile_paths(ledger)
            paths = []
            total = 0
            for current, directories, filenames in os.walk(root, followlinks=False):
                current = Path(current)
                directories[:] = sorted(
                    d
                    for d in directories
                    if d.casefold() not in EXCLUDED_PARTS and (current != root or d in DATA_DIRS)
                )
                for directory in directories:
                    if (current / directory).is_symlink():
                        raise ValueError("Bundles refuse symlinked data directories.")
                for filename in sorted(filenames):
                    path = current / filename
                    name = path.relative_to(root).as_posix()
                    if not allowed_path(name) or path == destination:
                        continue
                    if (
                        kind == "profile"
                        and name not in profile_files
                        and not name.startswith(("context/", "content/"))
                    ):
                        continue
                    if path.is_symlink():
                        raise ValueError("Bundles refuse symlinked data: " + name)
                    if not path.is_file():
                        continue
                    size = path.stat().st_size
                    total += size
                    if size > MAX_FILE_BYTES or total > MAX_TOTAL_BYTES or len(paths) >= MAX_FILES:
                        raise ValueError("Bundle exceeds documented file/size limits.")
                    paths.append((path, name))
            required = {"config/career-ledger.json", *ledger.settings()["answer_sources"]}
            if ledger.settings()["primary_cv"]:
                required.add(ledger.settings()["primary_cv"])
            included = {name for _, name in paths}
            missing = sorted(required - included)
            if missing:
                raise ValueError("Configured files cannot be bundled: " + ", ".join(missing))
            if kind == "profile" and any(
                name.startswith("applications/operations/") for name in included
            ):
                raise ValueError("Profile bundles cannot contain coordinator history.")
            manifest = {
                "format": FORMAT,
                "schema_version": 1,
                "kind": kind,
                "created_at": datetime.now(timezone.utc).isoformat(),
                "files": [],
            }
            with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                read_total = 0
                for path, name in paths:
                    # Refuse a final-component symlink swapped in after the inventory.
                    file_fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
                    with os.fdopen(file_fd, "rb") as payload:
                        data = payload.read(MAX_FILE_BYTES + 1)
                    read_total += len(data)
                    if len(data) > MAX_FILE_BYTES or read_total > MAX_TOTAL_BYTES:
                        raise ValueError("Data changed beyond the bundle limit: " + name)
                    archive.writestr(name, data)
                    manifest["files"].append(
                        {
                            "path": name,
                            "size": len(data),
                            "sha256": hashlib.sha256(data).hexdigest(),
                        }
                    )
                archive.writestr(
                    "manifest.json", json.dumps(manifest, sort_keys=True, separators=(",", ":"))
                )
            stream.flush()
            os.fsync(stream.fileno())
        # Hard-link publication refuses a racing existing destination, unlike replace().
        try:
            os.link(temporary, destination)
        except OSError as error:
            raise ValueError(
                "Bundle destination does not support safe no-overwrite publication; choose a local filesystem."
            ) from error
        directory = os.open(destination.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
        extensions = {}
        for _, name in paths:
            extension = Path(name).suffix.lower() or "no extension"
            extensions[extension] = extensions.get(extension, 0) + 1
        return {
            "bundle": str(destination),
            "kind": kind,
            "files": len(paths),
            "encrypted": False,
            "extensions": extensions,
            "note": "Private plaintext ZIP; store locally or encrypt before sharing. Workspace bundles preserve attempts; profile bundles omit application history.",
        }
    finally:
        Path(temporary).unlink(missing_ok=True)


def _manifest(archive):
    infos = archive.infolist()
    if len(infos) > MAX_FILES + 1 or sum(i.file_size for i in infos) > MAX_TOTAL_BYTES:
        raise ValueError("Bundle exceeds documented expansion limits.")
    if any(i.flag_bits & 1 for i in infos):
        raise ValueError("Decrypt the private bundle outside the harness before import.")
    names = [i.filename for i in infos]
    if (
        len(names) != len(set(unicodedata.normalize("NFC", name).casefold() for name in names))
        or names.count("manifest.json") != 1
    ):
        raise ValueError("Bundle contains duplicate names or no unique manifest.")
    if archive.getinfo("manifest.json").file_size > 2 * 1024 * 1024:
        raise ValueError("Bundle manifest exceeds its limit.")
    manifest = json.loads(archive.read("manifest.json"))
    if not isinstance(manifest, dict) or set(manifest) != {
        "format",
        "schema_version",
        "kind",
        "created_at",
        "files",
    }:
        raise ValueError("Unsupported bundle manifest.")
    if (
        manifest["format"] != FORMAT
        or type(manifest["schema_version"]) is not int
        or manifest["schema_version"] != 1
        or manifest["kind"] not in {"workspace", "profile"}
    ):
        raise ValueError("Unsupported bundle format/version/kind.")
    if not isinstance(manifest["created_at"], str):
        raise ValueError("Invalid bundle creation metadata.")
    files = manifest["files"]
    if not isinstance(files, list) or len(files) > MAX_FILES:
        raise ValueError("Invalid bundle file list.")
    declared = set()
    for entry in files:
        if not isinstance(entry, dict) or set(entry) != {"path", "size", "sha256"}:
            raise ValueError("Invalid bundle file entry.")
        name = entry["path"]
        if (
            not allowed_path(name)
            or name in declared
            or type(entry["size"]) is not int
            or not 0 <= entry["size"] <= MAX_FILE_BYTES
            or not isinstance(entry["sha256"], str)
            or not re.fullmatch(r"[a-f0-9]{64}", entry["sha256"])
        ):
            raise ValueError("Invalid or unsafe bundle entry.")
        declared.add(name)
        if name not in names:
            raise ValueError("Declared bundle file is missing.")
        info = archive.getinfo(name)
        if (
            info.is_dir()
            or info.file_size != entry["size"]
            or stat.S_ISLNK(info.external_attr >> 16)
        ):
            raise ValueError("Bundle entry size/type differs from manifest.")
    if declared | {"manifest.json"} != set(names) or "config/career-ledger.json" not in declared:
        raise ValueError("Bundle has undeclared files or missing workspace configuration.")
    return manifest


def import_bundle(root, source):
    try:
        return _import_bundle(root, source)
    except (zipfile.BadZipFile, UnicodeError) as error:
        raise ValueError("Bundle ZIP or text data is invalid.") from error


def _import_bundle(root, source):
    root, source = Path(root).resolve(), Path(source).resolve()
    assert_external_workspace(root)
    if root.exists() and (not root.is_dir() or any(root.iterdir())):
        raise ValueError(
            "Import only into a new/empty workspace; no merge or overwrite is implicit."
        )
    root.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(source) as archive:
        manifest = _manifest(archive)
        with tempfile.TemporaryDirectory(
            dir=root.parent, prefix=".job-harness-import-"
        ) as temporary:
            stage = Path(temporary)
            config = json.loads(archive.read("config/career-ledger.json"))
            validate(LEDGER_SCHEMA, config, "bundle_configuration")
            required = {
                "config/career-ledger.json",
                *config.get(
                    "answer_sources",
                    [
                        "applications/approved-answers.md",
                        "context/profile/application-decisions.md",
                    ],
                ),
            }
            if config.get("primary_cv"):
                required.add(config["primary_cv"])
            if required - {entry["path"] for entry in manifest["files"]}:
                raise ValueError("Bundle is missing configured answer sources or CV.")
            if manifest["kind"] == "profile":
                configured = {
                    "AGENTS.md",
                    "README.md",
                    "config/career-ledger.json",
                    "config/career-workflows.json",
                    *config.get(
                        "answer_sources",
                        [
                            "applications/approved-answers.md",
                            "context/profile/application-decisions.md",
                        ],
                    ),
                }
                if config.get("primary_cv"):
                    configured.add(config["primary_cv"])
                for entry in manifest["files"]:
                    name = entry["path"]
                    if name.startswith("applications/operations/") or not (
                        name.startswith(("context/", "content/")) or name in configured
                    ):
                        raise ValueError("Profile bundle contains non-profile history or files.")
                _initialize(stage)
            for entry in manifest["files"]:
                data = archive.read(entry["path"])
                if hashlib.sha256(data).hexdigest() != entry["sha256"]:
                    raise ValueError("Bundle file digest mismatch: " + entry["path"])
                target = stage.joinpath(*PurePosixPath(entry["path"]).parts)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
                target.chmod(0o600)
            (stage / "applications").mkdir(exist_ok=True)
            ledger = Ledger(stage)
            ledger.settings()
            state = ledger.load_state()
            if state.get("pending"):
                raise ValueError(
                    "Bundle contains an interrupted journal; resolve it in the source workspace first."
                )
            ledger.dispatch(
                "rebuild_pipeline",
                {
                    "idempotency_key": "bundle:rebuild:"
                    + hashlib.sha256(archive.read("manifest.json")).hexdigest()
                },
            )
            for directory in [stage, *[p for p in stage.rglob("*") if p.is_dir()]]:
                directory.chmod(0o700)
            stage.replace(root)
    return {
        "workspace": str(root),
        "kind": manifest["kind"],
        "files": len(manifest["files"]),
        "note": "Imported locally. Provider sessions must be connected separately; no external action was executed.",
    }
