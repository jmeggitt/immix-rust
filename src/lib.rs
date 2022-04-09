// TODO: Reduce the number of unsafe functions then remove this
#![allow(clippy::missing_safety_doc)]

use lazy_static::lazy_static;
use std::sync::atomic::Ordering;

pub mod common;
pub mod heap;
pub mod objectmodel;

use common::ObjectReference;
use heap::freelist;
use heap::freelist::FreeListSpace;
use heap::immix::ImmixMutatorLocal;
use heap::immix::ImmixSpace;
use parking_lot::RwLock;
use std::boxed::Box;
use std::sync::Arc;

pub use heap::gc::set_low_water_mark;
pub use heap::immix::ImmixMutatorLocal as Mutator;

#[repr(C)]
pub struct GC {
    immix_space: Arc<ImmixSpace>,
    lo_space: Arc<RwLock<FreeListSpace>>,
}

lazy_static! {
    pub static ref MY_GC: RwLock<Option<GC>> = RwLock::new(None);
}

#[no_mangle]
pub extern "C" fn gc_init(immix_size: usize, lo_size: usize, n_gcthreads: usize) {
    // set this line to turn on certain level of debugging info
    //    simple_logger::init_with_level(log::LogLevel::Trace).ok();

    // init space size
    heap::IMMIX_SPACE_SIZE.store(immix_size, Ordering::SeqCst);
    heap::LO_SPACE_SIZE.store(lo_size, Ordering::SeqCst);

    let (immix_space, lo_space) = {
        let immix_space = Arc::new(ImmixSpace::new(immix_size));
        let lo_space = Arc::new(RwLock::new(FreeListSpace::new(lo_size)));

        heap::gc::init(immix_space.clone(), lo_space.clone());

        (immix_space, lo_space)
    };

    *MY_GC.write() = Some(GC {
        immix_space,
        lo_space,
    });
    println!(
        "heap is {} bytes (immix: {} bytes, lo: {} bytes) . ",
        immix_size + lo_size,
        immix_size,
        lo_size
    );

    // gc threads
    heap::gc::GC_THREADS.store(n_gcthreads, Ordering::SeqCst);
    println!("{} gc threads", n_gcthreads);

    // init object model
    objectmodel::init();
}

#[no_mangle]
pub extern "C" fn new_mutator() -> Box<ImmixMutatorLocal> {
    Box::new(ImmixMutatorLocal::new(
        MY_GC.read().as_ref().unwrap().immix_space.clone(),
    ))
}

#[no_mangle]
pub extern "C" fn drop_mutator(mutator: Box<ImmixMutatorLocal>) {
    // Not required, but explicitly drop mutator to make intentions more explicit
    drop(mutator)
}

#[no_mangle]
#[inline(always)]
pub extern "C" fn yieldpoint(mutator: &mut Box<ImmixMutatorLocal>) {
    mutator.yieldpoint();
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn yieldpoint_slow(mutator: &mut Box<ImmixMutatorLocal>) {
    mutator.yieldpoint_slow()
}

#[no_mangle]
#[inline(always)]
pub extern "C" fn alloc(
    mutator: &mut Box<ImmixMutatorLocal>,
    size: usize,
    align: usize,
) -> ObjectReference {
    let addr = mutator.alloc(size, align);
    unsafe { addr.to_object_reference() }
}

#[no_mangle]
pub extern "C" fn alloc_slow(
    mutator: &mut Box<ImmixMutatorLocal>,
    size: usize,
    align: usize,
) -> ObjectReference {
    let ret = mutator.try_alloc_from_local(size, align);
    unsafe { ret.to_object_reference() }
}

#[no_mangle]
pub extern "C" fn alloc_large(
    mutator: &mut Box<ImmixMutatorLocal>,
    size: usize,
) -> ObjectReference {
    let ret = freelist::alloc_large(
        size,
        8,
        mutator,
        MY_GC.read().as_ref().unwrap().lo_space.clone(),
    );
    unsafe { ret.to_object_reference() }
}
