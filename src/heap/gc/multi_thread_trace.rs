use crate::common::Address;
use crate::heap::immix::ImmixLineMarkTable;
use crate::{common, objectmodel, ImmixSpace, ObjectReference};
#[cfg(feature = "mt-trace")]
use crossbeam::deque::{Steal, Stealer, Worker};
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "mt-trace")]
use std::sync::mpsc;
#[cfg(feature = "mt-trace")]
use std::sync::mpsc::channel;
use std::sync::Arc;
#[cfg(feature = "mt-trace")]
use std::thread;

const PUSH_BACK_THRESHOLD: usize = 50;

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
        for _ in 0..num_cpus::get() {
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
unsafe fn steal_trace_object(
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
unsafe fn steal_process_edge(
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
