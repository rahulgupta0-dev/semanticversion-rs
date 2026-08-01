# Makefile — semanticversion-rs
# Port Mortem 2026, Track D (Python → Rust)

PYTHON ?= python3
VENV_PYTHON ?= .venv/bin/python
MATURIN ?= maturin

.PHONY: all build test test-original test-rust fuzz bench clean dev

all: build test

## Install Rust build into active venv (development)
dev:
	$(MATURIN) develop

## Full production build (wheel)
build:
	$(MATURIN) build --release

## Run ORIGINAL unmodified Python test suite against Rust build (judge's validation path)
## TODO: uncomment after PyO3 binding module is complete
test-original: dev
	# TODO: enable once PyO3 binding (module 8) is implemented
	# pytest tests/original/ -q --tb=short
	@echo "TODO: PyO3 binding not yet implemented — enable after module 8"

## Run native Rust unit tests
test-rust:
	cargo test

## Run both test suites
test: test-rust
	@echo "test-original: run 'make test-original' once PyO3 binding is complete"

## Differential fuzz (60+ seconds against Python oracle)
fuzz: dev
	$(PYTHON) fuzz/fuzz_driver.py --duration 60 --output fuzz/log.txt
	@tail -2 fuzz/log.txt

## Criterion benchmarks
bench:
	cargo bench

## Remove build artefacts
clean:
	cargo clean
	find . -name '*.pyc' -delete
	find . -name '__pycache__' -type d -exec rm -rf {} + 2>/dev/null || true
