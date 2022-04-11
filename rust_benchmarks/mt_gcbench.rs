#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused_variables)]

use std::alloc::Layout;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

use immix_rust::heap;
use immix_rust::heap::immix::ImmixMutatorLocal;
use immix_rust::heap::immix::ImmixSpace;
use std::mem::size_of;

const kStretchTreeDepth: usize = 18;
const kLongLivedTreeDepth: usize = 16;
const kArraySize: usize = 500000;
const kMinTreeDepth: usize = 4;
const kMaxTreeDepth: usize = 21;

#[repr(C)] // Enforce field ordering
struct Node {
    _header: u64,
    left: *mut Node,
    right: *mut Node,
    _i: i32,
    _j: i32,
}

fn init_Node(me: *mut Node, l: *mut Node, r: *mut Node) {
    unsafe {
        (*me).left = l;
        (*me).right = r;
    }
}

fn TreeSize(i: usize) -> usize {
    (1 << (i + 1)) - 1
}

fn NumIters(i: usize) -> usize {
    20 * TreeSize(kMaxTreeDepth) / TreeSize(i)
}

fn Populate(iDepth: usize, thisNode: *mut Node, mutator: &mut ImmixMutatorLocal) {
    if iDepth == 0 {
        return;
    }
    unsafe {
        (*thisNode).left = alloc(mutator);
        (*thisNode).right = alloc(mutator);
        Populate(iDepth - 1, (*thisNode).left, mutator);
        Populate(iDepth - 1, (*thisNode).right, mutator);
    }
}

fn MakeTree(iDepth: usize, mutator: &mut ImmixMutatorLocal) -> *mut Node {
    if iDepth == 0 {
        alloc(mutator)
    } else {
        let left = MakeTree(iDepth - 1, mutator);
        let right = MakeTree(iDepth - 1, mutator);
        let result = alloc(mutator);
        init_Node(result, left, right);

        result
    }
}

fn PrintDiagnostics() {}

fn TimeConstruction(depth: usize, mutator: &mut ImmixMutatorLocal) {
    let iNumIters = NumIters(depth);
    println!("creating {} trees of depth {}", iNumIters, depth);

    let time_start = Instant::now();
    for _ in 0..iNumIters {
        let tempTree = alloc(mutator);
        Populate(depth, tempTree, mutator);

        // destroy tempTree
    }
    let elapsed = time_start.elapsed();
    println!("\tTop down construction took {:?}", elapsed);

    let time_start = Instant::now();
    for _ in 0..iNumIters {
        let tempTree = MakeTree(depth, mutator);
    }
    let elapsed = time_start.elapsed();
    println!("\tButtom up construction took {:?}", elapsed);
}

fn run_one_test(immix_space: Arc<ImmixSpace>) {
    heap::gc::set_low_water_mark();
    let mut mutator = ImmixMutatorLocal::new(immix_space);


    println!(
        " Creating a long-lived binary tree of depth {}",
        kLongLivedTreeDepth
    );
    let longLivedTree = alloc(&mut mutator);
    Populate(kLongLivedTreeDepth, longLivedTree, &mut mutator);

    let mut d = kMinTreeDepth;
    while d <= kMaxTreeDepth {
        TimeConstruction(d, &mut mutator);
        d += 2;
    }

    // drop(longLivedTree);
    mutator.destroy();
}

#[inline(always)]
fn alloc(mutator: &mut ImmixMutatorLocal) -> *mut Node {
    let addr = mutator.alloc(Layout::new::<Node>());
    mutator.init_object(addr, 0b1100_0011);
    addr.to_ptr_mut::<Node>()
}

use std::time::Instant;

pub fn start() {
    heap::gc::set_low_water_mark();

    let n_threads: usize = num_cpus::get();

    let immix_space: Arc<ImmixSpace> = {
        let space: ImmixSpace = ImmixSpace::new(heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst));
        Arc::new(space)
    };

    // let mut mutator = ImmixMutatorLocal::new(immix_space.clone());


    let peak_capacity = size_of::<Node>() * n_threads * TreeSize(kLongLivedTreeDepth)
        + size_of::<Node>() * n_threads * TreeSize(kMaxTreeDepth);
    println!("Garbage Collector Test");
    println!(
        " Live storage will peak at {} bytes ({} MB).\n",
            peak_capacity,
            peak_capacity >> 20
            // + (size_of::<Array>() as i32)
    );

    println!(
        " Stretching memory with a binary tree or depth {}",
        kStretchTreeDepth
    );
    PrintDiagnostics();

    let time_start = Instant::now();

    // Stretch the memory space quickly
    // let tempTree = MakeTree(kStretchTreeDepth, &mut mutator);
    // destroy tree


    println!(" Creating a long-lived array of {} doubles", kArraySize);
    // TODO: mutator.alloc(size_of::<Array>(), 8);

    let mut threads = vec![];
    for i in 0..n_threads {
        let immix_space_clone = immix_space.clone();
        let t = thread::spawn(move || {
            run_one_test(immix_space_clone);
        });
        threads.push(t);
    }

    // run one test locally
    // let mut d = kMinTreeDepth;
    // while d <= kMaxTreeDepth {
    //     TimeConstruction(d, &mut mutator);
    //     d += 2;
    // }

    // mutator.destroy();

    for t in threads {
        t.join().unwrap();
    }

    // if longLivedTree.is_null() {
    //     println!("Failed(long lived tree wrong)");
    // }

    let elapsed = time_start.elapsed();

    PrintDiagnostics();
    println!("Completed in {:?}", elapsed);
    println!("Finished with {} collections", heap::gc::gc_count());
}
