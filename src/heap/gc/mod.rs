use crate::heap::immix::ImmixMutatorLocal;
use crate::heap::immix::ImmixSpace;
use crate::heap::immix::MUTATORS;
use crate::heap::immix::N_MUTATORS;
use crate::objectmodel;
use std::arch::asm;
use std::mem::size_of;
use std::ptr::null_mut;

use crate::common::AddressMap;
use crate::common::{Address, ObjectReference};

use lazy_static::lazy_static;
use log::trace;
use parking_lot::{Condvar, Mutex, RwLock};
use std::sync::atomic::{AtomicIsize, AtomicPtr, AtomicUsize, Ordering};
use std::sync::Arc;

mod multi_thread_trace;
pub use multi_thread_trace::start_trace;

lazy_static! {
    static ref STW_COND: Arc<(Mutex<usize>, Condvar)> = Arc::new((Mutex::new(0), Condvar::new()));
    static ref ROOTS: RwLock<Vec<ObjectReference>> = RwLock::new(vec![]);
}

static CONTROLLER: AtomicIsize = AtomicIsize::new(NO_CONTROLLER);
const NO_CONTROLLER: isize = -1;

pub fn trigger_gc() {
    trace!("Triggering GC...");

    for (_, mutator) in MUTATORS.write().iter_mut() {
        mutator.set_take_yield(true);
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
fn is_valid_object(addr: Address, start: Address, end: Address, live_map: &AddressMap<u8>) -> bool {
    if addr >= end || addr < start {
        return false;
    }

    live_map.get(addr) & (1 << objectmodel::OBJ_START_BIT) != 0
}

fn stack_scan(immix_space: &ImmixSpace) -> Vec<ObjectReference> {
    let stack_ptr: Address = Address::from_ptr(immmix_get_stack_ptr());
    let low_water_mark: Address = get_low_water_mark();

    let mut cursor = stack_ptr;
    let mut ret = vec![];

    while cursor < low_water_mark {
        let value: Address = unsafe { *cursor.to_ptr::<Address>() };

        if is_valid_object(
            value,
            immix_space.start(),
            immix_space.end(),
            &immix_space.alloc_map,
        ) {
            ret.push(unsafe { value.to_object_reference() });
        }

        cursor = cursor.plus(size_of::<*mut ()>());
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
    let (direct_index, _) = mutator.id().into_raw_parts();
    let controller_id = CONTROLLER.compare_exchange(
        NO_CONTROLLER,
        direct_index as isize,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );

    trace!(
        "Mutator{:?} saw the controller is {:?}",
        mutator.id(),
        controller_id
    );

    // prepare the mutator for gc - return current block (if it has)
    mutator.prepare_for_gc();

    // scan its stack
    let mut thread_roots = stack_scan(&*mutator.immix_space());
    ROOTS.write().append(&mut thread_roots);

    // user thread call back to prepare for gc
    //    USER_THREAD_PREPARE_FOR_GC.read()();

    match controller_id {
        Err(controller) => {
            assert_ne!(controller, direct_index as isize);

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

            trace!(
                "expect {} mutators to park",
                N_MUTATORS.load(Ordering::SeqCst) - 1
            );
            while count < N_MUTATORS.load(Ordering::SeqCst) - 1 {
                let new_count = { *lock.lock() };
                if new_count != count {
                    count = new_count;
                    trace!("count = {}", count);
                }
            }

            trace!("everyone stopped, gc will start");

            // roots->trace->sweep
            gc(mutator.immix_space());

            // mutators will resume
            CONTROLLER.store(NO_CONTROLLER, Ordering::SeqCst);
            for (_, mutator) in MUTATORS.write().iter_mut() {
                mutator.set_take_yield(false);
                mutator.set_still_blocked(false);
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
    trace!("Mutator{:?} blocked", mutator.id());

    let &(ref lock, ref cvar) = &*STW_COND.clone();
    let mut count = lock.lock();
    *count += 1;

    mutator.global.set_still_blocked(true);

    while mutator.global.is_still_blocked() {
        cvar.wait(&mut count);
    }

    trace!("Mutator{:?} unblocked", mutator.id());
}

static GC_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn gc_count() -> usize {
    GC_COUNT.load(Ordering::SeqCst)
}

fn gc(immix_space: Arc<ImmixSpace>) {
    GC_COUNT.fetch_add(1, Ordering::SeqCst);

    trace!("GC starts");

    // creates root deque
    let roots: &mut Vec<ObjectReference> = &mut ROOTS.write();

    // mark & trace
    start_trace(roots, immix_space.clone());

    trace!("trace done");

    // sweep
    immix_space.sweep();

    immix_space.trace_map.flip_mark_state();
    trace!("GC finishes");
}
