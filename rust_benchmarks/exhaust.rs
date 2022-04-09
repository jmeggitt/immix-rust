use immix_rust::heap;
use immix_rust::heap::freelist::FreeListSpace;
use immix_rust::heap::immix::ImmixMutatorLocal;
use immix_rust::heap::immix::ImmixSpace;

use parking_lot::RwLock;
use std::time::Instant;

pub const OBJECT_SIZE: usize = 24;
pub const OBJECT_ALIGN: usize = 8;

pub const ALLOCATION_TIMES: usize = 50000000;

pub fn exhaust_alloc() {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    let shared_space: Arc<ImmixSpace> = {
        let space: ImmixSpace = ImmixSpace::new(heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst));

        Arc::new(space)
    };
    let lo_space: Arc<RwLock<FreeListSpace>> = {
        let space: FreeListSpace = FreeListSpace::new(heap::LO_SPACE_SIZE.load(Ordering::SeqCst));
        Arc::new(RwLock::new(space))
    };
    heap::gc::init(shared_space.clone(), lo_space);

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
        heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst)
    );

    alloc_loop(&mut mutator);
}

#[inline(never)]
fn alloc_loop(mutator: &mut ImmixMutatorLocal) {
    let time_start = Instant::now();

    for _ in 0..ALLOCATION_TIMES {
        //        mutator.yieldpoint();

        let res = mutator.alloc(OBJECT_SIZE, OBJECT_ALIGN);
        mutator.init_object(res, 0b1100_0011);
    }

    let elapsed = time_start.elapsed();
    println!("time used: {:?}", elapsed);
}
