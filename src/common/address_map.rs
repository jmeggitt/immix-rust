use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::mem::size_of;
use std::ops::{Index, IndexMut};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::common::Address;
use crate::common::LOG_POINTER_SIZE;

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
        debug_assert!(addr < self.end);

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

pub struct SafeAddressMap<T> {
    offset: usize,
    addresses: Vec<T>,
}

impl<T> SafeAddressMap<T> {
    pub fn new_of<F: FnMut() -> T>(start: usize, end: usize, f: F) -> Self {
        let ptr_size = size_of::<*mut ()>();
        let len = ((end - start) + ptr_size - 1) / ptr_size;

        let mut addresses = Vec::with_capacity(len);
        addresses.resize_with(len, f);

        SafeAddressMap {
            offset: start,
            addresses,
        }
    }

    #[inline(always)]
    pub fn get(&self, index: usize) -> Option<&T> {
        let normalized_index = index.overflowing_sub(self.offset).0 / size_of::<*mut ()>();
        self.addresses.get(normalized_index)
    }
}

impl<T: Default> SafeAddressMap<T> {
    pub fn new(start: usize, end: usize) -> Self {
        Self::new_of(start, end, T::default)
    }
}

impl<T> Index<usize> for SafeAddressMap<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.addresses[(index - self.offset) / size_of::<*mut ()>()]
    }
}

impl<T> IndexMut<usize> for SafeAddressMap<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.addresses[(index - self.offset) / size_of::<*mut ()>()]
    }
}

pub struct TraceMap {
    map: SafeAddressMap<AtomicU8>,
    mark_state: Cell<u8>,
}

impl TraceMap {
    pub fn new(start: usize, end: usize) -> Self {
        TraceMap {
            map: SafeAddressMap::new(start, end),
            mark_state: Cell::new(0),
        }
    }

    pub fn flip_mark_state(&self) {
        self.mark_state.set(self.mark_state.get() ^ 1);
    }

    #[inline(always)]
    pub fn is_traced<T>(&self, ptr: *const T) -> bool {
        self.map[ptr as usize].load(Ordering::Relaxed) == self.mark_state.get()
    }

    #[inline(always)]
    pub fn is_untraced_and_valid<T>(&self, ptr: *const T) -> bool {
        match self.map.get(ptr as usize) {
            Some(v) => v.load(Ordering::Relaxed) != self.mark_state.get(),
            None => false,
        }
    }

    #[inline(always)]
    pub fn mark_as_traced<T>(&self, ptr: *const T) {
        self.map[ptr as usize].store(self.mark_state.get(), Ordering::Relaxed)
    }
}
