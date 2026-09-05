use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use flatpg::strings_pool::StringsPool;

const SIZES: [usize; 2] = [1_000, 40_000];

/// Interns every string once, the shape a bulk insert of distinct property values takes.
fn bench_unique(c: &mut Criterion) {
    let mut group = c.benchmark_group("strings_pool/unique");
    for &n in &SIZES {
        let values: Vec<String> = (0..n).map(|i| format!("edge-{i}-value")).collect();
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &values, |b, values| {
            b.iter(|| {
                let mut pool = StringsPool::new();
                for value in values {
                    black_box(pool.intern(value));
                }
                pool
            });
        });
    }
    group.finish();
}

/// Interns a small set of labels over and over, so all but the first lookup is a hit.
fn bench_repeated(c: &mut Criterion) {
    let mut group = c.benchmark_group("strings_pool/repeated");
    let labels: Vec<String> = (0..16).map(|i| format!("label-{i}")).collect();
    for &n in &SIZES {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut pool = StringsPool::new();
                for i in 0..n {
                    black_box(pool.intern(&labels[i % labels.len()]));
                }
                pool
            });
        });
    }
    group.finish();
}

/// Resolves handles back to strings, the read side every property lookup goes through.
fn bench_resolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("strings_pool/resolve");
    for &n in &SIZES {
        let mut pool = StringsPool::new();
        let ids: Vec<_> = (0..n).map(|i| pool.intern(&format!("edge-{i}"))).collect();

        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &ids, |b, ids| {
            b.iter(|| {
                for &id in ids {
                    black_box(pool.get(id));
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_unique, bench_repeated, bench_resolve);
criterion_main!(benches);
