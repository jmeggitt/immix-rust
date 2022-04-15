use std::fmt;
use std::mem;

mod address_map;
pub use self::address_map::{AddressMap, SafeAddressMap, TraceMap};

pub const LOG_POINTER_SIZE: usize = 3;
pub const POINTER_SIZE: usize = 1 << LOG_POINTER_SIZE;

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Address(usize);

impl Address {
    #[inline(always)]
    pub fn plus(&self, bytes: usize) -> Self {
        Address(self.0 + bytes)
    }
    #[inline(always)]
    pub fn sub(&self, bytes: usize) -> Self {
        Address(self.0 - bytes)
    }
    #[inline(always)]
    pub fn offset<T>(&self, offset: isize) -> Self {
        Address((self.0 as isize + mem::size_of::<T>() as isize * offset) as usize)
    }
    #[inline(always)]
    pub fn diff(&self, another: Address) -> usize {
        debug_assert!(
            self.0 >= another.0,
            "for a.diff(b), a needs to be larger than b"
        );
        self.0 - another.0
    }

    #[inline(always)]
    pub unsafe fn load<T: Copy>(&self) -> T {
        *(self.0 as *mut T)
    }
    #[inline(always)]
    pub unsafe fn store<T>(&self, value: T) {
        *(self.0 as *mut T) = value;
    }
    #[inline(always)]
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }
    #[inline(always)]
    pub fn align_up(&self, align: usize) -> Address {
        Address((self.0 + align - 1) & !(align - 1))
    }

    pub fn is_aligned_to(&self, align: usize) -> bool {
        self.0 % align == 0
    }

    #[inline(always)]
    pub unsafe fn to_object_reference(&self) -> ObjectReference {
        mem::transmute(self.0)
    }
    #[inline(always)]
    pub fn from_ptr<T>(ptr: *const T) -> Address {
        Address(ptr as usize)
    }
    #[inline(always)]
    pub fn to_ptr<T>(&self) -> *const T {
        unsafe { mem::transmute(self.0) }
    }
    #[inline(always)]
    pub fn to_ptr_mut<T>(&self) -> *mut T {
        unsafe { mem::transmute(self.0) }
    }
    #[inline(always)]
    pub fn as_usize(&self) -> usize {
        self.0
    }
    #[inline(always)]
    pub unsafe fn zero() -> Address {
        Address(0)
    }
}

impl fmt::UpperHex for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:X}", self.0)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{:X}", self.0)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{:X}", self.0)
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectReference(usize);

impl ObjectReference {
    #[inline(always)]
    pub fn to_address(&self) -> Address {
        Address(self.0)
    }

    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.0 != 0
    }
    pub fn value(&self) -> usize {
        self.0
    }
}

impl fmt::UpperHex for ObjectReference {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:X}", self.0)
    }
}

impl fmt::Display for ObjectReference {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{:X}", self.0)
    }
}

impl fmt::Debug for ObjectReference {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{:X}", self.0)
    }
}

#[inline(always)]
pub fn test_nth_bit(value: u8, index: usize) -> bool {
    value & (1 << index) != 0
}

#[inline(always)]
pub fn lower_bits(value: u8, len: usize) -> u8 {
    value & ((1 << len) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_u8_bits() {
        let value: u8 = 0b1100_0011;

        assert_eq!(test_nth_bit(value, 6), true);

        assert_eq!(lower_bits(value, 6), 0b00_0011);
    }
}
