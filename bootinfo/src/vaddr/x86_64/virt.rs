use core::fmt;

use bytemuck::Zeroable;
use derive_more::Display;

use crate::{
    paddr::PAddr,
    vaddr::{Entry, NodeEntry, PdEntry, PdTable, PdptEntry, PdptTable, Pml4Table, PtEntry, PtTable, UnionEntry, VFlags},
};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Zeroable)]
#[repr(transparent)]
pub struct VAddr(usize);
impl VAddr {
    #[inline]
    pub const fn new(virtual_address: usize) -> Self {
        Self(virtual_address)
    }

    #[inline]
    pub const fn addr(self) -> usize {
        self.0
    }

    #[inline]
    pub const fn ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    #[inline]
    pub const fn ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
    }

    #[inline]
    pub(crate) const fn info(self) -> VAddrInfo {
        VAddrInfo {
            page_offset: self.0 & 0xfff,
            pt_index: (self.0 >> 12) & 0x1ff,
            pd_index: (self.0 >> 21) & 0x1ff,
            pdpt_index: (self.0 >> 30) & 0x1ff,
            pml4_index: (self.0 >> 39) & 0x1ff,
        }
    }
}

#[derive(Clone, Copy, Zeroable)]
#[repr(C)]
pub(crate) struct VAddrInfo {
    pub page_offset: usize,
    pub pt_index: usize,
    pub pd_index: usize,
    pub pdpt_index: usize,
    pub pml4_index: usize,
}

impl fmt::Debug for VAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for VAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", self.0 as *const ())
    }
}

#[derive(Debug, Display, Clone, Copy)]
#[repr(C)]
pub enum VirtualMapBuildError {
    #[display("Couldn't allocate a page table")]
    PageTable,
    #[display("Couldn't map {p_addr} to {v_addr_reserved} because it is reserved")]
    Reserved { p_addr: PAddr, v_addr_reserved: VAddr },
}

#[derive(Debug)]
#[repr(C)]
pub struct VirtualMapBuilder<T: VirtualMapper> {
    table: Pml4Table,
    recursion_index: usize,
    mapper: T,
}

impl<T: VirtualMapper> VirtualMapBuilder<T> {
    #[inline]
    pub const fn new(mapper: T, recursion_index: usize) -> Self {
        Self {
            table: bytemuck::zeroed(),
            recursion_index,
            mapper,
        }
    }
}

impl<T: VirtualMapper> VirtualMapBuilder<T> {
    pub fn map(&mut self, p_addr: PAddr, v_addr: VAddr, flags: VFlags) -> Result<(), VirtualMapBuildError> {
        let VAddrInfo {
            pt_index,
            pd_index,
            pdpt_index,
            pml4_index,
            ..
        } = v_addr.info();

        if pml4_index == self.recursion_index {
            return Err(VirtualMapBuildError::Reserved {
                p_addr,
                v_addr_reserved: v_addr,
            })
        }

        unsafe {
            let pdpt = self.mapper.page_table_ptr(
                match self.table.pdpt_entries[pml4_index] {
                    e if e.is_present() => e,
                    ref mut e => {
                        *e = NodeEntry::new(Entry::WRITABLE, self.mapper.new_page_table().ok_or(VirtualMapBuildError::PageTable)?);
                        *e
                    }
                }
                .child_addr(),
            ) as *mut PdptTable;
            let pd = self.mapper.page_table_ptr(
                match (*pdpt).pd_entries[pdpt_index] {
                    e if e.is_present()
                        && let UnionEntry::Node(e) = e.kind() =>
                    {
                        e
                    }
                    ref mut e => {
                        *e = PdptEntry::node(NodeEntry::new(
                            Entry::WRITABLE,
                            self.mapper.new_page_table().ok_or(VirtualMapBuildError::PageTable)?,
                        ));
                        e.kind().force_node()
                    }
                }
                .child_addr(),
            ) as *mut PdTable;
            let pt = self.mapper.page_table_ptr(
                match (*pd).pt_entries[pd_index] {
                    e if e.is_present()
                        && let UnionEntry::Node(e) = e.kind() =>
                    {
                        e
                    }
                    ref mut e => {
                        *e = PdEntry::node(NodeEntry::new(
                            Entry::WRITABLE,
                            self.mapper.new_page_table().ok_or(VirtualMapBuildError::PageTable)?,
                        ));
                        e.kind().force_node()
                    }
                }
                .child_addr(),
            ) as *mut PtTable;
            match (*pt).phys_pages[pt_index] {
                e if e.is_present() => panic!("Couldn't map {v_addr} to {p_addr}; already mapped to {}", e.addr()),
                ref mut e => *e = PtEntry::new(flags.into(), p_addr) | PtEntry::GLOBAL,
            }
        }

        Ok(())
    }

    pub fn finish(mut self) -> Result<(PAddr, VirtualMap), VirtualMapBuildError> {
        // Add a recursion entry to the PML4 table
        let pml4_phys = self.mapper.new_page_table().ok_or(VirtualMapBuildError::PageTable)?;
        match self.table.pdpt_entries[self.recursion_index] {
            e if e.is_present() => unreachable!("`recursion_index` is checked in app `map_*` methods"),
            ref mut e => *e = unsafe { NodeEntry::new(Entry::WRITABLE, pml4_phys) },
        }

        unsafe {
            self.mapper.page_table_ptr(pml4_phys).cast::<Pml4Table>().write(self.table);
        }

        Ok((pml4_phys, VirtualMap {
            recursion_index: self.recursion_index,
        }))
    }
}

/// # Safety
/// - [`Self::new_page_table`] must return a [`PAGE_SIZE`](crate::PAGE_SIZE)-aligned physical
///   address that is completely free to be written to (nothing else "owns" it).
pub unsafe trait VirtualMapper {
    fn new_page_table(&self) -> Option<PAddr>;

    /// # Safety
    /// - `p_addr` must have been obtained with [`Self::new_page_table`].
    unsafe fn page_table_ptr(&self, p_addr: PAddr) -> *mut ();
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VirtualMap {
    /// # Safety:
    /// `recursion_index` must be N where [`pdpt_entries[N]`](crate::vaddr::Pml4Table::pdpt_entries)
    /// points to the physical address of the PML4 table itself (i.e. recursive slot).
    recursion_index: usize,
}
