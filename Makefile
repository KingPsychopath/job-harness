PYTHON ?= python3
.PHONY: check test check-parity check-package assets build demo

test:
	cargo test --workspace --locked

check: test
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	$(PYTHON) scripts/check_repository.py
	$(PYTHON) scripts/generate_contracts.py --check
	PYTHONPATH=providers/local $(PYTHON) -m unittest discover -s providers/local/tests -v

check-parity:
	cargo build --locked
	$(PYTHON) scripts/check_parity.py

build:
	cargo build --release --locked

check-package: build
	$(PYTHON) scripts/verify_native.py --binary target/release/job-harness

assets:
	$(PYTHON) scripts/render_assets.py

demo:
	cargo build --locked
	JOB_HARNESS_BINARY="$$(pwd)/target/debug/job-harness" ./scripts/demo.sh
