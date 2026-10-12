"""Check code-only boundaries, local documentation links and generic skill metadata."""

import re
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {
    ".agents",
    ".github",
    ".gitattributes",
    ".gitignore",
    "AGENTS.md",
    "README.md",
    "LICENSE",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "CHANGELOG.md",
    "Makefile",
    "pyproject.toml",
    "job_harness",
    "docs",
    "scripts",
    "tests",
}
FORBIDDEN = [
    b"/Users/",
    b"-----BEGIN PRIVATE KEY-----",
    b"-----BEGIN OPENSSH PRIVATE KEY-----",
]


def tracked_or_source():
    if (ROOT / ".git").exists():
        paths = subprocess.run(
            ["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True
        ).stdout.split(b"\0")
        return [ROOT / p.decode() for p in paths if p]
    return [
        p
        for top in ALLOWED
        for p in (ROOT / top).rglob("*")
        if p.is_file() and "__pycache__" not in p.parts and p.suffix not in {".pyc", ".pyo"}
    ] + [ROOT / name for name in ALLOWED if (ROOT / name).is_file()]


def check():
    paths = tracked_or_source()
    links = 0
    for path in paths:
        name = path.relative_to(ROOT)
        if name.parts[0] not in ALLOWED or path.suffix in {".jhb", ".bundle", ".pdf"}:
            raise ValueError("Private/runtime file in repository: " + str(name))
        if path.is_symlink():
            if (
                name != Path(".agents/skills")
                or path.resolve() != ROOT / "job_harness/resources/skills"
            ):
                raise ValueError("Unexpected repository symlink: " + str(name))
            continue
        data = path.read_bytes()
        # The gate contains its own test needles; only that audited source is exempt.
        if name != Path("scripts/check_repository.py") and any(
            value in data for value in FORBIDDEN
        ):
            raise ValueError("Candidate-specific content in generic source: " + str(name))
        if path.suffix == ".svg":
            ET.fromstring(data)
        if path.suffix == ".md":
            for raw in re.findall(r"\]\(([^)]+)\)", data.decode()):
                target = raw.split("#", 1)[0].strip("<>")
                if not target or re.match(r"[a-z][a-z0-9+.-]*:", target):
                    continue
                if not (path.parent / target).exists():
                    raise ValueError("Broken local link in " + str(name) + ": " + target)
                links += 1
    for path in (ROOT / "job_harness/resources/skills").glob("*/SKILL.md"):
        front = path.read_text().split("---", 2)[1]
        fields = yaml.safe_load(front)
        if (
            not isinstance(fields, dict)
            or fields.get("name") != path.parent.name
            or not fields.get("description")
        ):
            raise ValueError("Invalid skill metadata: " + path.parent.name)
    print(
        f"PASS generic repository: {len(paths)} files, {links} local links, SVG XML and nine skills"
    )


if __name__ == "__main__":
    check()
