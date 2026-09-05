use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use flatpg::{
    graph::{Graph, builder::GraphDiff},
    prelude::CheckIntegrity,
};
use test_fixtures::{
    TestSchema,
    graphs::{build_bulk_diff, build_multi_kind_diff},
};

const SIZES: [usize; 3] = [1_000, 20_000, 200_000];
const EDGES_PER_NODE: usize = 2;

fn bench_group(c: &mut Criterion, name: &str, build: fn(usize, usize) -> GraphDiff<TestSchema>) {
    let mut group = c.benchmark_group(name);
    for &n in &SIZES {
        let (graph, _) = build(n, EDGES_PER_NODE)
            .apply(Graph::<TestSchema>::new())
            .expect("apply base diff");

        group.throughput(Throughput::Elements((n * EDGES_PER_NODE) as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &graph, |b, graph| {
            b.iter(|| black_box(graph).check_integrity().expect("check integrity"));
        });
    }
    group.finish();
}

fn bench_single_kind(c: &mut Criterion) {
    bench_group(c, "check_integrity/single_kind", build_bulk_diff);
}

fn bench_multi_kind(c: &mut Criterion) {
    bench_group(c, "check_integrity/multi_kind", build_multi_kind_diff);
}

criterion_group!(benches, bench_single_kind, bench_multi_kind);
criterion_main!(benches);
