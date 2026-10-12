#!/usr/bin/env python3
"""Compatibility entry point; supports the existing registered launch command."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from job_harness.mcp import main

if __name__ == "__main__":
    main(default_profile="full")
