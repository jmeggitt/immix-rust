use crate::heap::gc;
use crate::heap::immix;
use crate::heap::immix::immix_space::ImmixBlock;
use crate::heap::immix::ImmixSpace;
use lazy_static::lazy_static;
use log::trace;

use crate::common::Address;

use crate::heap::immix::line_mark::LineMark;
use generational_arena::{Arena, Index};
use parking_lot::RwLock;
use std::alloc::Layout;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::*;

lazy_static! {
    pub static ref MUTATORS: RwLock<Arena<Arc<ImmixMutatorGlobal>>> = RwLock::new(Arena::with_capacity(1024));
    // pub static ref N_MUTATORS: RwLock<usize> = RwLock::new(0);
}

pub static N_MUTATORS: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
pub struct ImmixMutatorLocal {
    id: Index,

    // use raw pointer here instead of AddressMapTable
    // to avoid indirection in fast path
    alloc_map: *mut u8,
    space_start: Address,

    // cursor might be invalid, but Option<Address> is expensive here
    // after every GC, we set both cursor and limit
    // to Address::zero() so that alloc will branch to slow path
    cursor: Address,
    limit: Address,
    line: usize,

    // globally accessible per-thread fields
    pub global: Arc<ImmixMutatorGlobal>,

    space: Arc<ImmixSpace>,
    block: Option<Box<ImmixBlock>>,
}

#[derive(Default, Debug)]
pub struct ImmixMutatorGlobal {
    take_yield: AtomicBool,
    still_blocked: AtomicBool,
}

impl ImmixMutatorLocal {
    pub fn request_gc(&mut self) {
        gc::trigger_gc();
        self.yieldpoint();
    }

    pub fn reset(&mut self) {
        unsafe {
            // should not use Address::zero() other than initialization
            self.cursor = Address::null();
            self.limit = Address::null();
        }
        self.line = immix::LINES_IN_BLOCK;

        self.block = None;
    }

    pub fn new(space: Arc<ImmixSpace>) -> ImmixMutatorLocal {
        let global = Arc::new(ImmixMutatorGlobal::new());

        let mut mutators_lock = MUTATORS.write();
        let id = mutators_lock.insert(global.clone());
        N_MUTATORS.fetch_add(1, Ordering::SeqCst);

        ImmixMutatorLocal {
            id,
            cursor: unsafe { Address::null() },
            limit: unsafe { Address::null() },
            line: immix::LINES_IN_BLOCK,
            block: None,
            alloc_map: space.alloc_map.ptr,
            space_start: space.start(),
            global,
            space,
        }
    }

    pub fn immix_space(&self) -> Arc<ImmixSpace> {
        self.space.clone()
    }

    pub fn destroy(&mut self) {
        {
            self.return_block();
        }

        let mut mutators_lock = MUTATORS.write();
        mutators_lock.remove(self.id);
        N_MUTATORS.fetch_sub(1, Ordering::SeqCst);

        if cfg!(debug_assertions) {
            println!(
                "destroy mutator. Now live mutators = {}",
                mutators_lock.len()
            );
        }
    }

    #[inline(always)]
    pub fn yieldpoint(&mut self) {
        if self.global.take_yield() {
            self.yieldpoint_slow();
        }
    }

    #[cold]
    pub fn yieldpoint_slow(&mut self) {
        trace!("Mutator{:?}: yieldpoint triggered, slow path", self.id);
        gc::sync_barrier(self);
    }

    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> Address {
        let start = self.cursor.align_up(layout.align());
        let end = start.plus(layout.size());

        if end <= self.limit {
            self.cursor = end;
            start
        } else {
            self.try_alloc_from_local(layout)
        }
    }

    #[inline(always)]
    pub fn init_object(&mut self, addr: Address, encode: u8) {
        unsafe {
            *self
                .alloc_map
                .add(addr.diff(self.space_start) / size_of::<*mut ()>()) = encode;
        }
    }

    #[inline(never)]
    pub fn init_object_no_inline(&mut self, addr: Address, encode: u8) {
        self.init_object(addr, encode);
    }

    #[cold]
    pub fn try_alloc_from_local(&mut self, layout: Layout) -> Address {
        // println!("Trying to allocate from local");

        if self.line < immix::LINES_IN_BLOCK {
            let opt_next_available_line = {
                let cur_line = self.line;
                self.block().get_next_available_line(cur_line)
            };

            match opt_next_available_line {
                Some(next_available_line) => {
                    // println!("next available line is {}", next_available_line);

                    // we can alloc from local blocks
                    let end_line = self.block().get_next_unavailable_line(next_available_line);

                    // println!("next unavailable line is {}", end_line);
                    self.cursor = self
                        .block()
                        .start()
                        .plus(next_available_line << immix::LOG_BYTES_IN_LINE);
                    self.limit = self
                        .block()
                        .start()
                        .plus(end_line << immix::LOG_BYTES_IN_LINE);
                    self.line = end_line;

                    // unsafe {
                    //     memsec::memset(self.cursor.to_ptr_mut(), 0, self.limit.diff(self.cursor));
                    // }

                    for line in next_available_line..end_line {
                        self.block()
                            .line_mark_table_mut()
                            .set(line, LineMark::FreshAlloc);
                    }

                    self.alloc(layout)
                }
                None => {
                    // println!("no available line in current block");
                    self.alloc_from_global(layout)
                }
            }
        } else {
            // we need to alloc from global space
            self.alloc_from_global(layout)
        }
    }

    fn alloc_from_global(&mut self, layout: Layout) -> Address {
        trace!("Mutator{:?}: slowpath: alloc_from_global", self.id);

        self.return_block();

        loop {
            // check if yield
            self.yieldpoint();

            let new_block: Option<Box<ImmixBlock>> = self.space.get_next_usable_block();

            match new_block {
                Some(b) => {
                    self.block = Some(b);
                    self.cursor = self.block().start();
                    self.limit = self.block().start();
                    self.line = 0;

                    return self.alloc(layout);
                }
                None => {
                    continue;
                }
            }
        }
    }

    pub fn prepare_for_gc(&mut self) {
        self.return_block();
    }

    pub fn id(&self) -> Index {
        self.id
    }

    fn return_block(&mut self) {
        if self.block.is_some() {
            self.space.return_used_block(self.block.take().unwrap());
        }
    }
    fn block(&mut self) -> &mut ImmixBlock {
        self.block.as_mut().unwrap()
    }

    pub fn print_object(&self, obj: Address, length: usize) {
        ImmixMutatorLocal::print_object_static(obj, length);
    }

    pub fn print_object_static(obj: Address, length: usize) {
        println!("===Object {:#X} size: {} bytes===", obj, length);
        let mut cur_addr = obj;
        while cur_addr < obj.plus(length) {
            println!("Address: {:#X}   {:#X}", cur_addr, unsafe {
                *cur_addr.to_ptr::<u64>()
            });
            cur_addr = cur_addr.plus(8);
        }
        println!("----");
        println!("=========");
    }
}

impl ImmixMutatorGlobal {
    pub fn new() -> ImmixMutatorGlobal {
        ImmixMutatorGlobal {
            take_yield: AtomicBool::new(false),
            still_blocked: AtomicBool::new(false),
        }
    }

    #[inline(always)]
    pub fn is_still_blocked(&self) -> bool {
        self.still_blocked.load(Ordering::SeqCst)
    }
    pub fn set_still_blocked(&self, b: bool) {
        self.still_blocked.store(b, Ordering::SeqCst);
    }

    pub fn set_take_yield(&self, b: bool) {
        self.take_yield.store(b, Ordering::SeqCst);
    }
    #[inline(always)]
    pub fn take_yield(&self) -> bool {
        self.take_yield.load(Ordering::SeqCst)
    }
}

impl fmt::Display for ImmixMutatorLocal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.cursor.to_ptr::<()>().is_null() {
            write!(f, "Mutator (not initialized)")
        } else {
            writeln!(f, "Mutator:")?;
            writeln!(f, "cursor= {:#X}", self.cursor)?;
            writeln!(f, "limit = {:#X}", self.limit)?;
            writeln!(f, "line  = {}", self.line)?;
            write!(f, "block = {}", self.block.as_ref().unwrap())
        }
    }
}
