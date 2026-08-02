#!/usr/bin/env python3
"""Differential fuzz driver for Python/Rust port of python-semanticversion.

Runs oracle.py in two venvs, diffs JSON, and logs results.

Usage:
python driver.py --time 60 [--n 200] [--out fuzz/log.txt] [--divergences fuzz/divergences.txt]
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import time

# ---------------------------------------------------------------------------
# Config - standard lib only
# ---------------------------------------------------------------------------
_ORACLE = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "oracle.py",
)
REF_PYTHON = "/home/dolphin/hackathon-ref/python-semanticversion/.venv/bin/python"
RUST_PYTHON = "/home/dolphin/rust-venv/bin/python"

VERSION_FIELDS = [
    "major", "minor", "patch", "prerelease", "build",
    "str", "repr", "partial", "valid", "compare",
]
SPEC_FIELDS = ["str", "repr", "clause_repr", "matches"]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def run_oracle_batch(python_exe, out_path, seed, n):
    """Run oracle.py in batch mode; return (rc, stderr)."""
    cmd = [
        python_exe, _ORACLE,
        "--seed", str(seed),
        "--n", str(n),
        "--out", out_path,
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    return r.returncode, r.stderr


def run_oracle_one(python_exe, kind, text):
    """Run oracle.py in single-input mode; return (rc, stdout, stderr)."""
    cmd = [python_exe, _ORACLE, "--one", kind, text]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    return r.returncode, r.stdout.strip(), r.stderr


def load_json(path):
    with open(path) as f:
        return json.load(f)


def diff_values(ref_res, rust_res, kind):
    """Return list of field names that differ between two ok results."""
    fields = VERSION_FIELDS if kind.startswith("version") else SPEC_FIELDS
    return [f for f in fields if ref_res.get(f) != rust_res.get(f)]


def normalize_hashes(results):
    """Replace each (kind, hash) with the index of its first appearance."""
    idx = {}
    out = []
    for entry in results:
        res = entry.get("result", {})
        if res.get("ok"):
            key = (entry.get("kind", ""), res.get("hash"))
            if key not in idx:
                idx[key] = len(idx)
            out.append(idx[key])
        else:
            out.append(None)
    return out


def compare_batches(ref_batch, rust_batch):
    """Compare two oracle batch outputs.

    Returns:
        hard      -- count of hard divergences
        soft      -- count of soft (same error type, different msg)
        soft_ex   -- list of up-to-5 (kind, text, ref_msg, rust_msg)
        first     -- human-readable description of first hard divergence
        anomalies -- list of (kind, text, anomaly_string)
    """
    ref = ref_batch.get("results", [])
    rust = rust_batch.get("results", [])
    if len(ref) != len(rust):
        return 1, 0, [], f"length: ref={len(ref)} rust={len(rust)}", []

    ref_norm = normalize_hashes(ref)
    rust_norm = normalize_hashes(rust)

    hard = soft = 0
    soft_ex, anomalies = [], []
    first_hard = None

    for i in range(len(ref)):
        ri = ref[i]
        ru = rust[i]

        kind = ri["kind"]
        text = ri["text"]
        rr = ri["result"]
        ru_res = ru["result"]

        rr_ok = rr.get("ok", False)
        ru_ok = ru_res.get("ok", False)

        # ok-bit differs
        if rr_ok != ru_ok:
            hard += 1
            if first_hard is None:
                first_hard = f"ok: ref={rr_ok} rust={ru_ok}"
            continue

        # both error
        if not rr_ok:
            rt = rr.get("error_type", "")
            rut = ru_res.get("error_type", "")
            if rt != rut:
                hard += 1
                if first_hard is None:
                    first_hard = f"error_type: ref={rt!r} rust={rut!r}"
            else:
                rm = rr.get("error_msg", "")
                rum = ru_res.get("error_msg", "")
                if rm != rum:
                    soft += 1
                    if len(soft_ex) < 5:
                        soft_ex.append((kind, text, rm, rum))
            continue

        # both ok
        diffs = diff_values(rr, ru_res, kind)
        if diffs:
            hard += 1
            if first_hard is None:
                first_hard = f"field {diffs[0]}: ref={str(rr.get(diffs[0]))[:80]} rust={str(ru_res.get(diffs[0]))[:80]}"
            continue

        # hash pattern
        if ref_norm[i] != rust_norm[i]:
            hard += 1
            if first_hard is None:
                first_hard = f"hash_pattern: ref_norm={ref_norm[i]} rust_norm={rust_norm[i]}"

        # anomalies
        for a in rr.get("anomalies", []):
            anomalies.append((kind, text, a))
        for a in ru_res.get("anomalies", []):
            anomalies.append((kind, text, a))

    return hard, soft, soft_ex, first_hard, anomalies


def shrink(kind, text, ref_py, rust_py, max_passes=16):
    """Shrink input by deleting at most max_passes chars, 1 per pass."""
    cur = text
    for _ in range(max_passes):
        if not cur:
            break
        new_cur = cur[:-1]  # delete last char (minimal mutation)
        _, ref_out, _ = run_oracle_one(ref_py, kind, new_cur)
        _, rust_out, _ = run_oracle_one(rust_py, kind, new_cur)
        ref_res = json.loads(ref_out)["result"]
        rust_res = json.loads(rust_out)["result"]
        if ref_res != rust_res:
            cur = new_cur
        else:
            break
    return cur


# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Differential fuzz driver")
    parser.add_argument("--time", type=int, default=60,
                        help="wall-time budget in seconds (default 60)")
    parser.add_argument("--n", type=int, default=200,
                        help="pairs per seed (default 200)")
    parser.add_argument("--out", type=str, default="fuzz/log.txt",
                        help="log file path (overwritten)")
    parser.add_argument("--divergences", type=str, default="fuzz/divergences.txt",
                        help="first divergence details path")
    args = parser.parse_args()

    out_path = args.out
    div_path = args.divergences
    tmp_ref_prefix = os.path.join(tempfile.gettempdir(), "diff_ref_")
    tmp_rust_prefix = os.path.join(tempfile.gettempdir(), "diff_rust_")

    total_hard = 0
    total_soft = 0
    all_anomalies = []
    divergence_written = False
    start = time.monotonic()
    seed = 0

    # Ensure output parent directories exist
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    os.makedirs(os.path.dirname(os.path.abspath(div_path)), exist_ok=True)

    with open(out_path, "w") as logf:
        logf.write("=== DIFFERENTIAL FUZZ ===\n")

        while True:
            elapsed = time.monotonic() - start
            if elapsed >= args.time:
                break

            tmp_ref = tempfile.mktemp(prefix=tmp_ref_prefix, suffix=f"_{seed}.json")
            tmp_rust = tempfile.mktemp(prefix=tmp_rust_prefix, suffix=f"_{seed}.json")

            try:
                rc_ref, stderr_ref = run_oracle_batch(REF_PYTHON, tmp_ref, seed, args.n)
                if rc_ref != 0:
                    msg = f"REF ORACLE CRASH seed={seed}: {stderr_ref}"
                    print(msg, file=sys.stderr)
                    logf.write(msg + "\n")
                    sys.exit(2)

                rc_rust, stderr_rust = run_oracle_batch(RUST_PYTHON, tmp_rust, seed, args.n)
                if rc_rust != 0:
                    msg = f"RUST ORACLE CRASH seed={seed}: {stderr_rust}"
                    print(msg, file=sys.stderr)
                    logf.write(msg + "\n")
                    sys.exit(2)

                ref_json = load_json(tmp_ref)
                rust_json = load_json(tmp_rust)

            finally:
                try:
                    os.unlink(tmp_ref)
                    os.unlink(tmp_rust)
                except OSError:
                    pass

            h, s, sex, fhd, anoms = compare_batches(ref_json, rust_json)
            total_hard += h
            total_soft += s
            all_anomalies.extend(anoms)

            ln = (f"seed={seed} pairs={args.n} div={h} soft={s}"
                  f" elapsed={elapsed:.1f}s")
            print(ln, flush=True)
            logf.write(ln + "\n")
            logf.flush()

            if h > 0 and not divergence_written:
                # first diverging entry
                ref_results = ref_json.get("results", [])
                rust_results = rust_json.get("results", [])
                div_kind = ref_results[0]["kind"]
                div_text = ref_results[0]["text"]
                minimal = shrink(div_kind, div_text, REF_PYTHON, RUST_PYTHON)
                _, ro, _ = run_oracle_one(REF_PYTHON, div_kind, minimal)
                _, uo, _ = run_oracle_one(RUST_PYTHON, div_kind, minimal)

                with open(div_path, "w") as df:
                    df.write(f"kind={div_kind}\n")
                    df.write(f"text={minimal}\n")
                    df.write(f"REF: {ro}\n")
                    df.write(f"RUST: {uo}\n")

                logf.write(
                    f"DIVERGENCE kind={div_kind} text={minimal!r} detail={fhd}\n"
                )
                divergence_written = True
                break

            seed += 1

    elapsed = time.monotonic() - start
    pairs = (seed + 1) * args.n if seed > 0 else args.n
    summary = (
        f"SUMMARY: duration={elapsed:.1f}s seeds={seed}"
        f" pairs={pairs}"
        f" hard_divergences={total_hard}"
        f" soft_msg_diffs={total_soft}"
        f" anomalies={len(all_anomalies)}"
        f" FUZZ_SURVIVOR={'true' if total_hard == 0 else 'false'}"
    )
    print(summary)
    with open(out_path, "a") as logf:
        logf.write(summary + "\n")
        if all_anomalies:
            logf.write("=== ANOMALIES ===\n")
            for k, t, a in sorted(all_anomalies)[:60]:
                logf.write(f"ANOMALY kind={k} anomaly={a} text={t!r}\n")

    sys.exit(0 if total_hard == 0 else 1)


if __name__ == "__main__":
    main()
