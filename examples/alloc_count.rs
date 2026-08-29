//! Temporary instrumentation: counts heap allocations per `GraphDiff::apply` workload.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use flatpg::{
    graph::{Graph, builder::GraphDiff},
    node::{NewNode, NodeId},
    property::PropertyValue,
};
use test_fixtures::{Status, TestEdge, TestProperty, TestSchema, builders};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static FREED_BYTES: AtomicUsize = AtomicUsize::new(0);
static ON: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ON.load(Ordering::Relaxed) == 1 {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ON.load(Ordering::Relaxed) == 1 {
            FREED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn node_for(i: usize) -> NewNode<TestSchema> {
    match i % 3 {
        0 => builders::AlphaNodeBuilder::new()
            .add_property(TestProperty::Key, format!("key-{i}"))
            .unwrap()
            .add_property(TestProperty::Values, format!("v{i}-a"))
            .unwrap()
            .add_property(TestProperty::State, Status::Active)
            .unwrap()
            .build(),
        1 => builders::BetaNodeBuilder::new()
            .add_property(TestProperty::Count, i as i32)
            .unwrap()
            .build(),
        _ => builders::GammaNodeBuilder::new()
            .add_property(TestProperty::Flag, i % 2 == 0)
            .unwrap()
            .add_property(TestProperty::Score, i as f64)
            .unwrap()
            .build(),
    }
}

fn bulk(n: usize, edges_per_node: usize) -> GraphDiff<TestSchema> {
    let mut diff = GraphDiff::<TestSchema>::default();
    let ids: Vec<usize> = (0..n).map(|i| diff.add_node(node_for(i))).collect();
    let stride = (n / (edges_per_node + 1)).max(1);
    for (i, &src) in ids.iter().enumerate() {
        for k in 1..=edges_per_node {
            diff.add_edge(
                src,
                ids[(i + k * stride) % n],
                TestEdge::Labeled,
                Some(PropertyValue::String(format!("edge-{i}-{k}"))),
            );
        }
    }
    diff
}

fn measure<T>(label: &str, unit: usize, f: impl FnOnce() -> T) {
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    ON.store(1, Ordering::Relaxed);
    let out = f();
    ON.store(0, Ordering::Relaxed);
    let a = ALLOCS.load(Ordering::Relaxed);
    let b = BYTES.load(Ordering::Relaxed);
    println!(
        "{label:<34} {a:>9} allocs  {:>7.1}/unit  {:>9} KB",
        a as f64 / unit as f64,
        b / 1024
    );
    drop(out);
}

fn footprint<T>(label: &str, unit: usize, f: impl FnOnce() -> T) -> T {
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    FREED_BYTES.store(0, Ordering::Relaxed);
    ON.store(1, Ordering::Relaxed);
    let out = f();
    ON.store(0, Ordering::Relaxed);
    let gross = BYTES.load(Ordering::Relaxed);
    let freed = FREED_BYTES.load(Ordering::Relaxed);
    let net = gross.saturating_sub(freed);
    println!(
        "{label:<34} {:>9.2} MB net  {:>7.1} B/unit  ({:>9.2} MB gross)",
        net as f64 / (1024.0 * 1024.0),
        net as f64 / unit as f64,
        gross as f64 / (1024.0 * 1024.0),
    );
    out
}

fn main() {
    let nodes_count = 20_000;

    let diff = bulk(nodes_count, 0);
    measure("add_node (20k nodes, 0 edges)", nodes_count, || {
        diff.apply(Graph::new()).expect("apply")
    });

    let diff = bulk(nodes_count, 2);
    measure("bulk_insert_empty (20k, 2 e/n)", nodes_count, || {
        diff.apply(Graph::new()).expect("apply")
    });

    let base = || bulk(nodes_count, 2).apply(Graph::new()).expect("base");
    let (_, ids) = base();

    let mut edges = GraphDiff::<TestSchema>::default();
    let stride = (ids.len() / 3).max(1);
    for (i, &src) in ids.iter().enumerate() {
        for k in 1..=2usize {
            edges.add_edge(
                src,
                ids[(i + k * stride) % ids.len()],
                TestEdge::Labeled,
                Some(PropertyValue::String(format!("more-{i}-{k}"))),
            );
        }
    }
    let (g2, _) = base();
    measure("add_edge (40k new edges)", nodes_count, || {
        edges.apply(g2).expect("apply")
    });

    let k = nodes_count / 10;
    let stride = (ids.len() / k).max(1);
    let mut upd = GraphDiff::<TestSchema>::default();
    for j in 0..k {
        let idx = (j * stride) % ids.len();
        let node: &NodeId<TestSchema> = &ids[idx];
        match idx % 3 {
            0 => {
                upd.update_node_property(
                    node,
                    TestProperty::Values,
                    PropertyValue::String(format!("updated-{idx}")),
                );
            }
            1 => {
                upd.update_node_property(node, TestProperty::Count, PropertyValue::Int(idx as i32));
            }
            _ => {
                upd.update_node_property(node, TestProperty::Score, PropertyValue::Double(1.0));
            }
        }
    }
    let (g3, _) = base();
    measure("update_property (2k updates)", k, || {
        upd.apply(g3).expect("apply")
    });

    let mut rm = GraphDiff::<TestSchema>::default();
    for j in 0..k {
        rm.remove_node(&ids[(j * stride) % ids.len()]);
    }
    let (g4, _) = base();
    measure("remove_node (2k removals)", k, || {
        rm.apply(g4).expect("apply")
    });

    let n2 = 100_000;
    let (g, _) = footprint("graph footprint (100k nodes, 100k edges)", n2, || {
        bulk(n2, 1).apply(Graph::new()).expect("apply")
    });
    drop(g);
}
