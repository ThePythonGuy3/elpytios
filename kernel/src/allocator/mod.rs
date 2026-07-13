use core::mem::Alignment;

use elpytios_bootinfo::PAGE_SIZE;

mod heap;
mod phys;
mod tree;
pub use heap::*;
pub use phys::*;
pub use tree::*;

/// Physical allocators are aligned to 32 pages (128 KiB).
/// This means pointers of allocations up to 32 pages are guaranteed to be aligned.
pub const PHYS_ALLOC_ALIGNMENT: Alignment = unsafe { Alignment::new_unchecked(1 << (32 * PAGE_SIZE).ilog2()) };
