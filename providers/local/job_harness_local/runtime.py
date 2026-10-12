"""Thin official SDK adapters. No generation, credentials, or workspace access."""
from __future__ import annotations

from collections import OrderedDict
import json
import math
import re
try:
    import resource
except ImportError:
    resource = None
import sys
import time
import zipfile

from .catalog import model


def text(value, bound=8192):
    try:
        encoded = value.encode("utf-8") if isinstance(value, str) else b""
    except UnicodeError:
        raise ValueError("invalid_arguments: input must be valid UTF-8") from None
    if not isinstance(value, str) or not value.strip() or len(encoded) > bound or "\0" in value:
        raise ValueError("invalid_arguments: supply bounded nonempty UTF-8 text without NUL")
    return value


def number(value, low=0.0, high=1.0):
    if isinstance(value, bool) or not isinstance(value, (float, int)) or not math.isfinite(value) or not low <= value <= high:
        raise ValueError("invalid_arguments: numeric value outside supported range")
    return float(value)


def validate_question(question):
    if not isinstance(question, dict):
        raise ValueError("invalid_arguments: each question must be an object")
    kind = question.get("type")
    if kind not in ("choice", "boolean", "nullable_boolean", "score"):
        raise ValueError("unsupported_question: choose choice, boolean, nullable_boolean or score")
    allowed = {"type"} | {"choice": {"instructions", "criteria"}, "score": {"instructions", "rubric"}, "boolean": {"condition", "threshold"}, "nullable_boolean": {"condition", "lower", "upper"}}[kind]
    if question.keys() - allowed:
        raise ValueError("invalid_arguments: unsupported question fields")
    q = dict(question)
    if kind == "choice":
        criteria = q.get("criteria")
        if not isinstance(criteria, dict) or not 2 <= len(criteria) <= 8:
            raise ValueError("invalid_arguments: choice requires 2–8 described options")
        for key, description in criteria.items():
            text(key, 64)
            text(description, 512)
        q["instructions"] = text(q.get("instructions", "Select the best supported option."), 512)
    elif kind == "score":
        rubric = q.get("rubric")
        if not isinstance(rubric, list) or not 2 <= len(rubric) <= 8:
            raise ValueError("invalid_arguments: score requires 2–8 ordered descriptions")
        for level in rubric:
            text(level, 512)
        q["instructions"] = text(q.get("instructions", "Evaluate the supplied rubric."), 512)
    else:
        q["condition"] = text(q.get("condition"), 512)
        if kind == "nullable_boolean":
            q["lower"] = number(q.get("lower", 0.2))
            q["upper"] = number(q.get("upper", 0.8))
            if q["lower"] >= q["upper"]:
                raise ValueError("invalid_arguments: lower must be less than upper")
        else:
            q["threshold"] = number(q.get("threshold", 0.5))
    return q


def decision_payload(payload):
    if not isinstance(payload, dict) or payload.keys() - {"model", "state", "questions"}:
        raise ValueError("invalid_arguments: decisions accept model, state and questions")
    state = text(payload.get("state"), 8192)
    questions = payload.get("questions")
    if not isinstance(questions, dict) or not 1 <= len(questions) <= 8:
        raise ValueError("invalid_arguments: supply 1–8 typed questions")
    checked = {}
    for key, value in questions.items():
        checked[text(key, 64)] = validate_question(value)
    return state, checked


def embedding_payload(payload):
    if not isinstance(payload, dict) or payload.keys() - {"model", "input", "encoding_format", "dimensions"}:
        raise ValueError("invalid_arguments: unsupported embedding request fields")
    if payload.get("encoding_format", "float") != "float" or payload.get("dimensions", 768) != 768:
        raise ValueError("unsupported_format: use 768-dimensional float embeddings")
    rows = payload.get("input")
    if isinstance(rows, str):
        rows = [rows]
    if not isinstance(rows, list) or not 1 <= len(rows) <= 32:
        raise ValueError("invalid_arguments: supply 1–32 text strings")
    for row in rows:
        text(row, 32768)
    if sum(len(row.encode()) for row in rows) > 512 * 1024:
        raise ValueError("context_budget: selected input exceeds 512 KiB")
    return rows


def triage_payload(payload):
    if not isinstance(payload, dict) or payload.keys() - {"model", "messages", "response_format", "stream"}:
        raise ValueError("unsupported_generation: this endpoint supports typed triage only")
    messages = payload.get("messages")
    if payload.get("stream", False) is not False or payload.get("response_format") != {"type": "json_object"} or not isinstance(messages, list) or len(messages) != 2:
        raise ValueError("unsupported_generation: no free-form generation, streaming or tool calls")
    if not all(isinstance(m, dict) and set(m) == {"role", "content"} for m in messages) or [m["role"] for m in messages] != ["system", "user"]:
        raise ValueError("unsupported_generation: expected fixed system/user triage request")
    system = text(messages[0]["content"], 4096)
    if not all(key in system for key in ["label", "needs_reply", "urgency", "uncertain", "rationale"]):
        raise ValueError("unsupported_generation: typed triage fields required")
    try:
        selected = json.loads(text(messages[1]["content"], 16384))
    except ValueError:
        raise ValueError("unsupported_generation: triage requires a selected JSON text field") from None
    if not isinstance(selected, dict) or set(selected) != {"text"}:
        raise ValueError("unsupported_generation: triage requires exactly one selected text field")
    return text(selected["text"], 8192)


def triage_consistency(state, results):
    label, urgency = results["label"]["value"], results["urgency"]["value"]
    original_reply = results["needs_reply"]["value"]
    lower = re.sub(r"\S+://\S+", " ", state.lower())
    request_found = any(term in lower for term in ("please reply", "let me know", "availability", "choose a time", "pick a slot", "booking link", "complete the test", "complete the assessment", "schedule a call", "are you available"))
    action_label = label in {"scheduling", "assessment"}
    needs_reply = True if request_found or action_label else original_reply
    uncertain = (results["needs_reply"]["abstained"] or results["label"]["confidence"] < 0.75 or results["urgency"]["confidence"] < 0.75 or needs_reply != original_reply or (needs_reply is True and not action_label) or (needs_reply is True and urgency == "normal"))
    if needs_reply is True and urgency == "normal":
        urgency = "review"
    return {"label": label, "needs_reply": needs_reply, "urgency": urgency, "uncertain": uncertain, "rationale": "Typed model proposal; inspect the source before acting.", "probabilities": results, "request_guard": request_found}


def peak_rss_bytes():
    if resource is None:
        return None
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return int(rss if sys.platform == "darwin" else rss * 1024)


class Runtime:
    def __init__(self, artifacts: dict[str, str], threads: int):
        self.decision_id = "laya-s256"
        self.embedder = None
        self.maker = None
        self.tokenizer = None
        self.prewarmed = OrderedDict()
        self.load_ms = {}
        if "embeddinggemma2-text" in artifacts:
            started = time.monotonic()
            import litert_lm
            self.embedding_api = litert_lm
            litert_lm.set_min_log_severity(litert_lm.LogSeverity.SILENT)
            self.embedder = litert_lm.EmbeddingEngine(artifacts["embeddinggemma2-text"], backend=litert_lm.Backend.CPU(thread_count=threads), max_input_length=2048)
            self.load_ms["embeddinggemma2-text"] = round((time.monotonic() - started) * 1000, 3)
        if "laya-s256" in artifacts:
            started = time.monotonic()
            from mediapipe.tasks.python.core import base_options
            from mediapipe.tasks.python.decision import decision_maker
            from tokenizers import Tokenizer
            self.decision_api = decision_maker
            self.maker = decision_maker.DecisionMaker.create_from_options(decision_maker.DecisionMakerOptions(base_options=base_options.BaseOptions(model_asset_path=artifacts["laya-s256"], delegate=base_options.BaseOptions.Delegate.CPU), max_num_tokens=256))
            with zipfile.ZipFile(artifacts["laya-s256"]) as bundle:
                if bundle.getinfo("tokenizer.json").file_size > 40 * 1024 * 1024:
                    raise ValueError("model_integrity_error: unexpected tokenizer size")
                self.tokenizer = Tokenizer.from_str(bundle.read("tokenizer.json").decode())
            self.tokenizer.no_truncation()
            self.tokenizer.no_padding()
            self.load_ms["laya-s256"] = round((time.monotonic() - started) * 1000, 3)

    def checked(self, result):
        try:
            number(result["confidence"])
            if "probability_true" in result:
                number(result["probability_true"])
            if "probabilities" in result:
                probabilities = result["probabilities"]
                values = list(probabilities.values()) if isinstance(probabilities, dict) else probabilities
                if not values or abs(sum(number(value) for value in values) - 1.0) > 0.001:
                    raise ValueError("invalid distribution")
            if result["type"] == "score":
                number(result["value"], 0, result["scale"]["maximum"])
        except (ValueError, KeyError, TypeError):
            raise ValueError("provider_schema_error: native decision result violates its typed contract") from None
        return result

    def question(self, state, q):
        # Reserve a conservative prompt allowance and refuse oversized context/schema.
        # This is admission control, not a reconstruction of the model's tokenizer/prompt runtime.
        tokens = len(self.tokenizer.encode(state + " " + json.dumps(q, ensure_ascii=False), add_special_tokens=True).ids) + 32
        if tokens > 256:
            raise ValueError("context_budget: Laya state plus question exceeds the conservative 256-token budget; select a shorter excerpt or fewer options")
        api = self.decision_api
        kind = q["type"]
        if kind == "choice":
            key = json.dumps(q, sort_keys=True)
            question = self.prewarmed.get(key)
            if question is None:
                question = api.ChoiceQuestion(criteria=q["criteria"], instructions=q["instructions"])
                self.maker.prewarm_choice(question)
                self.prewarmed[key] = question
                if len(self.prewarmed) > 16:
                    self.prewarmed.popitem(last=False)
            else:
                self.prewarmed.move_to_end(key)
            result = self.maker.evaluate_choice(state, question)
            if result.selected_key not in q["criteria"] or set(result.probabilities) != set(q["criteria"]):
                raise ValueError("provider_schema_error: native choice did not preserve option keys")
            return self.checked({"type": kind, "value": result.selected_key, "probabilities": result.probabilities, "confidence": result.confidence})
        if kind in ("boolean", "nullable_boolean"):
            result = self.maker.evaluate_boolean(state, api.BooleanQuestion(condition=q["condition"], threshold=q.get("threshold", 0.5)))
            p = result.probability_true
            value = result.value if kind == "boolean" else True if p >= q["upper"] else False if p <= q["lower"] else None
            if not isinstance(result.value, bool):
                raise ValueError("provider_schema_error: native boolean result is not typed")
            return self.checked({"type": kind, "value": value, "probability_true": p, "confidence": result.confidence, "abstained": value is None})
        result = self.maker.evaluate_score(state, api.ScoreQuestion(rubric=q["rubric"], instructions=q["instructions"]))
        if len(result.probabilities) != len(q["rubric"]) or abs(sum(i * probability for i, probability in enumerate(result.probabilities)) - result.expected_score) > 0.001:
            raise ValueError("provider_schema_error: native rubric probability count differs")
        return self.checked({"type": kind, "value": result.expected_score, "level": result.selected_key, "probabilities": result.probabilities, "confidence": result.confidence, "scale": {"minimum": 0, "maximum": len(q["rubric"]) - 1}})

    def decide(self, payload):
        if self.maker is None:
            raise ValueError("model_not_loaded: start serve with --decision-model laya-s256")
        state, questions = decision_payload(payload)
        return {"model": self.decision_id, "results": {name: self.question(state, q) for name, q in questions.items()}, "action": "suggestion_only", "canonical_changes": False}

    def embeddings(self, payload):
        if self.embedder is None:
            raise ValueError("model_not_loaded: start serve with --embedding-model embeddinggemma2-text")
        rows = embedding_payload(payload)
        api = self.embedding_api
        options = api.EmbeddingOptions(normalize=True, insert_special_tokens=True, input_overflow_strategy=api.InputOverflowStrategy.ERROR)
        vectors = self.embedder.compute_embedding_batch(rows, options=options)
        if len(vectors) != len(rows) or any(len(v.embedding) != 768 or not all(math.isfinite(n) for n in v.embedding) or abs(sum(n * n for n in v.embedding) - 1.0) > 0.01 for v in vectors):
            raise ValueError("provider_schema_error: invalid native embedding result")
        return {"object": "list", "model": "embeddinggemma2-text", "data": [{"object": "embedding", "index": i, "embedding": v.embedding} for i, v in enumerate(vectors)]}

    def triage(self, payload):
        state = triage_payload(payload)
        questions = {
            "label": {"type": "choice", "instructions": "Classify the message's main purpose.", "criteria": {"receipt": "Acknowledges receiving a job application.", "rejection": "Rejects the candidate or ends the hiring process.", "scheduling": "Requests interview availability or choosing a meeting time.", "assessment": "Requests completing a hiring test or assessment.", "other": "Other or unclear hiring message."}},
            "needs_reply": {"type": "nullable_boolean", "condition": "The message requests a reply, availability, booking or completing a task."},
            "urgency": {"type": "choice", "instructions": "Choose the review priority.", "criteria": {"normal": "Informational update with no requested action.", "review": "Requests a reply or action without an immediate deadline.", "urgent": "Action required today or within the next twenty-four hours."}},
        }
        results = self.decide({"state": state, "questions": questions})["results"]
        decision = triage_consistency(state, results)
        return {"object": "chat.completion", "model": self.decision_id, "choices": [{"index": 0, "message": {"role": "assistant", "content": json.dumps(decision, allow_nan=False)}, "finish_reason": "stop"}]}

    def call(self, route, payload):
        expected = "embeddinggemma2-text" if route == "/v1/embeddings" else self.decision_id
        if payload.get("model") != expected:
            raise ValueError("unknown_model: request the explicitly selected model id")
        started = time.monotonic()
        methods = {"/v1/embeddings": self.embeddings, "/v1/decisions": self.decide, "/v1/chat/completions": self.triage}
        if route not in methods:
            raise ValueError("unsupported_route: select embeddings or typed decisions")
        result = methods[route](payload)
        result["model_revision"] = model(expected)["revision"]
        result["metrics"] = {"elapsed_ms": round((time.monotonic() - started) * 1000, 3), "worker_peak_rss_bytes": peak_rss_bytes()}
        # json serialization rejects non-finite native scores before any caller receives them.
        json.dumps(result, allow_nan=False)
        return result

    def close(self):
        if self.maker:
            self.maker.close()
        if self.embedder:
            self.embedder.close()
