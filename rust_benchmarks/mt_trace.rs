use immix_rust::{Address, ImmixMutatorLocal, ImmixSpace};

use std::alloc::Layout;
use std::time::Instant;

pub const K: usize = 4;
pub const TREE_DEPTH: usize = 10; // 10
pub const TREE_COUNT: usize = 50; // 50

pub const OBJECT_SIZE: usize = K * 8;
pub const OBJECT_ALIGN: usize = 8;

#[inline(always)]
fn alloc_k_ary_tree(mutator: &mut ImmixMutatorLocal) -> Address {
    let addr = mutator.alloc(Layout::from_size_align(OBJECT_SIZE, OBJECT_ALIGN).unwrap());
    mutator.init_object(addr, 0b1100_1111);
    addr
}

fn make_tree(depth: usize, mutator: &mut ImmixMutatorLocal) -> Address {
    if depth == 0 {
        alloc_k_ary_tree(mutator)
    } else {
        let mut children = vec![];
        for _ in 0..K {
            children.push(make_tree(depth - 1, mutator));
        }

        let result = alloc_k_ary_tree(mutator);
        //        println!("parent node: {:X}", result);

        let mut cursor = result;
        for _ in 0..K {
            let child = children.pop().unwrap();
            //            println!("  child: {:X} at {:X}", child, cursor);
            unsafe { *cursor.to_ptr_mut::<Address>() = child };
            cursor = cursor.plus(8);
        }

        result
    }
}

#[allow(unused_variables)]
pub fn alloc_trace(space_size: usize) {
    use std::sync::Arc;

    let shared_space = Arc::new(ImmixSpace::new(space_size));

    let mut mutator = ImmixMutatorLocal::new(shared_space);

    println!(
        "Trying to allocate 1 object of (size {}, align {}). ",
        K * 8,
        8
    );
    println!(
        "Considering header size of {}, an object should be {}. ",
        0, OBJECT_SIZE
    );

    println!(
        "Trying to allocate {} trees of depth {}, which is {} objects ({} bytes)",
        TREE_COUNT,
        TREE_DEPTH,
        TREE_COUNT * K.pow(TREE_DEPTH as u32),
        TREE_COUNT * K.pow(TREE_DEPTH as u32) * OBJECT_SIZE
    );

    let mut roots = vec![];

    for _ in 0..TREE_COUNT {
        roots.push(unsafe { make_tree(TREE_DEPTH, &mut mutator).to_object_reference() });
    }

    println!("Start tracing");

    let time_start = Instant::now();

    // TODO: Fix this
    // heap::gc::start_trace(&mut roots, shared_space);
    println!("{:?}", roots);

    let elapsed = time_start.elapsed();

    println!("time used: {:?}", elapsed);
}
