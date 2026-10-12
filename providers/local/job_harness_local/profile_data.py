"""Packaged projection of reviewed multi-artifact model profiles."""
from importlib.resources import files
import json


def profiles():
    return json.loads(files(__package__).joinpath("model_profiles.json").read_text())["profiles"]


def profile(identifier):
    for item in profiles():
        if item["id"] == identifier:
            return item
    raise ValueError("unknown_model: select a model from list")
