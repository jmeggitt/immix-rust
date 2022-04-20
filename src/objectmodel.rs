use crate::common::ObjectReference;
use std::mem::size_of;

use crate::common::Address;

pub const REF_BITS_LEN: usize = 6;
pub const OBJ_START_BIT: usize = 6;
pub const SHORT_ENCODE_BIT: usize = 7;

#[inline(always)]
pub unsafe fn get_ref_byte(alloc_map: *mut u8, space_start: Address, obj: ObjectReference) -> u8 {
    *alloc_map.add(obj.to_address().diff(space_start) / size_of::<*mut ()>())
}
