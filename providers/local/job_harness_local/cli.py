from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from .artifacts import download, verify
from .catalog import CATALOG, default_cache, info, model
from .profile_data import profiles


def main(argv=None):
    names = list(CATALOG) + [item["id"] for item in profiles()]
    parser = argparse.ArgumentParser(description="Optional local typed inference. No automatic weight download.")
    parser.add_argument("--cache-dir", type=Path, default=default_cache())
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list", help="show metadata without loading or downloading models")
    describe = sub.add_parser("info", help="review model size, source, license, hardware and unknowns")
    describe.add_argument("model", choices=names)
    install = sub.add_parser("download", help="explicitly install exactly the reviewed model artifact")
    install.add_argument("model", choices=names)
    install.add_argument("--approve", required=True, help="approval_token from info; confirms its stated download scope and license review")
    serve = sub.add_parser("serve", help="serve installed models on 127.0.0.1 only; Ctrl-C releases them")
    serve.add_argument("--embedding-model", choices=["embeddinggemma2-text"])
    serve.add_argument("--decision-model", choices=["laya-s256", "kev-0.8b"])
    serve.add_argument("--port", type=int, default=18081)
    serve.add_argument("--threads", type=int, default=2)
    serve.add_argument("--timeout", type=int, default=30, help="hard inference worker deadline, seconds")
    serve.add_argument("--startup-timeout", type=int, default=60)
    args = parser.parse_args(argv)
    try:
        if args.command == "list":
            result = {"required": False, "automatic_downloads": False, "models": [info(name, args.cache_dir) for name in names]}
        elif args.command == "info":
            result = info(args.model, args.cache_dir)
        elif args.command == "download":
            result = download(args.cache_dir, args.model, args.approve)
        else:
            if not args.embedding_model and not args.decision_model:
                raise ValueError("invalid_arguments: explicitly select at least one installed model")
            if not 0 <= args.port <= 65535 or not 1 <= args.threads <= 16 or not 1 <= args.timeout <= 120 or not 1 <= args.startup_timeout <= 120:
                raise ValueError("invalid_arguments: port 0–65535, threads 1–16 and deadlines 1–120 seconds")
            artifacts = {name: str(verify(args.cache_dir, name).resolve()) for name in (args.embedding_model, args.decision_model) if name}
            from .server import Gateway, Worker
            worker = Worker(artifacts, args.threads, args.timeout, args.startup_timeout)
            try:
                server = Gateway(args.port, worker, {name: model(name)["revision"] for name in artifacts})
                print(json.dumps({"endpoint": f"http://127.0.0.1:{server.server_port}/v1", "models": server.models, "metadata": worker.metadata}), flush=True)
                try:
                    server.serve_forever(poll_interval=0.25)
                finally:
                    server.server_close()
            finally:
                worker.close()
            return 0
        print(json.dumps(result, indent=2, allow_nan=False))
        return 0
    except KeyboardInterrupt:
        return 0
    except (OSError, ValueError) as exc:
        # Catalog/lifecycle errors are fixed metadata; inference diagnostics never reach here.
        message = str(exc) if isinstance(exc, ValueError) else "local_provider_failed: inspect optional environment and selected local paths"
        print(json.dumps({"ok": False, "error": message}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
