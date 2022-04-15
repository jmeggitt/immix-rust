use crate::{common, objectmodel, ImmixSpace, ObjectReference};
use crossbeam::deque::Injector;
use crossbeam::deque::{Steal, Worker};
use std::hint::spin_loop;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

#[inline(never)]
pub fn start_trace(work_stack: &mut Vec<ObjectReference>, immix_space: Arc<ImmixSpace>) {
    let injector = Arc::new(Injector::new());

    let cpus = num_cpus::get();
    let active_threads = Arc::new(AtomicUsize::new(cpus));

    // Fill in initial injector items
    work_stack.drain(..).for_each(|x| injector.push(x));

    // Launch trace threads
    let mut join_handles = Vec::with_capacity(cpus);
    for _ in 0..cpus {
        let injector_handle = injector.clone();
        let active_threads_handle = active_threads.clone();
        let immix_handle = immix_space.clone();
        join_handles.push(thread::spawn(move || {
            worker_batch_steal_trace(injector_handle, active_threads_handle, immix_handle)
        }));
    }

    // Wait for all threads to finish
    join_handles
        .into_iter()
        .map(JoinHandle::join)
        .for_each(Result::unwrap);
}

fn worker_batch_steal_trace(
    injector: Arc<Injector<ObjectReference>>,
    active_threads: Arc<AtomicUsize>,
    immix_space: Arc<ImmixSpace>,
) {
    let worker = Worker::new_fifo();

    let trace_map = &immix_space.trace_map;
    let alloc_map = immix_space.alloc_map.ptr;
    let line_mark_table = &immix_space.line_mark_table;
    let (space_start, space_end) = (immix_space.start(), immix_space.end());

    loop {
        let next = match worker.pop() {
            Some(v) => v,
            None => loop {
                match injector.steal_batch_and_pop(&worker) {
                    Steal::Empty => {
                        active_threads.fetch_sub(1, Ordering::SeqCst);

                        while injector.is_empty() {
                            if active_threads.load(Ordering::SeqCst) == 0 {
                                return;
                            }
                            spin_loop();
                        }

                        active_threads.fetch_add(1, Ordering::SeqCst);
                    }
                    Steal::Success(v) => break v,
                    Steal::Retry => continue,
                }
            },
        };

        let addr = next.to_address();
        assert!(addr >= space_start && addr < space_end);
        trace_map.mark_as_traced(addr.to_ptr::<()>());
        line_mark_table.mark_line_live(addr);

        let mut base = addr;
        loop {
            let value = unsafe { objectmodel::get_ref_byte(alloc_map, space_start, next) };
            let (ref_bits, short_encode) = (
                common::lower_bits(value, objectmodel::REF_BITS_LEN),
                common::test_nth_bit(value, objectmodel::SHORT_ENCODE_BIT),
            );
            macro_rules! steal_process_edge {
                    ($($offset:literal)+) => {{$(
                        let obj_addr = unsafe { base.plus($offset).load::<ObjectReference>() };
                        if trace_map.is_untraced_and_valid(obj_addr.value() as *const ()) {
                            injector.push(obj_addr);
                        }
                    )+}};
                }

            match ref_bits {
                0b0000_0001 => steal_process_edge!(0),
                0b0000_0011 => steal_process_edge!(0 8),
                0b0000_1111 => steal_process_edge!(0 8 16 24),
                _ => panic!("unexpected ref_bits patterns: {:0b}", ref_bits),
            }

            assert!(short_encode);
            if short_encode {
                break;
            } else {
                base = base.plus(objectmodel::REF_BITS_LEN * 8);
            }
        }
    }
}
