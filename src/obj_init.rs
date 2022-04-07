use time;

use crate::heap;
use crate::heap::immix::ImmixMutatorLocal;
use crate::heap::immix::ImmixSpace;
use crate::heap::freelist::FreeListSpace;
use crate::common::Address;

use std::sync::RwLock;
use std::sync::{Arc};
use std::sync::atomic::Ordering;

use crate::exhaust::OBJECT_SIZE;
use crate::exhaust::OBJECT_ALIGN;
use crate::exhaust::ALLOCATION_TIMES;

const INIT_TIMES : usize = ALLOCATION_TIMES;

#[allow(unused_variables)]
pub fn alloc_init() {
    let shared_space : Arc<ImmixSpace> = {
        let space : ImmixSpace = ImmixSpace::new(heap::IMMIX_SPACE_SIZE.load(Ordering::SeqCst));
        
        Arc::new(space)
    };
    let lo_space : Arc<RwLock<FreeListSpace>> = {
        let space : FreeListSpace = FreeListSpace::new(heap::LO_SPACE_SIZE.load(Ordering::SeqCst));
        Arc::new(RwLock::new(space))
    };
    heap::gc::init(shared_space.clone(), lo_space.clone());

    let mut mutator = ImmixMutatorLocal::new(shared_space.clone());
    
    println!("Trying to allocate 1 object of (size {}, align {}). ", OBJECT_SIZE, OBJECT_ALIGN);
    const ACTUAL_OBJECT_SIZE : usize = OBJECT_SIZE;
    println!("Considering header size of {}, an object should be {}. ", 0, ACTUAL_OBJECT_SIZE);
    
    println!("Trying to allocate {} objects, which will take roughly {} bytes", INIT_TIMES, INIT_TIMES * ACTUAL_OBJECT_SIZE);
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
    let t_start = time::now_utc();
    
    for obj in objs {    
        mutator.init_object_no_inline(obj, 0b1100_0011);
//        mutator.init_object_no_inline(obj, 0b1100_0111);
    }
    
    let t_end = time::now_utc();
    
    println!("time used: {} msec", (t_end - t_start).num_milliseconds());
}