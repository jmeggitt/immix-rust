// use immix_rust::heap;
// use immix_rust::heap::immix::ImmixMutatorLocal;
// use immix_rust::heap::immix::ImmixSpace;
use immix_rust::{ImmixMutatorLocal, ImmixSpace};
use std::alloc::Layout;

use std::time::Instant;

pub const OBJECT_SIZE: usize = 24;
pub const OBJECT_ALIGN: usize = 8;

pub const ALLOCATION_TIMES: usize = 50000000;

pub fn exhaust_alloc(space_size: usize) {
    use std::sync::Arc;

    let shared_space = Arc::new(ImmixSpace::new(space_size));

    let mut mutator = ImmixMutatorLocal::new(shared_space);

    println!(
        "Trying to allocate {} objects of (size {}, align {}). ",
        ALLOCATION_TIMES, OBJECT_SIZE, OBJECT_ALIGN
    );
    const ACTUAL_OBJECT_SIZE: usize = OBJECT_SIZE;
    println!(
        "Considering header size of {}, an object should be {}. ",
        0, ACTUAL_OBJECT_SIZE
    );
    println!(
        "This would take {} bytes of {} bytes heap",
        ALLOCATION_TIMES * ACTUAL_OBJECT_SIZE,
        space_size
    );

    alloc_loop(&mut mutator);
}

#[inline(never)]
fn alloc_loop(mutator: &mut ImmixMutatorLocal) {
    let time_start = Instant::now();

    for _ in 0..ALLOCATION_TIMES {
        //        mutator.yieldpoint();

        let res = mutator.alloc(Layout::from_size_align(OBJECT_SIZE, OBJECT_ALIGN).unwrap());
        mutator.init_object(res, 0b1100_0011);
    }

    let elapsed = time_start.elapsed();
    println!("time used: {:?}", elapsed);
}
