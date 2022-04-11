use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use crate::common::Address;
use lazy_static::lazy_static;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, AtomicUsize, Ordering};
use generational_arena::{Arena, Index};
use parking_lot::{Condvar, Mutex, RwLock};
use crate::heap::immix::ImmixMutatorGlobal;
use crate::{ImmixSpace, ObjectReference};

pub mod gc;
pub mod immix;

pub const ALIGNMENT_VALUE: u8 = 1;

pub const IMMIX_SPACE_RATIO: f64 = 1.0 - LO_SPACE_RATIO;
pub const LO_SPACE_RATIO: f64 = 0.2;
pub const DEFAULT_HEAP_SIZE: usize = 500 << 20;

lazy_static! {
    // Safe to remove (Only used in benchmarks)
    pub static ref IMMIX_SPACE_SIZE: AtomicUsize =
        AtomicUsize::new((DEFAULT_HEAP_SIZE as f64 * IMMIX_SPACE_RATIO) as usize);
}

#[inline(always)]
pub fn fill_alignment_gap(start: Address, end: Address) {
    debug_assert!(end >= start);
    start.memset(ALIGNMENT_VALUE, end.diff(start));
}

#[repr(C)]
pub struct ImmixGC {
    space: ImmixSpace,
    // mutators: Mutex<Arena<Arc<AtomicBool>>>,
    // num_mutators: AtomicUsize,
    mark_state: AtomicUsize,
    // has_controller: AtomicBool,
    should_block: Arc<AtomicBool>,
    active_mutators: Mutex<usize>,
    blocking_condvar: Condvar,
    // TODO: This looks like a good candidate to replace with sharded_slab::Slab
    roots: Mutex<Vec<ObjectReference>>,
}

impl ImmixGC {
    pub fn new() -> Arc<Self> {
        Arc::new(ImmixGC {
            space: todo!(),
            // mutators: Mutex::new(Arena::with_capacity(1024)),
            // num_mutators: AtomicUsize::new(0),
            mark_state: AtomicUsize::new(1),
            // gc_controller: AtomicIsize::new(NO_CONTROLLER),
            should_block: Arc::new(AtomicBool::new(false)),
            active_mutators: Mutex::new(0),
            blocking_condvar: Condvar::new(),
        })
    }

    fn append_roots(&self, references: &[ObjectReference]) {
        let mut guard = self.roots.lock();
        guard.extend_from_slice(references);
    }

    fn claim_control_or_block(&self) -> bool {
        // self.has_controller.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok()
        let mut lock = self.active_mutators.lock();
        *lock -= 1;

        // If this as the last thread to yield it will take control of the gc
        if *lock == 0 {
            return self.should_block.load(Ordering::SeqCst)
        }

        // Otherwise block until the gc finishes
        while self.should_block.load(Ordering::SeqCst) {
            self.blocking_condvar.wait(&mut lock);
        }

        *lock += 1;
        false
    }

    fn release_block(&self) {
        self.should_block.store(false, Ordering::SeqCst);
        self.
    }

    fn release_control(&self) {
        self.should_block.store(false, Ordering::SeqCst);
        self.has_controller.store(false, Ordering::SeqCst);
    }

    pub fn trigger_gc(&self) -> bool {
        self.should_block.swap(true, Ordering::SeqCst)
    }

    #[inline(always)]
    fn mark_state(&self) -> usize {
        self.mark_state.load(Ordering::SeqCst)
    }

    fn flip_mark_state(&self) {
        self.mark_state.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| Some(x ^ 1)).ok();
    }

    fn add_mutator(&self) {
        let lock = self.active_mutators.lock();
        *lock += 1;
        // let mut mutators_lock = self.mutators.lock();
        // self.num_mutators.fetch_add(1, Ordering::SeqCst);
        // mutators_lock.insert(mutator_global.clone())
    }

    fn remove_mutator(&self) {
        let lock = self.active_mutators.lock();
        *lock -= 1;
        // let mut mutators_lock = self.mutators.lock();
        // self.num_mutators.fetch_sub(1, Ordering::SeqCst);
        // mutators_lock.remove(index);
    }
}

impl Deref for ImmixGC {
    type Target = ImmixSpace;

    fn deref(&self) -> &Self::Target {
        &self.space
    }
}

impl DerefMut for ImmixGC {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.space
    }
}

unsafe impl Sync for ImmixGC {}

