# Benchmark Methodology — semanticversion-rs

Port Mortem 2026, Track D | Module 11

---

## Hardware

- **CPU**: Intel Xeon @ 2.20GHz, 2 cores (cloud VM)
- **RAM**: 15 GiB (4.4 GiB available at benchmark time)
- **OS**: Debian 12 (Linux 6.1.0-51-cloud-amd64, x86_64)
- **Rust**: 1.96.0 (2026-05-25), release profile (`opt-level=3`)
- **Python**: 3.11.2 (CPython)
- **Note**: Hackathon cloud machine — not a bare-metal bench rig. Results are honest but not micro-benchmark-precision. Measurements are single-run; no outlier sanitization beyond criterion's built-in warmup + bootstrapping.

---

## Method

### 1. Criterion micro-benchmarks (native Rust core, no PyO3)
- **Tool:** `criterion` =="0.5.1"
- **Location:** `benches/criterion_bench.rs`
- **Profile:** `opt-level=3`, `lto=fat`
- **Method:** per function a closure batch (N items); criterion divides by item count to report per-element times. Reported: p50, p99, throughput (ops/sec, element-level).
- **Functions:** `Version::parse` (20 strings), `SimpleSpec::parse` (8), `NpmSpec::parse` (8), `match_version` (npm specs matched against one version, 7), `precedence_lt` / `precedence_gt` single pair.

### 2. Rust-vs-Python speedup
- **Script:** `bench/rust_vs_python.py`
- **Method:** One Python process, imports from either `../home/dolphin/rust-venv/bin/python` (PyO3 binding) or `../home/dolphin/hackathon-ref/python-semanticversion/.venv/bin/python` (reference). Performs an identical workload: parse 100k versions; 100k SimpleSpec; 100k NpmSpec; 100k match calls; 100k precedence_key comparisons. Wall clock via `time.time()` wrapping the entire iteration. Reported: speedup factor (Python / Rust).
- **Note on comparison benchmark:** The `precedence_key` access in the PyO3 binding incurs Python-tuple construction overhead and is NOT reflective of raw comparison throughput (the native `precedence_lt` runs at ~386ns p50 in criterion). Report includes both: native comparison speeds (~2.3M ops/s), and Python-level key-based comparison speedup (0.27× — slower due to binding overhead).

### 3. Peak RSS
- **Script:** `bench/measure_rss.py`
- **Method:** Polls `/proc/self/status` every 1ms while parsing 100k versions, matching 100k spec-version pairs, and allocating 100k live `Version` objects. Reported: peak VmRSS converted to MB. No RSS sampling on Rust process alone; both measurements include the full Python process weight (interpreter + binding).

---

## Data Files
- `bench/criterion_results.json` — p50, p99, thrpt per-operation (native Rust)
- `bench/speedup.json` — Rust-vs-Python timing
- `bench/rss_results.json` — peak RSS Rust vs Python