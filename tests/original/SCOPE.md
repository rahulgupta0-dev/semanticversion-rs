# SCOPE — Out-of-Scope Components

This file is hashed along with the test files but is excluded from parity counting.

## Excluded: `test_django.py` (16 tests) and `django_fields.py`

**Reason:** Django is a Python-only web ORM framework. Porting `django_fields.py` to Rust
would require integrating with a Rust ORM (e.g., Diesel, SeaORM), which is out of scope
for this hackathon. Additionally, R6 prohibits linking to the Python runtime.

**Baseline behavior:** All 16 tests in `test_django.py` skip with `"Django not installed"`
even on a clean clone without Django. This is the ORIGINAL baseline skip behavior.
Under our PyO3 build (`maturin develop`), Django is still not installed, so these
16 tests continue to skip naturally — **zero parity impact**.

**Verification:**
```
pytest tests/original/test_django.py -rs
# Expected: 16 skipped, reason: "Django not installed"
```

**Parity calculation:**
- Total original tests collected: 70 (54 non-Django + 16 Django)
- Baseline non-Django passing: 54
- Django tests: 16 (skip in original, skip in port — identical behavior)
- **Parity denominator: 54** (non-Django tests only)
- **Parity numerator: TBD** (target: 54/54 = 100%)

**DECISIONS.md reference:** D13 (Django excluded), D14 (PyO3 strategy, Django self-resolves)
