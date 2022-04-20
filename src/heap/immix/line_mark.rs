use crate::common::Address;
use crate::heap::immix;

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::{self, Debug, Formatter};

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum LineMark {
    Free,
    Live,
    FreshAlloc,
    ConservLive,
    PrevLive,
}

// this table will be accessed through unsafe raw pointers. since Rust doesn't provide a data structure for such guarantees:
// 1. Non-overlapping segments of this table may be accessed concurrently from different mutator threads
// 2. One element may be written into at the same time by different gc threads during tracing

#[derive(Clone)]
pub struct LineMarkTable {
    space_start: Address,
    ptr: *mut LineMark,
    len: usize,
}

impl LineMarkTable {
    pub fn new(space_start: Address, space_end: Address) -> LineMarkTable {
        let line_mark_table_len = space_end.diff(space_start) / immix::BYTES_IN_LINE;
        let line_mark_table = {
            // TODO: This could likely be replaced with a Vec
            let layout = Layout::array::<LineMark>(line_mark_table_len).unwrap();
            let ret = unsafe { System.alloc(layout) as *mut LineMark };
            let mut cursor = ret;

            for _ in 0..line_mark_table_len {
                unsafe {
                    *cursor = LineMark::Free;
                }
                cursor = unsafe { cursor.offset(1) };
            }

            ret
        };

        LineMarkTable {
            space_start,
            ptr: line_mark_table,
            len: line_mark_table_len,
        }
    }

    pub fn take_slice(&mut self, start: usize, len: usize) -> LineMarkTableSlice {
        LineMarkTableSlice {
            ptr: unsafe { self.ptr.add(start) },
            len,
        }
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn get(&self, index: usize) -> LineMark {
        debug_assert!(index <= self.len);
        unsafe { *self.ptr.add(index) }
    }

    #[inline(always)]
    fn set(&self, index: usize, value: LineMark) {
        debug_assert!(index <= self.len);
        unsafe { *self.ptr.add(index) = value };
    }

    #[inline(always)]
    pub fn mark_line_live(&self, addr: Address) {
        let line_table_index = addr.diff(self.space_start) >> immix::LOG_BYTES_IN_LINE;

        self.set(line_table_index, LineMark::Live);

        if line_table_index < self.len - 1 {
            self.set(line_table_index + 1, LineMark::ConservLive);
        }
    }

    #[inline(always)]
    pub fn mark_line_live2(&self, space_start: Address, addr: Address) {
        let line_table_index = addr.diff(space_start) >> immix::LOG_BYTES_IN_LINE;

        self.set(line_table_index, LineMark::Live);

        if line_table_index < self.len - 1 {
            self.set(line_table_index + 1, LineMark::ConservLive);
        }
    }
}

impl Drop for LineMarkTable {
    fn drop(&mut self) {
        let layout = Layout::array::<LineMark>(self.len).unwrap();
        unsafe { System.dealloc(self.ptr as *mut u8, layout) }
    }
}

impl Debug for LineMarkTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", self.ptr)
    }
}

#[derive(Clone)]
pub struct LineMarkTableSlice {
    ptr: *mut LineMark,
    len: usize,
}

impl LineMarkTableSlice {
    #[inline(always)]
    pub fn get(&self, index: usize) -> LineMark {
        debug_assert!(index <= self.len);
        unsafe { *self.ptr.add(index) }
    }
    #[inline(always)]
    pub fn set(&mut self, index: usize, value: LineMark) {
        debug_assert!(index <= self.len);
        unsafe { *self.ptr.add(index) = value };
    }
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get_next_available_line(&self, cur_line: usize) -> Option<usize> {
        let mut i = cur_line;
        while i < self.len {
            match self.get(i) {
                LineMark::Free => return Some(i),
                _ => i += 1,
            }
        }
        None
    }

    // TODO: This looks like it should return an Option
    pub fn get_next_unavailable_line(&self, cur_line: usize) -> usize {
        let mut i = cur_line;
        while i < self.len {
            match self.get(i) {
                LineMark::Free => i += 1,
                _ => return i,
            }
        }
        i
    }
}

impl Debug for LineMarkTableSlice {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", self.ptr)
    }
}
