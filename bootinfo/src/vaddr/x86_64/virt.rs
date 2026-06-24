use bytemuck::Zeroable;

use super::assert_size_align;
use crate::{PAGE_SIZE, vaddr::PdptTable};

const _: () = assert_size_align::<Pml4TableVirt>();
//const _: () = assert_size_align::<PdptTableVirt>();
//const _: () = assert_size_align::<PdTableVirt>();
//const _: () = assert_size_align::<PtTableVirt>();

#[derive(Zeroable)]
#[repr(C, align(4096))]
pub struct Pml4TableVirt {
    pdpl_entries: [*mut PdptTable; PAGE_SIZE / size_of::<*mut PdptTable>()],
}
