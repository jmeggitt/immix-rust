use crate::heap::freelist::FreeListSpace;
use crate::heap::immix::ImmixLineMarkTable;
use crate::heap::immix::ImmixMutatorLocal;
use crate::heap::immix::ImmixSpace;
use crate::heap::immix::MUTATORS;
use crate::heap::immix::N_MUTATORS;
use crate::objectmodel;
use std::arch::asm;
use std::ptr::null_mut;

use crate::common;
use crate::common::AddressMap;
use crate::common::{Address, ObjectReference};

use lazy_static::lazy_static;
use log::trace;
use parking_lot::{Condvar, Mutex, RwLock};
use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};
use std::sync::Arc;

#[cfg(feature = "mt-trace")]
use crossbeam::deque::{Steal, Stealer, Worker};
#[cfg(feature = "mt-trace")]
use std::sync::mpsc;
#[cfg(feature = "mt-trace")]
use std::sync::mpsc::channel;
#[cfg(feature = "mt-trace")]
use std::thread;

use std::sync::atomic;

lazy_static! {
    static ref STW_COND: Arc<(Mutex<usize>, Condvar)> = Arc::new((Mutex::new(0), Condvar::new()));
    static ref GC_CONTEXT: RwLock<GCContext> = RwLock::new(GCContext {
        immix_space: None,
        lo_space: None
    });
    static ref ROOTS: RwLock<Vec<ObjectReference>> = RwLock::new(vec![]);
}

static CONTROLLER: AtomicIsize = atomic::AtomicIsize::new(0);
const NO_CONTROLLER: isize = -1;

pub struct GCContext {
    immix_space: Option<Arc<ImmixSpace>>,
    lo_space: Option<Arc<RwLock<FreeListSpace>>>,
}

pub fn init(immix_space: Arc<ImmixSpace>, lo_space: Arc<RwLock<FreeListSpace>>) {
    CONTROLLER.store(NO_CONTROLLER, Ordering::SeqCst);
    let mut gccontext = GC_CONTEXT.write();
    gccontext.immix_space = Some(immix_space);
    gccontext.lo_space = Some(lo_space);
}

pub fn trigger_gc() {
    trace!("Triggering GC...");

    for m in MUTATORS.write().iter_mut() {
        if m.is_some() {
            m.as_mut().unwrap().set_take_yield(true);
        }
    }
}

fn immmix_get_stack_ptr() -> *mut () {
    let mut ret: *mut ();

    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("mov {0}, rsp", out(reg) ret);
    }

    #[cfg(target_arch = "x86")]
    unsafe {
        asm!("mov {0}, esp", out(reg) ret);
    }

    // Bootstrap hack to get stack pointer. Should work on any system... probably
    if cfg!(not(any(target_arch = "x86_64", target_arch = "x86"))) {
        ret = null_mut();
        ret = &mut ret as *mut _ as *mut ();
    }

    ret
}

thread_local!(static LOW_WATER_MARK: AtomicPtr<()> = AtomicPtr::new(null_mut()));

pub extern "C" fn set_low_water_mark() {
    LOW_WATER_MARK.with(|f| f.store(immmix_get_stack_ptr(), Ordering::Relaxed));
}

fn get_low_water_mark() -> Address {
    Address::from_ptr(LOW_WATER_MARK.with(|v| v.load(Ordering::Relaxed)))
}

#[inline(always)]
pub fn is_valid_object(
    addr: Address,
    start: Address,
    end: Address,
    live_map: &AddressMap<u8>,
) -> bool {
    if addr >= end || addr < start {
        return false;
    }

    common::test_nth_bit(live_map.get(addr), objectmodel::OBJ_START_BIT)
}

pub fn stack_scan() -> Vec<ObjectReference> {
    let stack_ptr: Address = Address::from_ptr(immmix_get_stack_ptr());
    let low_water_mark: Address = get_low_water_mark();

    let mut cursor = stack_ptr;
    let mut ret = vec![];

    let gccontext = GC_CONTEXT.read();
    let immix_space = gccontext.immix_space.as_ref().unwrap();

    while cursor < low_water_mark {
        let value: Address = unsafe { cursor.load::<Address>() };

        if is_valid_object(
            value,
            immix_space.start(),
            immix_space.end(),
            &immix_space.alloc_map,
        ) {
            ret.push(unsafe { value.to_object_reference() });
        }

        cursor = cursor.plus(common::POINTER_SIZE);
    }

    let roots_from_stack = ret.len();

    macro_rules! store_registers {
        ($arch:literal: $($reg:ident)+) => {
            #[cfg(target_arch = $arch)]
            #[allow(non_snake_case)]
            unsafe {
                $(store_registers!(@fetch $reg);)+
                $(store_registers!(@store $reg);)+
            }
        };
        (@fetch $reg:ident) => {
            let mut $reg: *mut ();
            asm!(concat!("mov {0}, ", stringify!($reg)), out(reg) $reg);
        };
        (@store $reg:ident) => {
            if is_valid_object(
                Address::from_ptr($reg),
                immix_space.start(),
                immix_space.end(),
                &immix_space.alloc_map,
            ) {
                ret.push(Address::from_ptr($reg).to_object_reference());
            }
        };
    }

    // This also checks registers that wouldn't make sense to store an object pointer in (ex: stack
    // pointers). Maybe consider removing later.
    store_registers!("x86_64": rax rbx rcx rdx rbp rsp rsi rdi r8 r9 r10 r11 r12 r13 r14 r15);
    store_registers!("x86": eax ebx ecx edx esi edi esp ebp);

    // TODO: Not sure if this is correct, but include all architectures with asm support
    store_registers!("arm": R0 R1 R2 R3 R4 R5 R6 R7 R8 R9 R10 R11 R12 SP LR);
    store_registers!("aarch64": W0 W1 W2 W3 W4 W5 W6 W7 W8 W9 W10 W11 W12 W13 W14 W15 W16 W17 W18
        W19 W20 W21 W22 W23 W24 W25 W26 W27 W28 W29 W30);
    store_registers!("riscv32": x1 x2 x3 x4 x5 x6 x7 x8 x9 x10 x11 x12 x13 x14 x15 x16 x17 x18 x19
        x20 x21 x22 x23 x24 x25 x26 x27 x28 x29 x30 x31);
    store_registers!("riscv64": x1 x2 x3 x4 x5 x6 x7 x8 x9 x10 x11 x12 x13 x14 x15 x16 x17 x18 x19
        x20 x21 x22 x23 x24 x25 x26 x27 x28 x29 x30 x31);

    let roots_from_registers = ret.len() - roots_from_stack;

    trace!(
        "roots: {} from stack, {} from registers",
        roots_from_stack,
        roots_from_registers
    );

    ret
}

#[inline(never)]
pub fn sync_barrier(mutator: &mut ImmixMutatorLocal) {
    let controller_id = CONTROLLER.compare_exchange(
        NO_CONTROLLER,
        mutator.id() as isize,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );

    trace!(
        "Mutator{} saw the controller is {:?}",
        mutator.id(),
        controller_id
    );

    // prepare the mutator for gc - return current block (if it has)
    mutator.prepare_for_gc();

    // scan its stack
    let mut thread_roots = stack_scan();
    ROOTS.write().append(&mut thread_roots);

    // user thread call back to prepare for gc
    //    USER_THREAD_PREPARE_FOR_GC.read()();

    match controller_id {
        Err(controller) => {
            assert_ne!(controller, mutator.id() as isize);

            // this thread will block
            block_current_thread(mutator);

            // reset current mutator
            mutator.reset();
        }
        Ok(_) => {
            // this thread is controller
            // other threads should block

            // wait for all mutators to be blocked
            let &(ref lock, ref cvar) = &*STW_COND.clone();
            let mut count = 0;

            trace!("expect {} mutators to park", *N_MUTATORS.read() - 1);
            while count < *N_MUTATORS.read() - 1 {
                let new_count = { *lock.lock() };
                if new_count != count {
                    count = new_count;
                    trace!("count = {}", count);
                }
            }

            trace!("everyone stopped, gc will start");

            // roots->trace->sweep
            gc();

            // mutators will resume
            CONTROLLER.store(NO_CONTROLLER, Ordering::SeqCst);
            for t in MUTATORS.write().iter_mut() {
                if t.is_some() {
                    let t_mut = t.as_mut().unwrap();
                    t_mut.set_take_yield(false);
                    t_mut.set_still_blocked(false);
                }
            }
            // every mutator thread will reset themselves, so only reset current mutator here
            mutator.reset();

            // resume
            {
                let mut count = lock.lock();
                *count = 0;
                cvar.notify_all();
            }
        }
    }
}

fn block_current_thread(mutator: &mut ImmixMutatorLocal) {
    trace!("Mutator{} blocked", mutator.id());

    let &(ref lock, ref cvar) = &*STW_COND.clone();
    let mut count = lock.lock();
    *count += 1;

    mutator.global.set_still_blocked(true);

    while mutator.global.is_still_blocked() {
        cvar.wait(&mut count);
    }

    trace!("Mutator{} unblocked", mutator.id());
}

pub static GC_COUNT: atomic::AtomicUsize = atomic::AtomicUsize::new(0);

fn gc() {
    GC_COUNT.store(
        GC_COUNT.load(atomic::Ordering::SeqCst) + 1,
        atomic::Ordering::SeqCst,
    );

    trace!("GC starts");

    // creates root deque
    let roots: &mut Vec<ObjectReference> = &mut ROOTS.write();

    // mark & trace
    {
        let gccontext = GC_CONTEXT.read();
        let immix_space = gccontext.immix_space.as_ref().unwrap();

        start_trace(roots, immix_space.clone());
    }

    trace!("trace done");

    // sweep
    {
        let mut gccontext = GC_CONTEXT.write();
        let immix_space = gccontext.immix_space.as_mut().unwrap();

        immix_space.sweep();
    }

    objectmodel::flip_mark_state();
    trace!("GC finishes");
}

pub const MULTI_THREAD_TRACE_THRESHOLD: usize = 10;

pub const PUSH_BACK_THRESHOLD: usize = 50;
pub static GC_THREADS: atomic::AtomicUsize = atomic::AtomicUsize::new(0);

#[inline(never)]
#[cfg(feature = "mt-trace")]
pub fn start_trace(work_stack: &mut Vec<ObjectReference>, immix_space: Arc<ImmixSpace>) {
    // creates root deque
    let worker = Worker::new_lifo();
    let stealer = worker.stealer();

    while !work_stack.is_empty() {
        worker.push(work_stack.pop().unwrap());
    }

    loop {
        let (sender, receiver) = channel::<ObjectReference>();

        let mut gc_threads = vec![];
        for _ in 0..GC_THREADS.load(atomic::Ordering::SeqCst) {
            let new_immix_space = immix_space.clone();
            let new_stealer = stealer.clone();
            let new_sender = sender.clone();
            let t = thread::spawn(move || {
                start_steal_trace(new_stealer, new_sender, new_immix_space);
            });
            gc_threads.push(t);
        }

        // only stealers own sender, when all stealers quit, the following loop finishes
        drop(sender);

        loop {
            let recv = receiver.recv();
            match recv {
                Ok(obj) => worker.push(obj),
                Err(_) => break,
            }
        }

        match worker.pop() {
            Some(obj_ref) => worker.push(obj_ref),
            None => break,
        }
    }
}

#[inline(never)]
#[cfg(not(feature = "mt-trace"))]
pub fn start_trace(local_queue: &mut Vec<ObjectReference>, immix_space: Arc<ImmixSpace>) {
    let mark_state = objectmodel::MARK_STATE.load(Ordering::SeqCst) as u8;

    while !local_queue.is_empty() {
        unsafe {
            trace_object(
                local_queue.pop().unwrap(),
                local_queue,
                immix_space.alloc_map.ptr,
                immix_space.trace_map.ptr,
                &immix_space.line_mark_table,
                immix_space.start(),
                immix_space.end(),
                mark_state,
            );
        }
    }
}

#[cfg(feature = "mt-trace")]
fn start_steal_trace(
    stealer: Stealer<ObjectReference>,
    job_sender: mpsc::Sender<ObjectReference>,
    immix_space: Arc<ImmixSpace>,
) {
    let mut local_queue = vec![];

    let line_mark_table = &immix_space.line_mark_table;
    let (alloc_map, trace_map) = (immix_space.alloc_map.ptr, immix_space.trace_map.ptr);
    let (space_start, space_end) = (immix_space.start(), immix_space.end());
    let mark_state = objectmodel::MARK_STATE.load(Ordering::SeqCst) as u8;

    loop {
        let work = {
            if !local_queue.is_empty() {
                local_queue.pop().unwrap()
            } else {
                let work = stealer.steal();
                match work {
                    Steal::Empty => return,
                    Steal::Retry => continue,
                    Steal::Success(obj) => obj,
                }
            }
        };

        unsafe {
            steal_trace_object(
                work,
                &mut local_queue,
                &job_sender,
                alloc_map,
                trace_map,
                line_mark_table,
                space_start,
                space_end,
                mark_state,
            );
        }
    }
}

#[inline(always)]
#[cfg(feature = "mt-trace")]
pub unsafe fn steal_trace_object(
    obj: ObjectReference,
    local_queue: &mut Vec<ObjectReference>,
    job_sender: &mpsc::Sender<ObjectReference>,
    alloc_map: *mut u8,
    trace_map: *mut u8,
    line_mark_table: &ImmixLineMarkTable,
    immix_start: Address,
    immix_end: Address,
    mark_state: u8,
) {
    objectmodel::mark_as_traced(trace_map, immix_start, obj, mark_state);

    let addr = obj.to_address();

    if addr >= immix_start && addr < immix_end {
        line_mark_table.mark_line_live(addr);
    } else {
        // freelist mark
    }

    let mut base = addr;
    loop {
        let value = objectmodel::get_ref_byte(alloc_map, immix_start, obj);
        let (ref_bits, short_encode) = (
            common::lower_bits(value, objectmodel::REF_BITS_LEN),
            common::test_nth_bit(value, objectmodel::SHORT_ENCODE_BIT),
        );
        match ref_bits {
            0b0000_0001 => {
                steal_process_edge(
                    base,
                    local_queue,
                    trace_map,
                    immix_start,
                    job_sender,
                    mark_state,
                );
            }
            0b0000_0011 => {
                steal_process_edge(
                    base,
                    local_queue,
                    trace_map,
                    immix_start,
                    job_sender,
                    mark_state,
                );
                steal_process_edge(
                    base.plus(8),
                    local_queue,
                    trace_map,
                    immix_start,
                    job_sender,
                    mark_state,
                );
            }
            0b0000_1111 => {
                steal_process_edge(
                    base,
                    local_queue,
                    trace_map,
                    immix_start,
                    job_sender,
                    mark_state,
                );
                steal_process_edge(
                    base.plus(8),
                    local_queue,
                    trace_map,
                    immix_start,
                    job_sender,
                    mark_state,
                );
                steal_process_edge(
                    base.plus(16),
                    local_queue,
                    trace_map,
                    immix_start,
                    job_sender,
                    mark_state,
                );
                steal_process_edge(
                    base.plus(24),
                    local_queue,
                    trace_map,
                    immix_start,
                    job_sender,
                    mark_state,
                );
            }
            _ => {
                panic!("unexpcted ref_bits patterns: {:b}", ref_bits);
            }
        }

        assert!(short_encode);
        if short_encode {
            return;
        } else {
            base = base.plus(objectmodel::REF_BITS_LEN * 8);
        }
    }
}

#[inline(always)]
#[cfg(feature = "mt-trace")]
pub unsafe fn steal_process_edge(
    addr: Address,
    local_queue: &mut Vec<ObjectReference>,
    trace_map: *mut u8,
    immix_start: Address,
    job_sender: &mpsc::Sender<ObjectReference>,
    mark_state: u8,
) {
    let obj_addr = addr.load::<ObjectReference>();

    if !obj_addr.to_address().is_zero()
        && !objectmodel::is_traced(trace_map, immix_start, obj_addr, mark_state)
    {
        if local_queue.len() >= PUSH_BACK_THRESHOLD {
            job_sender.send(obj_addr).unwrap();
        } else {
            local_queue.push(obj_addr);
        }
    }
}

#[inline(always)]
pub unsafe fn trace_object(
    obj: ObjectReference,
    local_queue: &mut Vec<ObjectReference>,
    alloc_map: *mut u8,
    trace_map: *mut u8,
    line_mark_table: &ImmixLineMarkTable,
    immix_start: Address,
    immix_end: Address,
    mark_state: u8,
) {
    objectmodel::mark_as_traced(trace_map, immix_start, obj, mark_state);

    let addr = obj.to_address();

    if addr >= immix_start && addr < immix_end {
        line_mark_table.mark_line_live(addr);
    } else {
        // freelist mark
    }

    let mut base = addr;
    loop {
        let value = objectmodel::get_ref_byte(alloc_map, immix_start, obj);
        let (ref_bits, short_encode) = (
            common::lower_bits(value, objectmodel::REF_BITS_LEN),
            common::test_nth_bit(value, objectmodel::SHORT_ENCODE_BIT),
        );

        match ref_bits {
            0b0000_0001 => {
                process_edge(base, local_queue, trace_map, immix_start, mark_state);
            }
            0b0000_0011 => {
                process_edge(base, local_queue, trace_map, immix_start, mark_state);
                process_edge(
                    base.plus(8),
                    local_queue,
                    trace_map,
                    immix_start,
                    mark_state,
                );
            }
            0b0000_1111 => {
                process_edge(base, local_queue, trace_map, immix_start, mark_state);
                process_edge(
                    base.plus(8),
                    local_queue,
                    trace_map,
                    immix_start,
                    mark_state,
                );
                process_edge(
                    base.plus(16),
                    local_queue,
                    trace_map,
                    immix_start,
                    mark_state,
                );
                process_edge(
                    base.plus(24),
                    local_queue,
                    trace_map,
                    immix_start,
                    mark_state,
                );
            }
            _ => {
                panic!("unexpcted ref_bits patterns: {:b}", ref_bits);
            }
        }

        debug_assert!(short_encode);
        if short_encode {
            return;
        } else {
            base = base.plus(objectmodel::REF_BITS_LEN * 8);
        }
    }
}

#[inline(always)]
pub unsafe fn process_edge(
    addr: Address,
    local_queue: &mut Vec<ObjectReference>,
    trace_map: *mut u8,
    space_start: Address,
    mark_state: u8,
) {
    let obj_addr: ObjectReference = addr.load();

    if !obj_addr.to_address().is_zero()
        && !objectmodel::is_traced(trace_map, space_start, obj_addr, mark_state)
    {
        local_queue.push(obj_addr);
    }
}
