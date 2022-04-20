use std::fmt;
use std::ptr::null;

mod address_map;
pub use self::address_map::{AddressMap, SafeAddressMap, TraceMap};

const LOG_POINTER_SIZE: usize = 3;

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Address(usize);

impl Address {
    #[inline(always)]
    pub fn plus(&self, bytes: usize) -> Self {
        Address(self.0 + bytes)
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
    pub fn align_up(&self, align: usize) -> Address {
        Address((self.0 + align - 1) & !(align - 1))
    }

    #[inline(always)]
    pub unsafe fn to_object_reference(self) -> ObjectReference {
        ObjectReference(self.0)
    }

    #[inline(always)]
    pub fn from_ptr<T>(ptr: *const T) -> Address {
        Address(ptr as usize)
    }
    #[inline(always)]
    pub fn to_ptr<T>(self) -> *const T {
        self.0 as *const T
    }
    #[inline(always)]
    pub fn to_ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }
    #[inline(always)]
    pub fn as_usize(&self) -> usize {
        self.0
    }
    #[inline(always)]
    pub unsafe fn null() -> Address {
        Address(null::<()>() as usize)
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
    pub fn to_address(self) -> Address {
        Address(self.0)
    }

    pub fn as_usize(&self) -> usize {
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
