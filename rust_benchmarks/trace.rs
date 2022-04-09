use immix_rust::common::Address;
use immix_rust::heap;
use immix_rust::heap::immix::ImmixMutatorLocal;
use immix_rust::heap::immix::ImmixSpace;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use crate::exhaust::ALLOCATION_TIMES;
use crate::exhaust::OBJECT_ALIGN;
use crate::exhaust::OBJECT_SIZE;

const TRACE_TIMES: usize = ALLOCATION_TIMES;

#[allow(unused_variables)]
pub fn alloc_trace() {
    let shared_space: Arc<ImmixSpace> = {
        let space: ImmixSpace = ImmixSpace::new(heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst));

        Arc::new(space)
    };
    heap::gc::init(shared_space.clone());

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
    let root = mutator.alloc(ACTUAL_OBJECT_SIZE, OBJECT_ALIGN);
    mutator.init_object(root, 0b1100_0001);

    let mut prev = root;
    for _ in 0..TRACE_TIMES - 1 {
        let res = mutator.alloc(ACTUAL_OBJECT_SIZE, OBJECT_ALIGN);
        mutator.init_object(res, 0b1100_0001);

        // set prev's 1st field (offset 0) to this object
        unsafe { prev.store::<Address>(res) };

        prev = res;
    }

    trace_loop(root, shared_space);
}

#[inline(never)]
fn trace_loop(root: Address, shared_space: Arc<ImmixSpace>) {
    println!("Start tracing");
    let mut roots = vec![unsafe { root.to_object_reference() }];

    let time_start = Instant::now();

    heap::gc::start_trace(&mut roots, shared_space);

    let elapsed = time_start.elapsed();

    println!("time used: {:?}", elapsed);
}
