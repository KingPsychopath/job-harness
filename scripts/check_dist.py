"""Audit wheel/sdist contents and exact bundled resource parity before distribution."""

import email
import tarfile
import zipfile
from pathlib import Path

from check_repository import FORBIDDEN

ROOT = Path(__file__).resolve().parents[1]
ALLOWED_TOP = {
    "job_harness",
    "scripts",
    "tests",
    "docs",
    "README.md",
    "AGENTS.md",
    "LICENSE",
    "SECURITY.md",
    "CONTRIBUTING.md",
    "CHANGELOG.md",
    "Makefile",
    "pyproject.toml",
    "PKG-INFO",
    "setup.cfg",
}
FORBIDDEN_SUFFIXES = {".jhb", ".bundle", ".pdf", ".pyc", ".pyo"}


def require(condition, message="Verification failed"):
    if not condition:
        raise AssertionError(message)


def audit():
    wheel = sorted((ROOT / "dist").glob("*.whl"))[-1]
    source = sorted((ROOT / "dist").glob("*.tar.gz"))[-1]
    expected = {
        p.relative_to(ROOT).as_posix(): p.read_bytes()
        for folder in [ROOT / "job_harness/resources", ROOT / "job_harness/defaults"]
        for p in folder.rglob("*")
        if p.is_file()
    }
    with zipfile.ZipFile(wheel) as archive:
        wheel_files = {name: archive.read(name) for name in archive.namelist()}
    with tarfile.open(source) as archive:
        source_files = {
            member.name.split("/", 1)[1]: archive.extractfile(member).read()
            for member in archive.getmembers()
            if member.isfile()
        }
        require(
            not any((member.issym() or member.islnk() for member in archive.getmembers())),
            "Verification failed",
        )
    for kind, files in [("wheel", wheel_files), ("sdist", source_files)]:
        for name, data in files.items():
            parts = Path(name).parts
            metadata = parts[0].endswith(".dist-info" if kind == "wheel" else ".egg-info")
            require(
                metadata or parts[0] in (ALLOWED_TOP if kind == "sdist" else {"job_harness"}), name
            )
            require(
                not any(
                    (part in {"CLAUDE.md", ".git", ".scratch", "__pycache__"} for part in parts)
                ),
                name,
            )
            require(Path(name).suffix not in FORBIDDEN_SUFFIXES, name)
            if name not in {"scripts/check_dist.py", "scripts/check_repository.py"}:
                require(not any((needle in data for needle in FORBIDDEN)), name)
        for name, data in expected.items():
            require(files.get(name) == data, f"{kind} missing/stale resource: {name}")
        actual = {
            name
            for name in files
            if name.startswith(("job_harness/resources/", "job_harness/defaults/"))
        }
        require(actual == set(expected), f"{kind} unexpected resource")
    metadata = email.message_from_bytes(
        next((data for name, data in wheel_files.items() if name.endswith("/METADATA")))
    )
    require(
        not [d for d in metadata.get_all("Requires-Dist", []) if "extra ==" not in d],
        "Runtime dependency",
    )
    print(
        f"PASS wheel/sdist allowlists, no private paths, exact parity for {len(expected)} resources, zero runtime dependencies"
    )


if __name__ == "__main__":
    audit()
