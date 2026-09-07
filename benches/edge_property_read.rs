//! Measures the cost of reading an edge's property while walking a node's adjacency.
//!
//! This is the one path the shared edge property store makes longer: the value is no longer
//! inline beside the neighbor, so reading it costs a second dependent load into the kind's
//! store. Two dimensions are measured.
//!
//! How the value is fetched:
//! - `scalar` uses [`Graph::get_edge_property_one`], the read a `One`-quantity kind's typed
//!   accessor generates. It is the like-for-like comparison against the pre-`EdgeSeq` API,
//!   which only ever returned a single value.
//! - `iterator` uses [`Graph::get_edge_property`], which has to build a `StorageArrayIter`
//!   because a `Multi` kind may hold a run. Against the old scalar API this shows what the
//!   iterator shape costs on top of the storage change.
//!
//! How the store is laid out relative to the adjacency, which decides whether the second load
//! is prefetchable:
//! - `grouped` builds each node's edges together, which is what a bulk load produces and what
//!   keeps a node's seqs consecutive in the store — one sequential stream, the best case.
//! - `interleaved` adds one edge per node per pass, so a node's k-th edge lands `node_count`
//!   entries away from its (k-1)-th. That reads as `EDGES_PER_NODE` sequential streams, which
//!   a hardware prefetcher still follows — it is not a random pattern despite looking like one.
//! - `scattered` adds edges in a shuffled order, so a node's seqs are uniform over the whole
//!   store and every read is an independent access. This is the real worst case, and what a
//!   long-lived graph drifts towards as edges are added and removed over time.
//!
//! `TestEdge::Distance` carries an `Int`, so nothing here touches the strings pool.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use flatpg::{
    edge::{Direction, EdgeId},
    graph::{Graph, builder::GraphDiff},
    property::PropertyValue,
    storage::StoredProperty,
};
use test_fixtures::{TestEdge, TestNode, TestSchema, builders};

const DEFAULT_SIZES: [usize; 2] = [20_000, 200_000];
const EDGES_PER_NODE: usize = 2;

/// Node counts to measure, overridable with `EDGE_READ_NODES` (comma-separated).
///
/// The default sizes keep every array inside a typical last-level cache, where the second
/// dependent load into the property store costs almost nothing. Raising this past
/// `L3 / (edges * 4 B)` is what exposes the store access as a memory stall instead.
fn sizes() -> Vec<usize> {
    std::env::var("EDGE_READ_NODES")
        .ok()
        .and_then(|raw| {
            raw.split(',')
                .map(|part| part.trim().parse().ok())
                .collect::<Option<Vec<usize>>>()
        })
        .unwrap_or_else(|| DEFAULT_SIZES.to_vec())
}

/// The two reads that differ from the pre-`EdgeSeq` implementation, kept apart so the two
/// versions of this benchmark stay otherwise identical. Before the change there was only one
/// `get_edge_property`, returning `Option<StoredProperty>`, and both map onto it.
#[inline]
fn read_scalar(graph: &Graph<TestSchema>, edge: EdgeId<TestSchema>) -> Option<StoredProperty> {
    graph
        .get_edge_property_one(edge)
        .expect("edge property lookup")
}

#[inline]
fn read_via_iterator(
    graph: &Graph<TestSchema>,
    edge: EdgeId<TestSchema>,
) -> Option<StoredProperty> {
    graph
        .get_edge_property(edge)
        .expect("edge property lookup")
        .next()
}

#[derive(Clone, Copy, PartialEq)]
enum Layout {
    Grouped,
    Interleaved,
    Scattered,
}

/// xorshift64*, so `Layout::Scattered` shuffles deterministically without a `rand` dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn build(node_count: usize, layout: Layout) -> Graph<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    let ids: Vec<usize> = (0..node_count)
        .map(|_| diff.add_node(builders::AlphaNodeBuilder::new().build()))
        .collect();

    let stride = (node_count / (EDGES_PER_NODE + 1)).max(1);
    let add = |diff: &mut GraphDiff<TestSchema>, i: usize, k: usize| {
        let dst = ids[(i + k * stride) % node_count];
        diff.add_edge(
            ids[i],
            dst,
            TestEdge::Distance,
            Some(PropertyValue::Int((i * EDGES_PER_NODE + k) as i32)),
        );
    };

    match layout {
        Layout::Grouped => {
            for i in 0..node_count {
                for k in 1..=EDGES_PER_NODE {
                    add(&mut diff, i, k);
                }
            }
        }
        Layout::Interleaved => {
            for k in 1..=EDGES_PER_NODE {
                for i in 0..node_count {
                    add(&mut diff, i, k);
                }
            }
        }
        Layout::Scattered => {
            let mut order: Vec<(u32, u32)> = (0..node_count)
                .flat_map(|i| (1..=EDGES_PER_NODE).map(move |k| (i as u32, k as u32)))
                .collect();
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
            for i in (1..order.len()).rev() {
                order.swap(i, (rng.next() % (i as u64 + 1)) as usize);
            }
            for (i, k) in order {
                add(&mut diff, i as usize, k as usize);
            }
        }
    }

    let (graph, _) = diff
        .apply(Graph::<TestSchema>::new())
        .expect("apply base diff");
    graph
}

/// Walks every node's outgoing edges and reads each one's property through `read`.
fn scan<R>(graph: &Graph<TestSchema>, read: R) -> usize
where
    R: Fn(&Graph<TestSchema>, EdgeId<TestSchema>) -> Option<StoredProperty> + Copy,
{
    let mut seen = 0usize;
    for node in graph.nodes_by_kind(TestNode::Alpha) {
        let edges = graph
            .get_edges(node, TestEdge::Distance, Direction::Out)
            .expect("out edges");
        for edge in edges {
            if read(graph, edge).is_some() {
                seen += 1;
            }
        }
    }
    seen
}

fn bench_group<R>(c: &mut Criterion, name: &str, layout: Layout, read: R)
where
    R: Fn(&Graph<TestSchema>, EdgeId<TestSchema>) -> Option<StoredProperty> + Copy,
{
    let mut group = c.benchmark_group(name);
    for n in sizes() {
        let graph = build(n, layout);
        group.throughput(Throughput::Elements((n * EDGES_PER_NODE) as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &graph, |b, graph| {
            b.iter(|| black_box(scan(black_box(graph), read)));
        });
    }
    group.finish();
}

const LAYOUTS: [(&str, Layout); 3] = [
    ("grouped", Layout::Grouped),
    ("interleaved", Layout::Interleaved),
    ("scattered", Layout::Scattered),
];

fn bench_scalar(c: &mut Criterion) {
    for (name, layout) in LAYOUTS {
        bench_group(
            c,
            &format!("edge_property_read/scalar/{name}"),
            layout,
            read_scalar,
        );
    }
}

fn bench_iterator(c: &mut Criterion) {
    for (name, layout) in LAYOUTS {
        bench_group(
            c,
            &format!("edge_property_read/iterator/{name}"),
            layout,
            read_via_iterator,
        );
    }
}

criterion_group!(benches, bench_scalar, bench_iterator);
criterion_main!(benches);
