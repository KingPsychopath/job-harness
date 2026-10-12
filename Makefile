CARGO ?= cargo
.PHONY: check test check-parity check-package check-models check-desktop assets build demo

test:
	$(CARGO) test --workspace --all-targets --locked

check: test
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings
	$(CARGO) run -p job-harness-dev --locked -- repository
	$(CARGO) run -p job-harness-dev --locked -- contracts --check

# Native compatibility vectors are retained after the completed cross-language migration.
check-parity:
	$(CARGO) test -p job-harness-domain --locked
	$(CARGO) test -p job-harness-core --locked

build:
	$(CARGO) build --release --bins --locked

check-package: build
	$(CARGO) run -p job-harness-dev --locked -- package --binary target/release/job-harness

check-models:
	$(CARGO) fmt --manifest-path providers/local/native/Cargo.toml -- --check
	$(CARGO) clippy --manifest-path providers/local/native/Cargo.toml --all-targets --locked -- -D warnings
	$(CARGO) test --manifest-path providers/local/native/Cargo.toml --locked

check-desktop:
	$(CARGO) fmt --manifest-path apps/desktop/Cargo.toml -- --check
	$(CARGO) clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
	$(CARGO) test --manifest-path apps/desktop/Cargo.toml --locked

# SVG source is edited directly. Rendering is an optional local preview.
assets:
	mkdir -p .scratch
	rsvg-convert docs/assets/hero.svg -o .scratch/hero.png
	rsvg-convert docs/assets/architecture.svg -o .scratch/architecture.png

demo:
	$(CARGO) build --locked
	JOB_HARNESS_BINARY="$$(pwd)/target/debug/job-harness" ./scripts/demo.sh
