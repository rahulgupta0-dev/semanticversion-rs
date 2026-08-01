# Makefile — semanticversion-rs
# Port Mortem 2026, Track D (Python → Rust)
#
# Default target (`make`): build the PyO3 extension into the venv and run the
# ORIGINAL unmodified python-semanticversion test suite against it.
# Exits non-zero if any test fails.

VENV ?= /home/dolphin/rust-venv
PYTHON ?= $(VENV)/bin/python
MATURIN ?= $(VENV)/bin/maturin
PYTEST ?= $(VENV)/bin/python -m pytest

.PHONY: all build test test-original test-rust fuzz bench clean dev

all: dev test-original

## Install Rust build into active venv (development)
dev:
	VIRTUAL_ENV=$(VENV) $(MATURIN) develop

## Full production build (wheel)
build:
	VIRTUAL_ENV=$(VENV) $(MATURIN) build --release

## Run ORIGINAL unmodified Python test suite against Rust build (judge's validation path)
test-original: dev
	$(PYTEST) tests/original/ -q

## Run native Rust unit tests
test-rust:
	cargo test

## Run both test suites
test: test-rust test-original

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
