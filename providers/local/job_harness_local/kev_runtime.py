"""Optional author-supported Kev MLX backend; SDKs imported only when selected."""
import json
from pathlib import Path
import time

from .profile_data import profile
from .runtime import Runtime, decision_payload


class KevRuntime(Runtime):
    def __init__(self, artifacts, threads):
        remaining = {name: path for name, path in artifacts.items() if name != "kev-0.8b"}
        super().__init__(remaining, threads)
        self.decision_id = "kev-0.8b"
        started = time.monotonic()
        import torch
        import mlx.core as mx
        from kev.checkpoint import Meta
        from kev.model import load_tokenizer, pad_id
        from kev.mlx_model import load_base, merge_lora, MLXDecisionModel
        from kev.api import SystemOneRequest, to_record, choice_confidence, score_confidence
        self.mx = mx
        self.request_type, self.to_record = SystemOneRequest, to_record
        self.choice_confidence, self.score_confidence = choice_confidence, score_confidence
        torch.set_num_threads(threads)
        root = Path(artifacts["kev-0.8b"])
        spec = profile("kev-0.8b")
        expected_base = next(component for component in spec["components"] if component["role"] == "base")
        # Use the SDK's Meta and MLX functions, with an explicit safe tensor-only head load.
        meta = Meta.from_dict(torch.load(root / "adapter" / "head.pt", map_location="cpu", weights_only=True))
        if meta.base != expected_base["repository"] or meta.base_revision != expected_base["revision"]:
            raise ValueError("model_integrity_error: Kev head does not name the reviewed base revision")
        self.tokenizer = load_tokenizer(str(root / "base"))
        backbone = load_base(root / "base")
        merge_lora(backbone, root / "adapter")
        self.kev_model = MLXDecisionModel(backbone, pad_id(self.tokenizer), head_dim=meta.head_dim)
        self.kev_model.head.load_state_dict(meta.head)
        self.kev_model.head.temperature = meta.temperature
        self.kev_model.eval()
        self.maker = self.kev_model
        self.load_ms[self.decision_id] = round((time.monotonic() - started) * 1000, 3)

    def decide(self, payload):
        state, questions = decision_payload(payload)
        translated = {}
        for name, question in questions.items():
            kind = question["type"]
            if kind == "choice":
                translated[name] = {"type": "choice", "instructions": question["instructions"], "criteria": question["criteria"]}
            elif kind == "score":
                translated[name] = {"type": "score", "instructions": question["instructions"], "criteria": question["rubric"]}
            else:
                translated[name] = {"type": "noul", "instructions": question["condition"]}
        try:
            native_request = self.request_type(state=state, questions=translated)
        except ValueError:
            raise ValueError("invalid_arguments: selected questions do not satisfy the pinned Kev SDK schema") from None
        record, metadata = self.to_record(native_request)
        try:
            # Stay within the author's validated 8k state limit and never truncate.
            encoded = self.kev_model.encode(self.tokenizer, record, max_state=8192, max_branch=16384, strict=True)
        except ValueError:
            raise ValueError("context_budget: Kev state or question exceeds the selected strict token budget") from None
        probabilities = self.kev_model.probs(encoded)
        self.mx.synchronize()
        if len(probabilities) != len(questions) or len(metadata) != len(questions):
            raise ValueError("provider_schema_error: Kev returned an unexpected question count")
        results = {}
        for (name, question), description, values in zip(questions.items(), metadata, probabilities):
            self.validate_metadata(name, question, description)
            p = [float(value) for value in values]
            if len(p) != len(description["keys"]):
                raise ValueError("provider_schema_error: Kev probability count differs from supplied options")
            kind = question["type"]
            if kind == "choice":
                keys = description["keys"]
                result = {"type": kind, "value": keys[max(range(len(p)), key=lambda i: p[i])], "probabilities": dict(zip(keys, p)), "confidence": self.choice_confidence(p)}
            elif kind == "score":
                result = {"type": kind, "value": sum(i * value for i, value in enumerate(p)), "level": str(max(range(len(p)), key=lambda i: p[i])), "probabilities": p, "confidence": self.score_confidence(p), "scale": {"minimum": 0, "maximum": len(p) - 1}}
            else:
                if len(p) != 2:
                    raise ValueError("provider_schema_error: Kev boolean requires two probabilities")
                probability_true = p[1]
                if kind == "nullable_boolean":
                    value = True if probability_true >= question["upper"] else False if probability_true <= question["lower"] else None
                else:
                    value = probability_true >= question["threshold"]
                result = {"type": kind, "value": value, "probability_true": probability_true, "confidence": abs(2 * probability_true - 1), "abstained": value is None}
            results[name] = self.checked(result)
        return {"model": self.decision_id, "results": results, "action": "suggestion_only", "canonical_changes": False, "runtime_metrics": {"mlx_process_peak_bytes": self.mx.get_peak_memory(), "memory_scope": "worker lifetime; MLX allocation counter is separate from process RSS", "state_tokens": encoded["state_tokens"], "state_truncated": encoded["state_truncated"]}}

    def validate_metadata(self, name, question, description):
        kind = question["type"]
        expected_type = kind if kind in {"choice", "score"} else "noul"
        keys = description.get("keys")
        valid = description.get("id") == name and description.get("type") == expected_type
        if kind == "choice":
            valid = valid and isinstance(keys, list) and len(keys) == len(question["criteria"]) and set(keys) == set(question["criteria"])
        elif kind == "score":
            valid = valid and keys == [str(i) for i in range(len(question["rubric"]))]
        else:
            # `noul` is the author's actual typed API literal, with explicit polarity.
            valid = valid and keys == ["false", "true"]
        if not valid:
            raise ValueError("provider_schema_error: Kev SDK metadata does not preserve supplied question identities, option keys or boolean polarity")

    def close(self):
        self.maker = None
        self.kev_model = None
        if hasattr(self, "mx"):
            self.mx.clear_cache()
        super().close()
