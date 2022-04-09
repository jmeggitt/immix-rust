use crate::common::{lower_bits, test_nth_bit, Address, ObjectReference};
use crate::heap::immix::{ImmixLineMarkTable, ImmixSpace};
use crate::objectmodel;
use crate::objectmodel::MARK_STATE;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[inline(never)]
pub fn start_trace(local_queue: &mut Vec<ObjectReference>, immix_space: Arc<ImmixSpace>) {
    let mark_state = MARK_STATE.load(Ordering::SeqCst) as u8;

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

#[inline(always)]
unsafe fn trace_object(
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
            lower_bits(value, objectmodel::REF_BITS_LEN),
            test_nth_bit(value, objectmodel::SHORT_ENCODE_BIT),
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
unsafe fn process_edge(
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
