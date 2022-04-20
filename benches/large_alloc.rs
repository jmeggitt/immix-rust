use criterion::{black_box, criterion_group, criterion_main, Criterion};
use immix_rust::{set_low_water_mark, ImmixMutatorLocal, ImmixSpace};
use std::alloc::Layout;
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::{Arc, Barrier};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

// Default space size of 1GB
const IMMIX_SPACE_SIZE: usize = 1024 << 20;

#[repr(C)] // Enforce field ordering
struct Node {
    left: *mut Node,
    right: *mut Node,
    _i: i32,
    _j: i32,
}

impl Node {
    pub fn space_required(depth: usize) -> usize {
        size_of::<Self>() * ((1 << depth) - 1)
    }

    pub fn iters_to_reach_gc(depth: usize, gc_count: usize) -> usize {
        (gc_count * IMMIX_SPACE_SIZE) / (num_cpus::get() * Self::space_required(depth))
    }

    #[inline(always)]
    pub fn alloc(mutator: &mut ImmixMutatorLocal) -> *mut Self {
        let addr = mutator.alloc(Layout::new::<Node>());

        // TODO: This may not be the correct encoding
        mutator.init_object(addr, 0b1100_0011);
        addr.to_ptr_mut::<Node>()
    }

    #[inline] // Use inline hint to hopefully unroll a few recursions of the function
    pub fn build_bottom_up(depth: usize, mutator: &mut ImmixMutatorLocal) -> *mut Self {
        if depth == 0 {
            return null_mut();
        }

        let left = Self::build_bottom_up(depth - 1, mutator);
        let right = Self::build_bottom_up(depth - 1, mutator);
        let node = Self::alloc(mutator);

        unsafe {
            (*node).left = left;
            (*node).right = right;
        }

        node
    }

    #[inline] // Use inline hint to hopefully unroll a few recursions of the function
    pub fn build_top_down(depth: usize, mutator: &mut ImmixMutatorLocal) -> *mut Self {
        if depth == 0 {
            return null_mut();
        }

        let node = Self::alloc(mutator);
        unsafe {
            (*node).left = Self::build_top_down(depth - 1, mutator);
            (*node).right = Self::build_top_down(depth - 1, mutator);
        }

        node
    }
}

fn bench_build_tree(
    iters: usize,
    depth: usize,
    approach: fn(usize, &mut ImmixMutatorLocal) -> *mut Node,
) -> Duration {
    set_low_water_mark();
    let immix_space = Arc::new(ImmixSpace::new(IMMIX_SPACE_SIZE));

    let mut join_handles = Vec::with_capacity(num_cpus::get());
    let start_barrier = Arc::new(Barrier::new(num_cpus::get()));

    for _ in 0..num_cpus::get() {
        let barrier = start_barrier.clone();
        let space_ref = immix_space.clone();

        join_handles.push(thread::spawn(move || {
            set_low_water_mark();
            let mut mutator = ImmixMutatorLocal::new(space_ref);

            // Wait until all of the threads have initialized and reached this point
            barrier.wait();
            let start_time = Instant::now();

            for _ in 0..iters {
                let _ = black_box(approach(depth, &mut mutator));
            }

            let elapsed = start_time.elapsed();
            mutator.destroy();
            elapsed
        }));
    }

    let mut total_duration = Duration::from_secs(0);
    join_handles
        .drain(..)
        .map(JoinHandle::join)
        .map(Result::unwrap)
        .for_each(|x| total_duration += x);
    total_duration
}

fn criterion_benchmark(c: &mut Criterion) {
    println!("Space Requirements:");
    for depth in 0..40 {
        let space = num_cpus::get() * Node::space_required(depth);

        if space > 8 << 30 {
            println!("\t{}:\t{} GB", depth, (space + (1 << 30) - 1) >> 30);
        } else if space > 8 << 20 {
            println!("\t{}:\t{} MB", depth, (space + (1 << 20) - 1) >> 20);
        } else if space > 8 << 10 {
            println!("\t{}:\t{} KB", depth, (space + (1 << 10) - 1) >> 10);
        } else {
            println!("\t{}:\t{} B", depth, space);
        }
    }

    let mut tree_group = c.benchmark_group("tree");

    tree_group.bench_function("GC(depth 4)", move |b| {
        b.iter_custom(|iters| {
            println!("Preparing to do {} iters", iters);
            bench_build_tree(
                Node::iters_to_reach_gc(4, 3 * iters as usize) + 1,
                4,
                Node::build_top_down,
            )
        })
    });
    tree_group.bench_function("GC(depth 8)", move |b| {
        b.iter_custom(|iters| {
            println!("Preparing to do {} iters of depth 8", iters);
            bench_build_tree(
                Node::iters_to_reach_gc(8, 3 * iters as usize) + 1,
                8,
                Node::build_top_down,
            )
        })
    });
    tree_group.bench_function("GC(depth 16)", move |b| {
        b.iter_custom(|iters| {
            println!("Preparing to do {} iters of depth 16", iters);
            bench_build_tree(
                Node::iters_to_reach_gc(16, 3 * iters as usize) + 1,
                16,
                Node::build_top_down,
            )
        })
    });

    tree_group.bench_function("top_down(19)", move |b| {
        b.iter_custom(|iters| bench_build_tree(iters as usize, 19, Node::build_top_down))
    });

    tree_group.bench_function("bottom_up(19)", move |b| {
        b.iter_custom(|iters| bench_build_tree(iters as usize, 19, Node::build_bottom_up))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
