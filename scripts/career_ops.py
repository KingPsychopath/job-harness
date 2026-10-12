#!/usr/bin/env python3
"""Compatibility import and CLI entry point for job_harness."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
if __name__ == "__main__":
    from job_harness.__main__ import legacy_cli

    legacy_cli()
else:
    from job_harness import ledger

    sys.modules[__name__] = ledger
