mod immix_mutator;
mod immix_space;
mod line_mark;

pub use self::immix_mutator::MUTATORS;
pub use self::immix_mutator::N_MUTATORS;
pub use immix_mutator::ImmixMutatorLocal;
pub use immix_space::ImmixSpace;

const LOG_BYTES_IN_LINE: usize = 8;
const BYTES_IN_LINE: usize = 1 << LOG_BYTES_IN_LINE;
const LOG_BYTES_IN_BLOCK: usize = 16;
const BYTES_IN_BLOCK: usize = 1 << LOG_BYTES_IN_BLOCK;
const LINES_IN_BLOCK: usize = 1 << (LOG_BYTES_IN_BLOCK - LOG_BYTES_IN_LINE);

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum BlockMark {
    Usable,
    Full,
}
