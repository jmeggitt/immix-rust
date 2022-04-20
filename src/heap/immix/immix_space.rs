use crate::common::AddressMap;
use crate::common::{Address, TraceMap};
use crate::heap::gc;
use crate::heap::immix;

use crate::heap::immix::line_mark::LineMark;
use crate::heap::immix::line_mark::{LineMarkTable, LineMarkTableSlice};
use crate::heap::immix::BlockMark;
use crossbeam::deque::{Injector, Steal};
use memmap2::{MmapMut, MmapOptions};
use std::collections::VecDeque;
use std::*;

#[repr(C)]
pub struct ImmixSpace {
    start: Address,
    end: Address,

    // TODO: Make SafeAddressMap<u8> atomic (SafeAddressMap<AtomicU8>)
    // these maps are writable at allocation, read-only at collection
    pub alloc_map: AddressMap<u8>,

    // these maps are only for collection
    pub trace_map: TraceMap,

    // this table will be accessed through unsafe raw pointers. since Rust doesn't provide a data structure for such guarantees:
    // 1. Non-overlapping segments of this table may be accessed concurrently from different mutator threads
    // 2. One element may be written into at the same time by different gc threads during tracing
    pub line_mark_table: LineMarkTable,

    total_blocks: usize, // for debug use

    mmap: MmapMut,
    usable_blocks: Injector<Box<ImmixBlock>>,
    used_blocks: Injector<Box<ImmixBlock>>,
}

const SPACE_ALIGN: usize = 1 << 19;

impl ImmixSpace {
    pub fn new(space_size: usize) -> ImmixSpace {
        // Acquire memory through mmap
        let mut anon_mmap = MmapOptions::new()
            .len(space_size + SPACE_ALIGN)
            .map_anon()
            .expect("failed to call mmap");
        let start: Address = Address::from_ptr::<u8>(anon_mmap.as_mut_ptr()).align_up(SPACE_ALIGN);
        let end: Address = start.plus(space_size);

        let line_mark_table = LineMarkTable::new(start, end);

        let mut ret = ImmixSpace {
            start,
            end,
            mmap: anon_mmap,

            line_mark_table,
            trace_map: TraceMap::new(start.as_usize(), end.as_usize()),
            alloc_map: AddressMap::new(start, end),
            usable_blocks: Injector::new(),
            used_blocks: Injector::new(),
            total_blocks: 0,
        };

        ret.init_blocks();

        ret
    }

    fn init_blocks(&mut self) {
        let mut id = 0;
        let mut block_start = self.start;
        let mut line = 0;

        while block_start.plus(immix::BYTES_IN_BLOCK) <= self.end {
            self.usable_blocks.push(Box::new(ImmixBlock {
                id,
                state: immix::BlockMark::Usable,
                start: block_start,
                line_mark_table: self.line_mark_table.take_slice(line, immix::LINES_IN_BLOCK),
            }));

            id += 1;
            block_start = block_start.plus(immix::BYTES_IN_BLOCK);
            line += immix::LINES_IN_BLOCK;
        }

        self.total_blocks = id;
    }

    pub fn return_used_block(&self, old: Box<ImmixBlock>) {
        // Unsafe and raw pointers are used to transfer ImmixBlock to/from each Mutator.
        // This avoids explicit ownership transferring
        // If we explicitly transfer ownership, the function needs to own the Mutator in order to move the ImmixBlock out of it (see ImmixMutatorLocal.alloc_from_global()),
        // and this will result in passing the Mutator object as value (instead of a borrowed reference) all the way in the allocation
        self.used_blocks.push(old);
    }

    pub fn get_next_usable_block(&self) -> Option<Box<ImmixBlock>> {
        loop {
            match self.usable_blocks.steal() {
                Steal::Empty => {
                    gc::trigger_gc();
                    return None;
                }
                Steal::Success(v) => return Some(v),
                Steal::Retry => {}
            }
        }
    }

    pub fn sweep(&self) {
        let mut free_lines = 0;
        let mut usable_blocks = 0;
        let mut full_blocks = 0;

        // let mut used_blocks_lock = self.used_blocks.lock();
        // let mut usable_blocks_lock = self.usable_blocks.lock();

        let mut live_blocks: VecDeque<Box<ImmixBlock>> =
            VecDeque::with_capacity(self.used_blocks.len());

        loop {
            // let mut block = used_blocks_lock.pop_front().unwrap();
            let mut block = match self.used_blocks.steal() {
                Steal::Empty => break,
                Steal::Success(v) => v,
                Steal::Retry => continue,
            };

            let mut has_free_lines = false;

            {
                let cur_line_mark_table = block.line_mark_table_mut();
                for i in 0..cur_line_mark_table.len() {
                    if cur_line_mark_table.get(i) != LineMark::Live
                        && cur_line_mark_table.get(i) != LineMark::ConservLive
                    {
                        has_free_lines = true;
                        cur_line_mark_table.set(i, LineMark::Free);

                        free_lines += 1;
                    }
                }

                // release the mutable borrow of 'block'
            }

            if has_free_lines {
                block.set_state(BlockMark::Usable);
                usable_blocks += 1;

                // usable_blocks_lock.push_front(block);
                self.usable_blocks.push(block);
            } else {
                block.set_state(BlockMark::Full);
                full_blocks += 1;
                live_blocks.push_front(block);
            }
        }

        // used_blocks_lock.append(&mut live_blocks);
        for block in live_blocks {
            self.used_blocks.push(block);
        }

        if cfg!(debug_assertions) {
            println!(
                "free lines    = {} of {} total",
                free_lines,
                self.total_blocks * immix::LINES_IN_BLOCK
            );
            println!("usable blocks = {}", usable_blocks);
            println!("full blocks   = {}", full_blocks);
        }

        if full_blocks == self.total_blocks {
            panic!("Out of memory in Immix Space");
        }

        debug_assert!(full_blocks + usable_blocks == self.total_blocks);
    }

    pub fn start(&self) -> Address {
        self.start
    }
    pub fn end(&self) -> Address {
        self.end
    }

    pub fn line_mark_table(&self) -> &LineMarkTable {
        &self.line_mark_table
    }

    #[inline(always)]
    pub fn addr_in_space(&self, addr: Address) -> bool {
        addr >= self.start && addr < self.end
    }
}

pub struct ImmixBlock {
    id: usize,
    state: immix::BlockMark,
    start: Address,

    // a segment of the big line mark table in ImmixSpace
    line_mark_table: LineMarkTableSlice,
}

impl ImmixBlock {
    pub fn get_next_available_line(&self, cur_line: usize) -> Option<usize> {
        self.line_mark_table.get_next_available_line(cur_line)
    }

    pub fn get_next_unavailable_line(&self, cur_line: usize) -> usize {
        self.line_mark_table.get_next_unavailable_line(cur_line)
    }

    pub fn id(&self) -> usize {
        self.id
    }
    pub fn start(&self) -> Address {
        self.start
    }
    pub fn set_state(&mut self, mark: immix::BlockMark) {
        self.state = mark;
    }
    #[inline(always)]
    pub fn line_mark_table(&self) -> &LineMarkTableSlice {
        &self.line_mark_table
    }
    #[inline(always)]
    pub fn line_mark_table_mut(&mut self) -> &mut LineMarkTableSlice {
        &mut self.line_mark_table
    }
}

/// Using raw pointers forbid the struct being shared between threads
/// we ensure the raw pointers won't be an issue, so we allow Sync/Send on ImmixBlock
unsafe impl Sync for ImmixBlock {}
unsafe impl Send for ImmixBlock {}
unsafe impl Sync for ImmixSpace {}
unsafe impl Send for ImmixSpace {}

impl fmt::Display for ImmixSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "ImmixSpace")?;
        writeln!(f, "range={:#X} ~ {:#X}", self.start, self.end)?;

        // print table by vec
        //        write!(f, "table={{\n").unwrap();
        //        for i in 0..self.line_mark_table_len {
        //            write!(f, "({})", i).unwrap();
        //            write!(f, "{:?},", unsafe{*self.line_mark_table.offset(i as isize)}).unwrap();
        //            if i % immix::BYTES_IN_LINE == immix::BYTES_IN_LINE - 1 {
        //                write!(f, "\n").unwrap();
        //            }
        //        }
        //        write!(f, "\n}}\n").unwrap();

        writeln!(f, "t_ptr={:?}", &self.line_mark_table)?;
        //        write!(f, "usable blocks:\n").unwrap();
        //        for b in self.usable_blocks.iter() {
        //            write!(f, "  {}\n", b).unwrap();
        //        }
        //        write!(f, "used blocks:\n").unwrap();
        //        for b in self.used_blocks.iter() {
        //            write!(f, "  {}\n", b).unwrap();
        //        }
        writeln!(f, "done")
    }
}

impl fmt::Display for ImmixBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ImmixBlock#{}(state={:?}, address={:#X}, line_table={:?}",
            self.id, self.state, self.start, &self.line_mark_table
        )?;

        write!(f, "[")?;
        for i in 0..immix::LINES_IN_BLOCK {
            write!(f, "{:?},", self.line_mark_table.get(i))?;
        }
        write!(f, "]")
    }
}
