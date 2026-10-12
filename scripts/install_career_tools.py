#!/usr/bin/env python3
"""Optional client-neutral skill links and MCP launch metadata; never stores credentials."""

import argparse
import json
import sys
from pathlib import Path

from job_harness.host import connection, default_workspace
from job_harness.workspace import SKILLS, source_file


def install(skills_dir):
    destination = Path(skills_dir).expanduser().resolve()
    plans = []
    for name in SKILLS:
        source = source_file(f".agents/skills/{name}/SKILL.md").parent
        target = destination / name
        if target.is_symlink() and target.resolve() == source:
            continue
        if target.exists() or target.is_symlink():
            raise ValueError(f"Conflicting skill: {target}; preserve and reconcile it first")
        plans.append((source, target))
    destination.mkdir(parents=True, exist_ok=True)
    for source, target in plans:
        target.symlink_to(source, target_is_directory=True)
    return {"linked": len(plans), "directory": str(destination)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skills-dir", type=Path, action="append", default=[])
    parser.add_argument("--print-mcp", action="store_true")
    parser.add_argument("--root", type=Path)
    parser.add_argument("--tool-profile", choices=["focused", "full"], default="focused")
    args = parser.parse_args()
    if not args.skills_dir and not args.print_mcp:
        parser.error("Choose --skills-dir or --print-mcp")
    try:
        if args.print_mcp:
            root = args.root or default_workspace()
            if root is None:
                raise ValueError("Select a workspace with --root or job-harness configure")
            print(json.dumps(connection(root, args.tool_profile)))
        for destination in dict.fromkeys(args.skills_dir):
            print(json.dumps(install(destination)))
    except (ValueError, OSError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
