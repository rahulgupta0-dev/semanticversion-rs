//! Criterion benchmarks — parse, match, compare latency + throughput.
//! Runs against the *native Rust core* (no PyO3 overhead).
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use semantic_version::{NpmSpec, SimpleSpec, Version};

// ---------------------------------------------------------------------------
// Static representative test data
// ---------------------------------------------------------------------------

static VERSION_STRINGS: &[&str] = &[
    "0.1.0",
    "1.0.0-alpha",
    "1.0.0-alpha.1",
    "1.0.0-alpha.beta",
    "1.0.0-beta",
    "1.0.0-beta.2",
    "1.0.0-beta.11",
    "1.0.0-rc.1",
    "1.0.0-rc.1+build.1",
    "1.0.0",
    "1.0.0+0.3.7",
    "1.3.7+build",
    "1.3.7+build.2.b8f12d7",
    "1.3.7+build.11.e0f985a",
    "2.0.0-rc.1",
    "2.0.0-rc.3",
    "2.0.0",
    "2.1.0",
    "2.2.0",
    "3.0.0",
];

static SIMPLE_SPECS: &[&str] = &[
    ">=1.0.0",
    ">=1.0.0,<2.0.0",
    ">=1.0.0-rc.1,<2.0.0",
    "==1.0.0-alpha.1",
    "!=1.0.0",
    "*",
    "~=1.2.3",
    "1.2.3",
];

static NPM_SPECS: &[&str] = &[
    "^1.2.3",
    "~1.2.3",
    ">=1.2.3 <2.0.0",
    "1.2.3 - 2.0.0",
    ">=1.2.3-rc.1 <2.0.0",
    "*",
    "1.x",
    "1.2.x",
];

// ---------------------------------------------------------------------------
// Parse benchmarks
// ---------------------------------------------------------------------------

fn bench_version_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse/Version::parse");
    g.throughput(Throughput::Elements(VERSION_STRINGS.len() as u64));
    g.bench_function("parse_many", |b| {
        b.iter(|| {
            for s in VERSION_STRINGS {
                let _ = black_box(Version::parse(black_box(s)));
            }
        });
    });
    g.finish();
}

fn bench_simple_spec_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse/SimpleSpec::parse");
    g.throughput(Throughput::Elements(SIMPLE_SPECS.len() as u64));
    g.bench_function("parse_many", |b| {
        b.iter(|| {
            for s in SIMPLE_SPECS {
                let _ = black_box(SimpleSpec::parse(black_box(s)));
            }
        });
    });
    g.finish();
}

fn bench_npm_spec_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse/NpmSpec::parse");
    g.throughput(Throughput::Elements(NPM_SPECS.len() as u64));
    g.bench_function("parse_many", |b| {
        b.iter(|| {
            for s in NPM_SPECS {
                let _ = black_box(NpmSpec::parse(black_box(s)));
            }
        });
    });
    g.finish();
}

// ---------------------------------------------------------------------------
// match_version benchmarks
// ---------------------------------------------------------------------------

fn bench_match_version(c: &mut Criterion) {
    let version = Version::parse("1.2.3-beta.1+build.42").unwrap();
    let specs: Vec<(NpmSpec, &str)> = NPM_SPECS
        .iter()
        .filter_map(|s| NpmSpec::parse(s).ok().map(|sp| (sp, *s)))
        .collect();

    let mut g = c.benchmark_group("match/match_version");
    g.throughput(Throughput::Elements(specs.len() as u64));
    g.bench_function("npm_specs", |b| {
        b.iter(|| {
            for (spec, _) in &specs {
                let _ = black_box(spec.match_version(black_box(&version)));
            }
        });
    });
    g.finish();
}

// ---------------------------------------------------------------------------
// Comparison benchmarks
// ---------------------------------------------------------------------------

fn bench_comparison(c: &mut Criterion) {
    let v1 = Version::parse("1.3.7+build.2.b8f12d7").unwrap();
    let v2 = Version::parse("2.0.0-rc.1").unwrap();

    let mut g = c.benchmark_group("compare");
    g.throughput(Throughput::Elements(1));
    g.bench_function("precedence_lt", |b| {
        b.iter(|| {
            let _ = black_box(black_box(&v1).precedence_lt(black_box(&v2)));
        });
    });
    g.bench_function("precedence_gt", |b| {
        b.iter(|| {
            let _ = black_box(black_box(&v1).precedence_gt(black_box(&v2)));
        });
    });
    g.finish();
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_version_parse,
    bench_simple_spec_parse,
    bench_npm_spec_parse,
    bench_match_version,
    bench_comparison,
);
criterion_main!(benches);