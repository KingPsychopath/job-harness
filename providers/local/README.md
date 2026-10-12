# Optional local model provider

Real EmbeddingGemma 2 text retrieval through LiteRT-LM and Laya typed decisions through MediaPipe, with a separate optional Kev MLX environment. All stay separate from the Rust core. Models never download during import or service startup.

See [lifecycle, model metadata, limits and measured verification](../../docs/MODELS.md). `requirements-runtime.txt` pins the optional SDK environment tested on Python 3.12 / macOS arm64. The Kev environment and pinned author SDK are listed separately in `requirements-runtime-kev.txt`. The package itself has no mandatory third-party dependencies; metadata commands work before installing the optional runtime.

```sh
PYTHONPATH=providers/local python3 -m job_harness_local list
PYTHONPATH=providers/local python3 -m unittest discover -s providers/local/tests -v
```
