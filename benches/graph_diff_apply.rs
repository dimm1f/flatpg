use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use flatpg::{
    edge::Direction,
    graph::{Graph, builder::GraphDiff},
    node::NodeId,
    property::PropertyValue,
};
use test_fixtures::{
    Status, TestEdge, TestProperty, TestSchema,
    graphs::{build_bulk_diff, node_for},
};

const SIZES: [usize; 2] = [1_000, 20_000];
const EDGES_PER_NODE: usize = 2;
const CHANGE_FRACTION: usize = 10;

fn build_incremental_diff(
    base_nodes: &[NodeId<TestSchema>],
    extra_node_count: usize,
    edges_per_node: usize,
) -> GraphDiff<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    let new_ids: Vec<usize> = (0..extra_node_count)
        .map(|i| diff.add_node(node_for(i)))
        .collect();

    let base_len = base_nodes.len();
    let stride = (base_len / (edges_per_node + 1)).max(1);
    for (i, &new_id) in new_ids.iter().enumerate() {
        for k in 1..=edges_per_node {
            let existing = base_nodes[(i * stride + k) % base_len];
            diff.add_edge(
                existing,
                new_id,
                TestEdge::Labeled,
                Some(PropertyValue::String(format!("inc-{i}-{k}"))),
            );
        }
    }
    diff
}

fn build_edges_only_diff(
    base_nodes: &[NodeId<TestSchema>],
    edges_per_node: usize,
) -> GraphDiff<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    let n = base_nodes.len();
    let stride = (n / (edges_per_node + 1)).max(1);
    for (i, &src) in base_nodes.iter().enumerate() {
        for k in 1..=edges_per_node {
            let dst = base_nodes[(i + k * stride) % n];
            diff.add_edge(
                src,
                dst,
                TestEdge::Labeled,
                Some(PropertyValue::String(format!("more-{i}-{k}"))),
            );
        }
    }
    diff
}

/// Property picked from `idx % 3`, matching `node_for`'s Alpha/Beta/Gamma
/// construction order so the property is always valid for that node's kind.
fn update_for(diff: &mut GraphDiff<TestSchema>, node: &NodeId<TestSchema>, idx: usize) {
    match idx % 3 {
        0 => {
            let values: Vec<PropertyValue> = (0..5)
                .map(|v| PropertyValue::String(format!("updated-{idx}-{v}")))
                .collect();
            diff.update_node_property(node, TestProperty::Values, values);
        }
        1 => {
            diff.update_node_property(
                node,
                TestProperty::Count,
                PropertyValue::Int((idx * 7) as i32),
            );
        }
        _ => {
            let tags: Vec<PropertyValue> = vec![Status::Inactive.into(), Status::Banned.into()];
            diff.update_node_property(node, TestProperty::Tags, tags);
        }
    }
}

fn bench_add_node(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply/add_node");
    for &n in &SIZES {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || build_bulk_diff(n, 0),
                |diff| black_box(diff.apply(Graph::new()).expect("apply diff")),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_add_edge(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply/add_edge");
    for &n in &SIZES {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || {
                    let (graph, base_ids) = build_bulk_diff(n, EDGES_PER_NODE)
                        .apply(Graph::new())
                        .expect("apply base diff");
                    let extra = build_edges_only_diff(&base_ids, EDGES_PER_NODE);
                    (graph, extra)
                },
                |(graph, extra)| black_box(extra.apply(graph).expect("apply edges-only diff")),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_remove_node(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply_changes/remove_node");
    for &n in &SIZES {
        let k = (n / CHANGE_FRACTION).max(1);
        group.throughput(Throughput::Elements(k as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let k = (n / CHANGE_FRACTION).max(1);
            b.iter_batched(
                || {
                    let (graph, ids) = build_bulk_diff(n, EDGES_PER_NODE)
                        .apply(Graph::new())
                        .expect("apply base diff");
                    let stride = (ids.len() / k).max(1);
                    let mut diff = GraphDiff::<TestSchema>::default();
                    for j in 0..k {
                        let idx = (j * stride) % ids.len();
                        diff.remove_node(&ids[idx]);
                    }
                    (graph, diff)
                },
                |(graph, diff)| black_box(diff.apply(graph).expect("apply remove_node diff")),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_remove_edge(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply_changes/remove_edge");
    for &n in &SIZES {
        let k = (n / CHANGE_FRACTION).max(1);
        group.throughput(Throughput::Elements(k as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let k = (n / CHANGE_FRACTION).max(1);
            b.iter_batched(
                || {
                    let (graph, ids) = build_bulk_diff(n, EDGES_PER_NODE)
                        .apply(Graph::new())
                        .expect("apply base diff");
                    let stride = (ids.len() / k).max(1);
                    let mut diff = GraphDiff::<TestSchema>::default();
                    for j in 0..k {
                        let node = ids[(j * stride) % ids.len()];
                        if let Some(edge) = graph
                            .get_edges(node, TestEdge::Labeled, Direction::Out)
                            .expect("out edges")
                            .next()
                        {
                            diff.remove_edge(edge);
                        }
                    }
                    (graph, diff)
                },
                |(graph, diff)| black_box(diff.apply(graph).expect("apply remove_edge diff")),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_update_property(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply_changes/update_property");
    for &n in &SIZES {
        let k = (n / CHANGE_FRACTION).max(1);
        group.throughput(Throughput::Elements(k as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let k = (n / CHANGE_FRACTION).max(1);
            b.iter_batched(
                || {
                    let (graph, ids) = build_bulk_diff(n, EDGES_PER_NODE)
                        .apply(Graph::new())
                        .expect("apply base diff");
                    let stride = (ids.len() / k).max(1);
                    let mut diff = GraphDiff::<TestSchema>::default();
                    for j in 0..k {
                        let idx = (j * stride) % ids.len();
                        update_for(&mut diff, &ids[idx], idx);
                    }
                    (graph, diff)
                },
                |(graph, diff)| black_box(diff.apply(graph).expect("apply update_property diff")),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_bulk_insert_empty(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply/bulk_insert_empty");
    for &n in &SIZES {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || build_bulk_diff(n, EDGES_PER_NODE),
                |diff| black_box(diff.apply(Graph::new()).expect("apply diff")),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_bulk_insert_incremental(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply/bulk_insert_incremental");
    for &n in &SIZES {
        let extra_n = (n / CHANGE_FRACTION).max(1);
        group.throughput(Throughput::Elements(extra_n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let extra_n = (n / CHANGE_FRACTION).max(1);
            b.iter_batched(
                || {
                    let (graph, base_ids) = build_bulk_diff(n, EDGES_PER_NODE)
                        .apply(Graph::new())
                        .expect("apply base diff");
                    let extra = build_incremental_diff(&base_ids, extra_n, EDGES_PER_NODE);
                    (graph, extra)
                },
                |(graph, extra)| black_box(extra.apply(graph).expect("apply incremental diff")),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_add_node,
    bench_add_edge,
    bench_remove_node,
    bench_remove_edge,
    bench_update_property,
    bench_bulk_insert_empty,
    bench_bulk_insert_incremental,
);
criterion_main!(benches);
