use immix_rust::common::ObjectReference;
use immix_rust::heap;
use immix_rust::heap::immix::ImmixMutatorLocal;
use immix_rust::heap::immix::ImmixSpace;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use crate::exhaust::ALLOCATION_TIMES;
use crate::exhaust::OBJECT_ALIGN;
use crate::exhaust::OBJECT_SIZE;

const MARK_TIMES: usize = ALLOCATION_TIMES;

#[allow(unused_variables)]
pub fn alloc_mark() {
    let shared_space: Arc<ImmixSpace> = {
        let space: ImmixSpace = ImmixSpace::new(heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst));

        Arc::new(space)
    };

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
        MARK_TIMES,
        MARK_TIMES * ACTUAL_OBJECT_SIZE
    );
    let mut objs = vec![];
    for _ in 0..MARK_TIMES {
        let res = mutator.alloc(ACTUAL_OBJECT_SIZE, OBJECT_ALIGN);
        mutator.init_object(res, 0b1100_0011);

        objs.push(unsafe { res.to_object_reference() });
    }

    mark_loop(objs, &shared_space);
}

#[inline(never)]
fn mark_loop(objs: Vec<ObjectReference>, shared_space: &Arc<ImmixSpace>) {
    println!("Start marking");
    let time_start = Instant::now();

    let mark_state = crate::objectmodel::MARK_STATE.load(Ordering::SeqCst) as u8;

    let line_mark_table = shared_space.line_mark_table();
    let (space_start, space_end) = (shared_space.start(), shared_space.end());

    let trace_map = shared_space.trace_map.ptr;

    for i in 0..objs.len() {
        let obj = unsafe { *objs.get_unchecked(i) };

        // mark the object as traced
        unsafe { crate::objectmodel::mark_as_traced(trace_map, space_start, obj, mark_state) };

        // mark meta-data
        if obj.to_address() >= space_start && obj.to_address() < space_end {
            line_mark_table.mark_line_live2(space_start, obj.to_address());
        }
    }

    let elapsed = time_start.elapsed();

    println!("time used: {:?}", elapsed);
}
