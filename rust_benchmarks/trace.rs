// use immix_rust::Address;
// use immix_rust::heap;
use immix_rust::{Address, ImmixMutatorLocal, ImmixSpace};
use std::alloc::Layout;

use std::sync::Arc;
use std::time::Instant;

use crate::exhaust::ALLOCATION_TIMES;
use crate::exhaust::OBJECT_ALIGN;
use crate::exhaust::OBJECT_SIZE;

const TRACE_TIMES: usize = ALLOCATION_TIMES;

#[allow(unused_variables)]
pub fn alloc_trace(space_size: usize) {
    let shared_space = Arc::new(ImmixSpace::new(space_size));

    let mut mutator = ImmixMutatorLocal::new(shared_space.clone());

    println!(
        "Trying to allocate 1 object of (size {}, align {}). ",
        OBJECT_SIZE, OBJECT_ALIGN
    );
    const ACTUAL_OBJECT_SIZE: usize = OBJECT_SIZE;
    println!(
        "Considering header size of {}, an object should be {}. ",
        0, ACTUAL_OBJECT_SIZE
    );

    println!(
        "Trying to allocate {} objects, which will take roughly {} bytes",
        TRACE_TIMES,
        TRACE_TIMES * ACTUAL_OBJECT_SIZE
    );
    let root = mutator.alloc(Layout::from_size_align(ACTUAL_OBJECT_SIZE, OBJECT_ALIGN).unwrap());
    mutator.init_object(root, 0b1100_0001);

    let mut prev = root;
    for _ in 0..TRACE_TIMES - 1 {
        let res = mutator.alloc(Layout::from_size_align(ACTUAL_OBJECT_SIZE, OBJECT_ALIGN).unwrap());
        mutator.init_object(res, 0b1100_0001);

        // set prev's 1st field (offset 0) to this object
        unsafe { *prev.to_ptr_mut::<Address>() = res };

        prev = res;
    }

    trace_loop(root, shared_space);
}

#[inline(never)]
fn trace_loop(root: Address, _shared_space: Arc<ImmixSpace>) {
    println!("Start tracing");
    let roots = vec![unsafe { root.to_object_reference() }];

    let time_start = Instant::now();

    // TODO: Fix this
    // heap::gc::start_trace(&mut roots, shared_space);
    println!("{:?}", roots);

    let elapsed = time_start.elapsed();

    println!("time used: {:?}", elapsed);
}
