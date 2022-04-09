use std::alloc::{GlobalAlloc, Layout, System};

use crate::common::Address;
use crate::common::LOG_POINTER_SIZE;

#[derive(Clone)]
pub struct AddressMap<T> {
    start: Address,
    end: Address,

    pub ptr: *mut T,
    layout: Layout,
}

impl<T> AddressMap<T> {
    pub fn new(start: Address, end: Address) -> AddressMap<T> {
        let len = end.diff(start) >> LOG_POINTER_SIZE;

        // TODO: This should be a regular Vec
        let layout = Layout::array::<T>(len).unwrap();
        let ptr = unsafe { System.alloc_zeroed(layout) as *mut T };

        AddressMap {
            start,
            end,
            ptr,
            layout,
        }
    }

    #[inline(always)]
    pub fn set(&self, addr: Address, value: T) {
        let index = (addr.diff(self.start) >> LOG_POINTER_SIZE) as isize;
        unsafe { *self.ptr.offset(index) = value };
    }
}

impl<T: Copy> AddressMap<T> {
    #[inline(always)]
    pub fn get(&self, addr: Address) -> T {
        let index = (addr.diff(self.start) >> LOG_POINTER_SIZE) as isize;
        unsafe { *self.ptr.offset(index) }
    }
}

impl<T> Drop for AddressMap<T> {
    fn drop(&mut self) {
        unsafe {
            System.dealloc(self.ptr as *mut u8, self.layout);
        }
    }
}
