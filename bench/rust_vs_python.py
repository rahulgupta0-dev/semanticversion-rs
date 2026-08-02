#!/usr/bin/env python3
"""
rust_vs_python.py — Run identical workloads in ref Python venv and rust-backed venv,
report speedup factor (rust time / python time).

Uses timeit for micro-benchmarks; runs single-process for fair comparison.
Python ref:  /home/dolphin/hackathon-ref/python-semanticversion/.venv/bin/python
Rust binding: /home/dolphin/rust-venv/bin/python
"""
import subprocess, sys, json, time, os

REF_PY  = '/home/dolphin/hackathon-ref/python-semanticversion/.venv/bin/python'
RUST_PY = '/home/dolphin/rust-venv/bin/python'

WORKLOAD = """
import json
from semantic_version import Version, SimpleSpec, NpmSpec, Spec

# -------------------- parse 100k version strings --------------------
VERSIONS = [
    "0.1.0", "1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta",
    "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0-rc.1+build.1", "1.0.0",
    "1.0.0+0.3.7", "1.3.7+build", "1.3.7+build.2.b8f12d7", "1.3.7+build.11.e0f985a",
    "2.0.0-rc.1", "2.0.0-rc.3", "2.0.0", "2.1.0", "2.2.0", "3.0.0",
]
SIMPLE_SPECS = [
    ">=1.0.0", ">=1.0.0,<2.0.0", ">=1.0.0-rc.1,<2.0.0", "==1.0.0-alpha.1",
    "!=1.0.0", "*", "~=1.2.3", "1.2.3",
]
NPM_SPECS = [
    "^1.2.3", "~1.2.3", ">=1.2.3 <2.0.0", "1.2.3 - 2.0.0",
    ">=1.2.3-rc.1 <2.0.0", "*", "1.x", "1.2.x",
]
N = 100000

t0 = time.time()
for _ in range(N // len(VERSIONS)):
    for s in VERSIONS:
        Version(s)
v_parse_time = time.time() - t0

t0 = time.time()
for _ in range(N // len(SIMPLE_SPECS)):
    for s in SIMPLE_SPECS:
        SimpleSpec(s)
s_parse_time = time.time() - t0

t0 = time.time()
for _ in range(N // len(NPM_SPECS)):
    for s in NPM_SPECS:
        NpmSpec(s)
n_parse_time = time.time() - t0

# --- match 100k (spec, version) pairs ---
ref_v = Version("1.2.3-beta.1+build.42")
nspecs = [NpmSpec(s) for s in NPM_SPECS]
t0 = time.time()
for _ in range(N // len(nspecs)):
    for sp in nspecs:
        sp.match(ref_v)
match_time = time.time() - t0

# --- precedence 100k comparisons ---
v1 = Version("1.3.7+build.2.b8f12d7")
v2 = Version("2.0.0-rc.1")
t0 = time.time()
for _ in range(N):
    v1.precendence() < v2  # invalid attribute name on purpose? version.py has precedence_key
key1 = v1.precedence_key
key2 = v2.precedence_key
t0 = time.time()
for _ in range(N):
    key1 = v1.precedence_key
    key2 = v2.precedence_key
    _ = key1 < key2
cmp_time = time.time() - t0

print(json.dumps({
    "v_parse_ms": round(v_parse_time * 1000, 2),
    "s_parse_ms": round(s_parse_time * 1000, 2),
    "n_parse_ms": round(n_parse_time * 1000, 2),
    "match_ms": round(match_time * 1000, 2),
    "cmp_ms": round(cmp_time * 1000, 2),
}))
"""

# Actually, fix the precedence benchmark
FIXED_WORKLOAD = WORKLOAD.replace(
    "v1.precence() < v2  # digits name on purpose! version.py has precedence_key\n# Actually = v1.precedence_key",
    "v1 = Version(\"1.3.7+build.2.b8f12d7\")\nv2 = Version(\"2.0.0-rc.1\")"
)

# There's a subtle bug in the generated script. Let me simplify:
CLEAN_WORKLOAD = r"""
import json, time
from semantic_version import Version, SimpleSpec, NpmSpec, compare

VERSIONS       = ["0.1.0","1.0.0-alpha","1.0.0-alpha.1","1.0.0-alpha.beta","1.0.0-beta","1.0.0-beta.2","1.0.0-beta.11","1.0.0-rc.1","1.0.0-rc.1+build.1","1.0.0","1.0.0+0.3.7","1.3.7+build","1.3.7+build.2.b8f12d7","1.3.7+build.11.e0f985a","2.0.0-rc.1","2.0.0-rc.3","2.0.0","2.1.0","2.2.0","3.0.0"]
SIMPLE_SPECS   = [">=1.0.0",">=1.0.0,<2.0.0",">=1.0.0-rc.1,<2.0.0","==1.0.0-alpha.1","!=1.0.0","*","~=1.2.3","1.2.3"]
NPM_SPECS      = ["^1.2.3","~1.2.3",">=1.2.3 <2.0.0","1.2.3 - 2.0.0",">=1.2.3-rc.1 <2.0.0","*","1.x","1.2.x"]
N              = 100000

t0 = time.time()
for _ in range(N // len(VERSIONS)):
    for s in VERSIONS:
        Version(s)
v_parse_ms = (time.time() - t0) * 1000

t0 = time.time()
for _ in range(N // len(SIMPLE_SPECS)):
    for s in SIMPLE_SPECS:
        SimpleSpec(s)
s_parse_ms = (time.time() - t0) * 1000

t0 = time.time()
for _ in range(N // len(NPM_SPECS)):
    for s in NPM_SPECS:
        NpmSpec(s)
n_parse_ms = (time.time() - t0) * 1000

ref_v = Version("1.2.3-beta.1+build.42")
nspecs = [NpmSpec(s) for s in NPM_SPECS]
t0 = time.time()
for _ in range(N // len(nspecs)):
    for sp in nspecs:
        sp.match(ref_v)
match_ms = (time.time() - t0) * 1000

v1 = Version("1.3.7+build.2.b8f12d7")
v2 = Version("2.0.0-rc.1")
t0 = time.time()
for _ in range(N):
    _ = v1.precedence_key < v2.precedence_key
cmp_ms = (time.time() - t0) * 1000

print(json.dumps({
    "v_parse_ms": round(v_parse_ms, 2),
    "s_parse_ms": round(s_parse_ms, 2),
    "n_parse_ms": round(n_parse_ms, 2),
    "match_ms": round(match_ms, 2),
    "cmp_ms": round(cmp_ms, 2),
}))
"""

def run(venv_python, label):
    t0 = time.time()
    r = subprocess.run([venv_python, "-c", CLEAN_WORKLOAD], capture_output=True, text=True, timeout=300)
    wall = time.time() - t0
    if r.returncode != 0:
        print(f"[{label}] FAILED (exit {r.returncode})]\nSTDERR:\n{r.stderr[:500]}", flush=True)
        return None
    try:
        d = json.loads(r.stdout)
    except Exception as e:
        print(f"[{label}] PARSE ERR: {e}\nstdout:{r.stdout[:500]}", flush=True)
        return None
    d['wall_sec'] = round(wall, 2)
    d['label'] = label
    return d

def main():
    print("Running Rust-backed benchmark...", flush=True)
    rust = run(RUST_PY, "rust")
    print("Running Python reference benchmark...", flush=True)
    ref = run(REF_PY, "python_ref")
    if rust is None or ref is None:
        print("Abort: one venv failed", flush=True)
        sys.exit(1)

    speedups = {}
    for k in ['v_parse_ms','s_parse_ms','n_parse_ms','match_ms','cmp_ms']:
        speedups[f"{k}_speedup"] = round(ref[k] / rust[k], 2) if rust[k] > 0 else 0

    result = {
        "rust": rust,
        "python_ref": ref,
        "speedup": speedups,
        "aggregate_speedup": round(
            sum(ref[k] for k in ['v_parse_ms','s_parse_ms','n_parse_ms','match_ms','cmp_ms'])
            / max(1, sum(rust[k] for k in ['v_parse_ms','s_parse_ms','n_parse_ms','match_ms','cmp_ms'])),
            2,
        ),
    }
    print("\n=== RESULTS ===")
    print(json.dumps(result, indent=2))
    with open(os.path.join(os.path.dirname(__file__), "speedup.json"), "w") as f:
        json.dump(result, f, indent=2)

if __name__ == "__main__":
    main()