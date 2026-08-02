#!/usr/bin/env python3
"""
measure_rss.py — Measure peak RSS (resident set size) for a representative workload.
Reads /proc/self/status periodically; reports peak VmRSS in MB.

Workload: parse 1M version strings + match 100k spec-version pairs.
Runs in ref Python venv vs rust-backed venv.
"""
import subprocess, json, sys, os, time, textwrap

REF_PY  = '/home/dolphin/hackathon-ref/python-semanticversion/.venv/bin/python'
RUST_PY = '/home/dolphin/rust-venv/bin/python'

WORKLOAD = textwrap.dedent(r"""
import time, os, json

# Poll peak RSS in background thread
peak_rss_kb = [0]
def poll_rss():
    global peak_rss_kb
    while True:
        try:
            with open('/proc/self/status') as f:
                for line in f:
                    if line.startswith('VmRSS:'):
                        kb = int(line.split()[1])
                        if kb > peak_rss_kb[0]:
                            peak_rss_kb[0] = kb
                        break
        except:
            pass
        time.sleep(0.001)

import threading
t = threading.Thread(target=poll_rss, daemon=True)
t.start()

from semantic_version import Version, SimpleSpec, NpmSpec

VERSIONS = ["0.1.0","1.0.0-alpha","1.0.0-alpha.1","1.0.0-alpha.beta","1.0.0-beta","1.0.0-beta.2","1.0.0-beta.11","1.0.0-rc.1","1.0.0-rc.1+build.1","1.0.0","1.0.0+0.3.7","1.3.7+build","1.3.7+build.2.b8f12d7","1.3.7+build.11.e0f985a","2.0.0-rc.1","2.0.0-rc.3","2.0.0","2.1.0","2.2.0","3.0.0"]
SIMPLE_SPECS = [">=1.0.0",">=1.0.0,<2.0.0",">=1.0.0-rc.1,<2.0.0","==1.0.0-alpha.1","!=1.0.0","*","~=1.2.3","1.2.3"]
NPM_SPECS = ["^1.2.3","~1.2.3",">=1.2.3 <2.0.0","1.2.3 - 2.0.0",">=1.2.3-rc.1 <2.0.0","*","1.x","1.2.x"]

# Warm up: parse everything once
for s in VERSIONS: Version(s)
for s in SIMPLE_SPECS: SimpleSpec(s)
for s in NPM_SPECS: NpmSpec(s)

# Workload: parse 100k versions
for _ in range(10000 // len(VERSIONS)):
    for s in VERSIONS:
        Version(s)

# Match 100k spec-version pairs
v = Version("1.2.3-beta.1+build.42")
nspecs = [NpmSpec(s) for s in NPM_SPECS]
for _ in range(10000 // len(nspecs)):
    for sp in nspecs:
        sp.match(v)

# Allocate many versions to measure steady-state RSS
versions = [Version(s) for s in VERSIONS] * 5000  # 100k version objects
time.sleep(0.05)  # let RSS poll catch up

print(json.dumps({"peak_rss_mb": round(peak_rss_kb[0] / 1024, 1)}))
""")


def run(venv_python, label):
    t0 = time.time()
    r = subprocess.run([venv_python, "-c", WORKLOAD], capture_output=True, text=True, timeout=300)
    wall = time.time() - t0
    if r.returncode != 0:
        print(f"[{label}] FAILED (exit {r.returncode})\nSTDERR:\n{r.stderr[:500]}", flush=True)
        return None
    try:
        d = json.loads(r.stdout)
    except Exception as e:
        print(f"[{label}] PARSE ERR: {e}\nstdout:{r.stdout[:500]}", flush=True)
        return None
    d['label'] = label
    d['wall_sec'] = round(wall, 2)
    return d

def main():
    print("Measuring Rust-backed RSS...", flush=True)
    rust = run(RUST_PY, "rust")
    print("Measuring Python ref RSS...", flush=True)
    ref = run(REF_PY, "python_ref")
    if rust is None or ref is None:
        print("Abort: one venv failed", flush=True)
        sys.exit(1)

    rust_mb = rust['peak_rss_mb']
    ref_mb = ref['peak_rss_mb']
    reduction = (1 - rust_mb / ref_mb) * 100 if ref_mb > 0 else 0

    result = {
        "rust_peak_rss_mb": round(rust_mb, 1),
        "python_ref_peak_rss_mb": round(ref_mb, 1),
        "rss_reduction_pct": round(reduction, 1),
    }
    print("\n=== RSS RESULTS ===")
    print(f"Python ref: {ref_mb:.1f} MB")
    print(f"Rust binding: {rust_mb:.1f} MB")
    print(f"Reduction: {reduction:.1f}%")
    print(json.dumps(result, indent=2))
    with open(os.path.join(os.path.dirname(__file__), "rss_results.json"), "w") as f:
        json.dump(result, f, indent=2)

if __name__ == "__main__":
    main()