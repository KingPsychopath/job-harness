PYTHON ?= python3
.PHONY: check test check-mcp check-package assets build

test:
	$(PYTHON) -m unittest discover -s tests -p 'test_*.py'

check-mcp:
	$(PYTHON) scripts/verify_job_harness_mcp.py

check: test check-mcp
	$(PYTHON) -m ruff check job_harness tests scripts
	$(PYTHON) scripts/check_repository.py

build:
	$(PYTHON) -c "import shutil; shutil.rmtree('dist', ignore_errors=True)"
	$(PYTHON) -m build

check-package: build
	$(PYTHON) scripts/check_dist.py
	$(PYTHON) scripts/verify_package.py

assets:
	$(PYTHON) scripts/render_assets.py
