// TODO: Reduce the number of unsafe functions then remove this
#![allow(clippy::missing_safety_doc)]

mod common;
mod heap;
mod objectmodel;

// Items with must be re-exported
pub use common::{Address, ObjectReference};
pub use heap::*;
