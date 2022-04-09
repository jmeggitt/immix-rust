use immix_rust::common::Address;
use immix_rust::heap;
use immix_rust::heap::freelist::FreeListSpace;
use immix_rust::heap::immix::ImmixMutatorLocal;
use immix_rust::heap::immix::ImmixSpace;

use parking_lot::RwLock;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use crate::exhaust::ALLOCATION_TIMES;
use crate::exhaust::OBJECT_ALIGN;
use crate::exhaust::OBJECT_SIZE;

const INIT_TIMES: usize = ALLOCATION_TIMES;

#[allow(unused_variables)]
pub fn alloc_init() {
    let shared_space: Arc<ImmixSpace> = {
        let space: ImmixSpace = ImmixSpace::new(heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst));

        Arc::new(space)
    };
    let lo_space: Arc<RwLock<FreeListSpace>> = {
        let space: FreeListSpace = FreeListSpace::new(heap::LO_SPACE_SIZE.load(Ordering::SeqCst));
        Arc::new(RwLock::new(space))
    };
    heap::gc::init(shared_space.clone(), lo_space.clone());

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
        INIT_TIMES,
        INIT_TIMES * ACTUAL_OBJECT_SIZE
    );
    let mut objs = vec![];
    for _ in 0..INIT_TIMES {
        let res = mutator.alloc(ACTUAL_OBJECT_SIZE, OBJECT_ALIGN);

        objs.push(res);
    }

    init_loop(objs, &mut mutator);
}

#[inline(never)]
fn init_loop(objs: Vec<Address>, mutator: &mut ImmixMutatorLocal) {
    println!("Start init objects");
    let time_start = Instant::now();

    for obj in objs {
        mutator.init_object_no_inline(obj, 0b1100_0011);
        //        mutator.init_object_no_inline(obj, 0b1100_0111);
    }

    let elapsed = time_start.elapsed();

    println!("time used: {:?}", elapsed);
}