#!/usr/bin/env python3
"""Differential fuzz oracle for python-semanticversion.

Given (seed, N), deterministically generate N random inputs, evaluate each
against whatever `semantic_version` module is importable in the running venv,
and dump the results to a JSON file.  The SAME file is run in two venvs
(reference original vs Rust/PyO3 port); the driver diffs the two JSONs.

Usage:
    python oracle.py --seed S --n N --out PATH        # batch mode
    python oracle.py --one <kind> <text>               # single-input mode
"""
import argparse
import json
import random
import string
import sys
import warnings

# Do NOT manipulate sys.path — the venv provides `semantic_version`.
import semantic_version as sv

PROBES = ['0.0.0', '1.2.3', '1.2.3-rc.1', '1.2.3+build.7',
          '10.0.0', '0.1.0']

KINDS = ['version', 'simple_spec', 'npm_spec', 'legacy_spec']

# Fixed edge-case strings for version generator.
EDGE_VERSIONS = [
    '', 'garbage', '1.2.3-', '1.2.3+', '1.2.3-+b', '1.2.3-..', '1.2.3.',
    ' 1.2.3', '1.2.3 ', '1.2.3.4', '1.2', '1', 'v1.2.3', '0.0.0',
    '1.2.3-a..b', '+', '-', '1..2',
    '18446744073709551615.0.0',           # u64::MAX major
    '18446744073709551615.0.0-rc.1',
    '1.2.3-99999999999999999999',          # huge numeric prerelease
    '1.2.3+01', '1.2.3-00', '1.2.3-0',
]

JUNK_CHARS = string.digits + string.ascii_lowercase + '-+.^*~=<> ,|'


def gen_alpha(rng):
    chars = string.ascii_lowercase + string.digits
    return ''.join(rng.choice(chars) for _ in range(rng.randint(1, 6)))


def gen_version(rng, partial_mode):
    r = rng.random()
    if r < 0.45:
        # valid grammar-based
        if rng.random() < 0.30:
            major = rng.randint(10**15, 2**64 - 1)
        else:
            major = rng.randint(0, 10)
        minor = rng.randint(0, 20) if rng.random() < 0.80 else rng.randint(10**15, 2**64 - 1)
        patch = rng.randint(0, 100) if rng.random() < 0.80 else rng.randint(10**15, 2**64 - 1)
        prerel = ''
        if rng.random() < 0.30:
            parts = []
            for _ in range(rng.randint(1, 3)):
                if rng.random() < 0.60:
                    parts.append(str(rng.randint(0, 99)))
                else:
                    parts.append(gen_alpha(rng))
            if rng.random() < 0.20:
                # inject a leading-zero numeric identifier (invalid prerelease)
                parts.append('0' + str(rng.randint(1, 9)))
            prerel = '-' + '.'.join(parts)
        build = ''
        if rng.random() < 0.20:
            build = '+' + '.'.join(
                str(rng.randint(0, 999)) if rng.random() < 0.5
                else gen_alpha(rng)
                for _ in range(rng.randint(1, 2))
            )
        return f'{major}.{minor}.{patch}{prerel}{build}'
    if r < 0.60:
        if partial_mode:
            forms = ['{m}', '{m}.{mi}', '{m}.{mi}.{p}']
        else:
            forms = ['{m}', '{m}.{mi}', '{m}.{mi}.{p}']  # invalid for full parse
        m = rng.randint(0, 10)
        mi = rng.randint(0, 10)
        p = rng.randint(0, 10)
        s = rng.choice(forms).format(m=m, mi=mi, p=p)
        if rng.random() < 0.30:
            s += '-' + '.'.join(
                str(rng.randint(0, 99)) if rng.random() < 0.5
                else gen_alpha(rng)
                for _ in range(rng.randint(1, 2))
            )
        if rng.random() < 0.20:
            s += '+' + '.'.join(
                str(rng.randint(0, 99)) for _ in range(rng.randint(1, 2))
            )
        return s
    if r < 0.75:
        # leading-zero invalids
        return rng.choice([
            f'0{rng.randint(1,9)}.{rng.randint(0,9)}.{rng.randint(0,9)}',
            f'{rng.randint(1,9)}.0{rng.randint(1,9)}.{rng.randint(0,9)}',
            f'{rng.randint(1,9)}.{rng.randint(0,9)}.0{rng.randint(1,9)}',
            f'1.2.3-0{rng.randint(1,9)}',
            '1.2.3-00',
            '1.2.3-0a',
        ])
    if r < 0.95:
        return rng.choice(EDGE_VERSIONS)
    # pure junk
    return ''.join(rng.choice(JUNK_CHARS) for _ in range(rng.randint(0, 12)))


def gen_simple_spec(rng):
    r = rng.random()
    if r < 0.45:
        op = rng.choice(['', '=', '==', '!=', '<', '<=', '>', '>=', '^', '~', '~='])
        target = rng.choice([
            f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}',
            f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}',
            f'{rng.randint(0, 10**6)}',
            f'{rng.randint(0, 9)}.x',
            f'{rng.randint(0, 9)}.{rng.randint(0, 9)}.*',
            '0.1.*',
        ])
        return op + target
    if r < 0.55:
        return '*'
    if r < 0.70:
        blocks = []
        for _ in range(rng.randint(2, 3)):
            op = rng.choice(['>=', '<', '>', '<=', '==', '!=', '^', '~'])
            target = f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}'
            blocks.append(op + target)
        return ','.join(blocks)
    if r < 0.80:
        return rng.choice([
            '>=1.2.3-rc.1', '!=1.2.3-', '!=1.2.3+', '==1.2.3+build.5',
            '<=1.2.3-', '<1.2.3-rc.1', '>1.2.3-alpha.3',
        ])
    return rng.choice([
        '>=', '==1.x', '!=1.2.3-..', '1.2.3 - 2.3.4', '>=1.2.3 <2.0.0',
        '~1.x', '!0.1', '=1.2.3,', '1.2.3 ', 'garbage', '^',
    ])


def gen_npm_spec(rng):
    r = rng.random()
    if r < 0.45:
        op = rng.choice(['', '=', '==', '!=', '<', '<=', '>', '>=', '^', '~', '~='])
        target = rng.choice([
            f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}',
            f'{rng.randint(0, 10**6)}.x',
            f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}.x',
            f'{rng.randint(0, 10**6)}.*',
            f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}',
            '*',
            '^0.0.x',
            '~1.2.3',
        ])
        return op + target
    if r < 0.60:
        a_full = f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}'
        a = rng.choice([a_full, '.'.join(a_full.split('.')[:2]), a_full.split('.')[0]])
        b_full = f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}'
        b = rng.choice([b_full, '.'.join(b_full.split('.')[:2]), b_full.split('.')[0]])
        return f'{a} - {b}'
    if r < 0.75:
        blocks = []
        for _ in range(rng.randint(2, 3)):
            op = rng.choice(['>=', '<', '>', '<=', '^', '~', ''])
            target = f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}'
            blocks.append(op + target)
            if rng.random() < 0.3:
                op2 = rng.choice(['<', '>=', '<='])
                t2 = f'{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}.{rng.randint(0, 10**6)}'
                blocks[-1] += f' {op2}{t2}'
        return ' || '.join(blocks)
    if r < 0.85:
        return rng.choice([
            '>1.2.3-alpha.3', '>=1.2.3-0', '^1.2.3-rc.1',
            '1.2.3 - 2.3.4-rc.1', '~1.2.3+build',
        ])
    return rng.choice([
        '||', '1.2.3 ||', 'x', '1.2.3 -', '- 2.0.0',
        '>=1.2.3 <', '<', '^0.0.0-', 'garbage', '',
    ])


def gen_legacy_spec(rng):
    if rng.random() < 0.70:
        return gen_simple_spec(rng)
    blocks = [gen_simple_spec(rng) for _ in range(rng.randint(2, 2))]
    return ','.join(blocks)


def gen_input(rng, i):
    if i % 7 == 3:
        return ('version_partial', gen_version(rng, partial_mode=True))
    kind = KINDS[i % len(KINDS)]
    if kind == 'version':
        return (kind, gen_version(rng, partial_mode=False))
    if kind == 'simple_spec':
        return (kind, gen_simple_spec(rng))
    if kind == 'npm_spec':
        return (kind, gen_npm_spec(rng))
    if kind == 'legacy_spec':
        return (kind, gen_legacy_spec(rng))
    raise AssertionError(kind)


def evaluate(kind, text):
    """Return the result dict for a single input."""
    try:
        if kind == 'version':
            v = sv.Version(text)
            return _version_result(text, v, full=True)
        if kind == 'version_partial':
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", DeprecationWarning)
                v = sv.Version(text, partial=True)
            return _version_result(text, v, full=False)
        if kind == 'simple_spec':
            spec = sv.SimpleSpec(text)
            return _spec_result(text, spec)
        if kind == 'npm_spec':
            spec = sv.NpmSpec(text)
            return _spec_result(text, spec)
        if kind == 'legacy_spec':
            spec = sv.Spec(text)
            return _spec_result(text, spec)
    except Exception as e:
        return {"ok": False, "error_type": type(e).__name__, "error_msg": str(e)}


def _version_result(text, v, full):
    compare_list = []
    anomalies = []
    compare_list = []
    anomalies = []
    for p in PROBES:
        try:
            pv = sv.Version(p)
        except Exception:
            compare_list.append(None)
            continue
        try:
            cmp = sv.compare(text, p)
        except Exception:
            cmp = None
        if cmp is NotImplemented:
            compare_list.append("NotImplemented")
        elif isinstance(cmp, int):
            compare_list.append(cmp)
        else:
            compare_list.append(str(cmp))
        # ---- anomaly detection (replicated on both implementations) ----
        try:
            if cmp is not NotImplemented:
                if isinstance(cmp, int) and cmp != 0 and v == pv:
                    anomalies.append(f"eq_true_but_compare_conflicts:probe='{p}'")
            else:
                if v == pv:
                    anomalies.append(f"eq_true_but_compare_notimplemented:probe='{p}'")
        except Exception:
            pass
        try:
            if v == pv and hash(v) != hash(pv):
                anomalies.append(f"eq_true_hash_diff:probe='{p}'")
        except Exception:
            pass
    anomalies = list(dict.fromkeys(anomalies))  # dedup, preserve order
    return {
        "ok": True,
        "major": v.major,
        "minor": v.minor,
        "patch": v.patch,
        "prerelease": list(v.prerelease),
        "build": list(v.build),
        "str": str(v),
        "repr": repr(v),
        "hash": hash(v),
        "partial": getattr(v, 'partial', False),
        "valid": sv.validate(text) if full else None,
        "compare": compare_list,
        "anomalies": anomalies,
    }


def _spec_result(text, spec):
    matches = []
    for p in PROBES:
        try:
            pv = sv.Version(p)
            matches.append(bool(spec.match(pv)))
        except Exception:
            matches.append(None)
    return {
        "ok": True,
        "str": str(spec),
        "repr": repr(spec),
        "hash": hash(spec),
        "clause_repr": repr(spec.clause),
        "matches": matches,
        "anomalies": [],
    }


def run_batch(seed, n, out_path):
    rng = random.Random(seed)
    results = []
    for i in range(n):
        kind, text = gen_input(rng, i)
        try:
            result = evaluate(kind, text)
        except Exception as e:
            print(f"CRASH_ORACLE {kind} {text}", file=sys.stderr)
            result = {"ok": False, "error_type": "CRASH:" + type(e).__name__,
                      "error_msg": str(e)}
        results.append({"kind": kind, "text": text, "result": result})
    data = {"seed": seed, "n": n, "results": results}
    with open(out_path, 'w') as f:
        json.dump(data, f, sort_keys=True, indent=1)


def run_one(kind, text):
    try:
        result = evaluate(kind, text)
    except Exception as e:
        print(f"CRASH_ORACLE {kind} {text}", file=sys.stderr)
        result = {"ok": False, "error_type": "CRASH:" + type(e).__name__,
                  "error_msg": str(e)}
    print(json.dumps({"kind": kind, "text": text, "result": result},
                     sort_keys=True, indent=1))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--seed', type=int)
    parser.add_argument('--n', type=int)
    parser.add_argument('--out', type=str)
    parser.add_argument('--one', nargs=2, metavar=('KIND', 'TEXT'))
    args = parser.parse_args()
    if args.one:
        kind, text = args.one
        run_one(kind, text)
    else:
        if args.seed is None or args.n is None or args.out is None:
            parser.error("--seed, --n, --out required (or --one KIND TEXT)")
        run_batch(args.seed, args.n, args.out)


if __name__ == '__main__':
    main()
