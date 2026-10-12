"""Loopback-only JSON gateway with an isolated, deadline-bounded inference worker."""
from __future__ import annotations

from http.server import BaseHTTPRequestHandler, HTTPServer
import importlib.metadata
import json
from multiprocessing.connection import Connection, Pipe
try:
    import resource
except ImportError:
    resource = None
import subprocess
import sys
import tempfile
from pathlib import Path
import os
import platform
import time

from .runtime import Runtime, peak_rss_bytes, embedding_payload, decision_payload, triage_payload

MAX_BODY = 1024 * 1024
ROUTES = {"/v1/embeddings", "/v1/decisions", "/v1/chat/completions"}
SAFE_CODES = {"invalid_arguments", "unsupported_question", "unsupported_generation", "unsupported_format", "unsupported_route", "unknown_model", "model_not_loaded", "model_integrity_error", "context_budget", "provider_schema_error", "model_inference_failed", "unsupported_runtime"}


def clean_error(exc):
    if isinstance(exc, ValueError) and str(exc).split(":", 1)[0] in SAFE_CODES:
        return str(exc)
    return "model_inference_failed: native runtime rejected the bounded request; no input or native error is logged"


def worker_main(pipe, artifacts, threads):
    for name in ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "VECLIB_MAXIMUM_THREADS", "MKL_NUM_THREADS", "NUMEXPR_NUM_THREADS"):
        os.environ[name] = str(threads)
    os.environ["TOKENIZERS_PARALLELISM"] = "false"
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    if resource is not None:
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    if sys.platform.startswith("linux"):
        import ctypes
        if ctypes.CDLL(None, use_errno=True).prctl(4, 0, 0, 0, 0) != 0:
            raise RuntimeError("worker hardening failed")
    # Linux affinity limits runnable CPUs, not the SDK's actual thread count.
    if hasattr(os, "sched_setaffinity"):
        allowed = sorted(os.sched_getaffinity(0))
        os.sched_setaffinity(0, allowed[:threads])
    # SDK diagnostics may echo rejected input. Keep native stdout/stderr private.
    with open(os.devnull, "wb") as sink:
        os.dup2(sink.fileno(), 1)
        os.dup2(sink.fileno(), 2)
    runtime = None
    initialized = False
    try:
        dependencies = {"litert-lm-api": "0.18.0"} if "embeddinggemma2-text" in artifacts else {}
        if "laya-s256" in artifacts:
            dependencies.update({"mediapipe": "1.1.0", "tokenizers": "0.23.3"})
        if "kev-0.8b" in artifacts:
            if sys.platform != "darwin" or platform.machine() != "arm64" or sys.version_info[:2] != (3, 12):
                raise ValueError("unsupported_runtime: selected Kev profile requires tested Apple Silicon macOS / Python3.12")
            dependencies.update({"kev": "0.1.0", "torch": "2.8.0", "transformers": "5.19.0", "mlx-lm": "0.31.3", "mlx": "0.32.3", "mlx-metal": "0.32.3", "safetensors": "0.8.0", "tokenizers": "0.23.3", "numpy": "2.5.4", "pydantic": "2.12.3"})
            direct = importlib.metadata.distribution("kev").read_text("direct_url.json")
            provenance = json.loads(direct or "{}")
            if provenance.get("dir_info") or provenance.get("vcs_info", {}).get("commit_id") != "2ecafc49fe1e8945cd600443e6c04c1cf0b3a78f":
                raise ValueError("unsupported_runtime: install the reviewed immutable Kev SDK revision")
            import kev
            distribution_root = Path(importlib.metadata.distribution("kev").locate_file("")).resolve()
            if not Path(kev.__file__).resolve().is_relative_to(distribution_root):
                raise ValueError("unsupported_runtime: Kev import does not resolve to its pinned installed distribution")
        for name, expected in dependencies.items():
            if importlib.metadata.version(name) != expected:
                raise ValueError("unsupported_runtime: use the pinned optional dependencies")
        if "kev-0.8b" in artifacts:
            from .kev_runtime import KevRuntime
            runtime = KevRuntime(artifacts, threads)
        else:
            runtime = Runtime(artifacts, threads)
        pipe.send_bytes(json.dumps({"ok": True, "load_ms": runtime.load_ms, "worker_peak_rss_bytes": peak_rss_bytes(), "sdk_versions": dependencies, "core_dump_limit_bytes": resource.getrlimit(resource.RLIMIT_CORE)[0] if resource is not None else None, "python_isolated": bool(sys.flags.isolated), "temporary_working_directory": Path.cwd().resolve() == Path(os.environ["TMPDIR"]).resolve(), "host_credentials_inherited": any(key.endswith(("_KEY", "_TOKEN")) or key.startswith("AWS_") for key in os.environ), "isolation": "fresh environment, temporary cwd, isolated Python imports, bounded JSON IPC; same uid, filesystem and network rights"}).encode())
        initialized = True
        while True:
            command = json.loads(pipe.recv_bytes(4 * 1024 * 1024))
            if command is None:
                break
            route, payload = command
            try:
                pipe.send_bytes(json.dumps({"ok": True, "data": runtime.call(route, payload)}, allow_nan=False).encode())
            except Exception as exc:
                pipe.send_bytes(json.dumps({"ok": False, "error": clean_error(exc)}).encode())
    except Exception as exc:
        try:
            startup_error = "runtime_unavailable: native worker protocol failed; restart serve explicitly" if initialized else clean_error(exc) if isinstance(exc, ValueError) and str(exc).split(":", 1)[0] in {"unsupported_runtime", "model_integrity_error"} else "runtime_start_failed: install the pinned SDKs and verify model artifacts; native diagnostics are suppressed"
            pipe.send_bytes(json.dumps({"ok": False, "error": startup_error}).encode())
        except (OSError, EOFError):
            pass
    finally:
        if runtime:
            runtime.close()
        pipe.close()


class ChildProcess:
    def __init__(self, process):
        self.process = process
    def is_alive(self):
        return self.process.poll() is None
    def join(self, timeout):
        try:
            self.process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            pass
    def terminate(self):
        self.process.terminate()
    def kill(self):
        self.process.kill()


class Worker:
    def __init__(self, artifacts, threads, timeout=30, startup_timeout=60):
        if os.name != "posix":
            raise ValueError("unsupported_runtime: this optional hardened native worker currently supports macOS/Linux; metadata and compatible external model hosts remain available")
        self.timeout = timeout
        self.temporary = tempfile.TemporaryDirectory(prefix="job-harness-inference-")
        self.pipe, child = Pipe()
        package_root = str(Path(__file__).resolve().parent.parent)
        # -I excludes cwd, PYTHONPATH and user site packages. The one inserted directory
        # is this already selected provider package, never a candidate workspace.
        bootstrap = "import sys,json;sys.path.insert(0,sys.argv[1]);from multiprocessing.connection import Connection;from job_harness_local.server import worker_main;channel=Connection(int(sys.argv[2]));config=json.loads(channel.recv_bytes(1048576));worker_main(channel,config['artifacts'],config['threads'])"
        environment = {"TMPDIR": self.temporary.name, "XDG_CACHE_HOME": self.temporary.name, "GLOG_logtostderr": "1", "GLOG_minloglevel": "3", "TF_CPP_MIN_LOG_LEVEL": "3"}
        started = time.monotonic()
        try:
            process = subprocess.Popen([sys.executable, "-I", "-c", bootstrap, package_root, str(child.fileno())], cwd=self.temporary.name, env=environment, pass_fds=(child.fileno(),), stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.process = ChildProcess(process)
            self.pipe.send_bytes(json.dumps({"artifacts": artifacts, "threads": threads}).encode())
        except Exception:
            self.pipe.close()
            child.close()
            self.temporary.cleanup()
            raise ValueError("runtime_start_failed: could not launch the isolated optional worker") from None
        child.close()
        if not self.pipe.poll(startup_timeout):
            self.close()
            raise ValueError("runtime_start_timeout: native model initialization exceeded the selected deadline")
        try:
            self.metadata = json.loads(self.pipe.recv_bytes(4 * 1024 * 1024))
        except (EOFError, OSError):
            self.close()
            raise ValueError("runtime_start_failed: native worker exited before initialization") from None
        if not self.metadata["ok"]:
            self.close()
            raise ValueError(self.metadata["error"])
        self.metadata["total_startup_ms"] = round((time.monotonic() - started) * 1000, 3)
        self.metadata["cpu_control"] = {"requested_threads": threads, "embedding_sdk_threads": threads if "embeddinggemma2-text" in artifacts else None, "laya_thread_control": "environment hints; Python DecisionMaker exposes no native thread-count field" if "laya-s256" in artifacts else None, "kev_thread_control": "Torch pointer head selected threads; MLX backbone uses Metal" if "kev-0.8b" in artifacts else None, "worker_cpu_affinity": "at most requested CPUs" if hasattr(os, "sched_setaffinity") else "unavailable on this platform"}

    def call(self, route, payload):
        if not self.process.is_alive():
            raise ValueError("runtime_unavailable: worker stopped; restart serve explicitly")
        try:
            self.pipe.send_bytes(json.dumps([route, payload], allow_nan=False).encode())
            if not self.pipe.poll(self.timeout):
                self.close()
                raise ValueError("provider_timeout: inference worker was terminated; restart serve explicitly")
            result = json.loads(self.pipe.recv_bytes(4 * 1024 * 1024))
        except (EOFError, OSError):
            self.close()
            raise ValueError("runtime_unavailable: native worker exited; restart serve explicitly") from None
        if not result["ok"]:
            raise ValueError(result["error"])
        return result["data"]

    def close(self):
        if self.process.is_alive():
            try:
                self.pipe.send_bytes(b"null")
                self.process.join(timeout=1)
            except (OSError, EOFError):
                pass
            if self.process.is_alive():
                self.process.terminate()
                self.process.join(timeout=1)
            if self.process.is_alive():
                self.process.kill()
                self.process.join(timeout=1)
        self.pipe.close()
        if hasattr(self, "temporary"):
            self.temporary.cleanup()


class Gateway(HTTPServer):
    allow_reuse_address = False
    request_queue_size = 4

    def __init__(self, port, worker, models):
        self.worker = worker
        self.models = models
        super().__init__(("127.0.0.1", port), Handler)

    def handle_error(self, request, client_address):
        # Unexpected transport errors must never print rejected source text.
        pass

    def get_request(self):
        socket, address = super().get_request()
        socket.settimeout(5)
        return socket, address


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("invalid_arguments: duplicate JSON property")
        result[key] = value
    return result


class Handler(BaseHTTPRequestHandler):
    server_version = "JobHarnessLocal/0.1"
    protocol_version = "HTTP/1.0"

    def log_message(self, format, *args):
        pass

    def respond(self, status, value):
        encoded = json.dumps(value, allow_nan=False, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(encoded)

    def trusted(self):
        expected = f"127.0.0.1:{self.server.server_port}"
        if self.headers.get("Origin") is not None or self.headers.get("Host") != expected:
            self.respond(403, {"error": {"code": "untrusted_origin", "message": "Use direct loopback requests with the bound host; browser origins are rejected"}})
            return False
        return True

    def do_GET(self):
        if not self.trusted():
            return
        if self.path == "/health":
            ready = self.server.worker.process.is_alive()
            self.respond(200 if ready else 503, {"status": "ready" if ready else "worker_stopped", "metadata": self.server.worker.metadata, "models": self.server.models})
        elif self.path == "/v1/models":
            self.respond(200, {"object": "list", "data": [{"object": "model", "id": name, "revision": revision} for name, revision in self.server.models.items()]})
        else:
            self.respond(404, {"error": {"code": "unsupported_route"}})

    def do_POST(self):
        if not self.trusted():
            return
        if self.path not in ROUTES:
            self.respond(404, {"error": {"code": "unsupported_route"}})
            return
        if self.headers.get_content_type() != "application/json" or self.headers.get("Transfer-Encoding") is not None:
            self.respond(415, {"error": {"code": "unsupported_media_type"}})
            return
        length = self.headers.get("Content-Length", "")
        if not length.isdecimal() or not 0 < int(length) <= MAX_BODY:
            self.respond(413, {"error": {"code": "request_limit"}})
            return
        try:
            raw = self.rfile.read(int(length))
            if len(raw) != int(length):
                raise ValueError("invalid_arguments: incomplete body")
            payload = json.loads(raw, object_pairs_hook=unique_object, parse_constant=lambda _: (_ for _ in ()).throw(ValueError("invalid_arguments: non-finite JSON numbers")))
            if not isinstance(payload, dict):
                raise ValueError("invalid_arguments: request must be a JSON object")
            validator = {"/v1/embeddings": embedding_payload, "/v1/decisions": decision_payload, "/v1/chat/completions": triage_payload}[self.path]
            # Validate selected inputs before IPC, so oversized/irrelevant JSON never
            # consumes the native worker or its deadline.
            validator(payload)
            result = self.server.worker.call(self.path, payload)
            self.respond(200, result)
        except (ValueError, UnicodeError, RecursionError) as exc:
            reason = str(exc)
            code = reason.split(":", 1)[0]
            if code not in SAFE_CODES | {"provider_timeout", "runtime_unavailable"}:
                reason, code = "invalid_arguments: supply valid bounded JSON", "invalid_arguments"
            self.respond(503 if code in {"provider_timeout", "runtime_unavailable", "model_inference_failed", "unsupported_runtime"} else 502 if code == "provider_schema_error" else 400, {"error": {"code": code, "message": reason}})
        except Exception:
            self.respond(500, {"error": {"code": "gateway_failure", "message": "Unexpected gateway failure; input and native diagnostics are suppressed"}})
