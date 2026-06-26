use core::fmt;

use bytemuck::Zeroable;

use crate::{
    paddr::PAddr,
    vaddr::{Entry, NodeEntry, PdEntry, PdTable, PdptEntry, PdptTable, Pml4Table, PtEntry, PtTable, UnionEntry, VFlags, VirtualMapError},
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

    #[inline]
    pub(crate) const fn from_info(
        VAddrInfo {
            page_offset,
            pt_index,
            pd_index,
            pdpt_index,
            pml4_index,
        }: VAddrInfo,
    ) -> Self {
        let addr =
            page_offset & 0xfff | (pt_index & 0x1ff) << 12 | (pd_index & 0x1ff) << 21 | (pdpt_index & 0x1ff) << 30 | (pml4_index & 0x1ff) << 39;
        Self(((addr as isize) << 16 >> 16) as usize)
    }
}

#[derive(Debug, Clone, Copy, Zeroable)]
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
        write!(f, "{:?}", self.info())
    }
}

impl fmt::Display for VAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", self.0 as *const ())
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct VirtualMapBuilder<T: VirtualMapper2> {
    table: Pml4Table,
    recursion_index: usize,
    mapper: T,
}

impl<T: VirtualMapper2> VirtualMapBuilder<T> {
    #[inline]
    pub const fn new(mapper: T, recursion_index: usize) -> Self {
        Self {
            table: bytemuck::zeroed(),
            recursion_index,
            mapper,
        }
    }
}

impl<T: VirtualMapper2> VirtualMapBuilder<T> {
    pub fn map(&mut self, p_addr: PAddr, v_addr: VAddr, flags: VFlags) -> Result<(), VirtualMapError> {
        let VAddrInfo {
            pt_index,
            pd_index,
            pdpt_index,
            pml4_index,
            ..
        } = v_addr.info();

        if pml4_index == self.recursion_index {
            return Err(VirtualMapError::Reserved { p_addr, v_addr })
        }

        unsafe {
            let pdpt = self.mapper.page_table_ptr(
                match &mut self.table.pdpt_entries[pml4_index] {
                    e if e.is_present() => e,
                    e => {
                        *e = NodeEntry::new(Entry::WRITABLE, self.mapper.new_page_table().ok_or(VirtualMapError::PageTable)?);
                        e
                    }
                }
                .child_addr(),
            ) as *mut PdptTable;
            let pd = self.mapper.page_table_ptr(
                match &mut (*pdpt).pd_entries[pdpt_index] {
                    e if e.is_present()
                        && let UnionEntry::Node(e) = e.kind() =>
                    {
                        e
                    }
                    e => {
                        *e = PdptEntry::node(NodeEntry::new(
                            Entry::WRITABLE,
                            self.mapper.new_page_table().ok_or(VirtualMapError::PageTable)?,
                        ));
                        e.kind().force_node()
                    }
                }
                .child_addr(),
            ) as *mut PdTable;
            let pt = self.mapper.page_table_ptr(
                match &mut (*pd).pt_entries[pd_index] {
                    e if e.is_present()
                        && let UnionEntry::Node(e) = e.kind() =>
                    {
                        e
                    }
                    e => {
                        *e = PdEntry::node(NodeEntry::new(
                            Entry::WRITABLE,
                            self.mapper.new_page_table().ok_or(VirtualMapError::PageTable)?,
                        ));
                        e.kind().force_node()
                    }
                }
                .child_addr(),
            ) as *mut PtTable;
            match &mut (*pt).phys_pages[pt_index] {
                e if e.is_present() => {
                    return Err(VirtualMapError::AlreadyMapped {
                        p_addr,
                        v_addr,
                        p_addr_existing: e.addr(),
                    })
                }
                e => *e = PtEntry::new(flags.into(), p_addr) | if flags.contains(VFlags::GLOBAL) { PtEntry::GLOBAL } else { PtEntry::empty() },
            }
        }

        Ok(())
    }

    pub fn finish(mut self) -> Result<(PAddr, VirtualMap), VirtualMapError> {
        // Add a recursion entry to the PML4 table
        let pml4_phys = self.mapper.new_page_table().ok_or(VirtualMapError::PageTable)?;
        match self.table.pdpt_entries[self.recursion_index] {
            e if e.is_present() => unreachable!("`recursion_index` is checked in app `map_*` methods"),
            ref mut e => *e = unsafe { NodeEntry::new(Entry::WRITABLE, pml4_phys) },
        }

        unsafe {
            self.mapper.page_table_ptr(pml4_phys).cast::<Pml4Table>().write(self.table);
        }

        Ok((pml4_phys, VirtualMap {
            mapper: unsafe { sealed::RecursiveMapper::new(self.recursion_index) },
        }))
    }
}

/// # Safety
/// - [`Self::new_page_table`] must return a [`PAGE_SIZE`](crate::PAGE_SIZE)-aligned physical
///   address that is completely free to be written to (nothing else "owns" it).
pub unsafe trait VirtualMapper2 {
    fn new_page_table(&self) -> Option<PAddr>;

    /// # Safety
    /// - `p_addr` must have been obtained with [`Self::new_page_table`].
    unsafe fn page_table_ptr(&self, p_addr: PAddr) -> *mut ();
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
// Note: From user-facing API perspective, `VirtualMap` must have no trait bounds.
pub struct VirtualMap<T: sealed::VirtualMapper = sealed::RecursiveMapper> {
    mapper: T,
}

mod sealed {
    use super::*;

    pub unsafe trait VirtualMapper {
        #[inline]
        fn reserved(&self, #[allow(unused_variables, reason = "Available for implementors, not defaults")] v_addr: VAddr) -> bool {
            false
        }

        fn pml4(&mut self) -> &mut Pml4Table;

        fn pdpt(&mut self, pml4_index: usize) -> &mut PdptTable;

        fn pd(&mut self, pml4_index: usize, pdpt_index: usize) -> &mut PdTable;

        fn pt(&mut self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> &mut PtTable;
    }

    #[derive(Debug, Clone, Copy)]
    pub struct RecursiveMapper {
        recursion_index: usize,
    }

    impl RecursiveMapper {
        /// # Safety:
        /// `recursion_index` must be N where
        /// [`pdpt_entries[N]`](crate::vaddr::Pml4Table::pdpt_entries) points to the
        /// physical address of the PML4 table itself (i.e. recursive slot).
        #[inline]
        pub const unsafe fn new(recursion_index: usize) -> Self {
            Self { recursion_index }
        }
    }

    unsafe impl VirtualMapper for RecursiveMapper {
        #[inline]
        fn reserved(&self, v_addr: VAddr) -> bool {
            let VAddrInfo { pml4_index, .. } = v_addr.info();
            pml4_index == self.recursion_index
        }

        #[inline]
        fn pml4(&mut self) -> &mut Pml4Table {
            unsafe {
                VAddr::from_info(VAddrInfo {
                    page_offset: 0, // Keep recursing so it ends up with the PML4 table itself
                    pt_index: self.recursion_index,
                    pd_index: self.recursion_index,
                    pdpt_index: self.recursion_index,
                    pml4_index: self.recursion_index,
                })
                .ptr_mut::<Pml4Table>()
                .as_mut_unchecked()
            }
        }

        #[inline]
        fn pdpt(&mut self, pml4_index: usize) -> &mut PdptTable {
            unsafe {
                VAddr::from_info(VAddrInfo {
                    page_offset: 0, // Stop recursing at PT index so it ends up with the PDPT entry
                    pt_index: pml4_index,
                    pd_index: self.recursion_index,
                    pdpt_index: self.recursion_index,
                    pml4_index: self.recursion_index,
                })
                .ptr_mut::<PdptTable>()
                .as_mut_unchecked()
            }
        }

        #[inline]
        fn pd(&mut self, pml4_index: usize, pdpt_index: usize) -> &mut PdTable {
            unsafe {
                VAddr::from_info(VAddrInfo {
                    page_offset: 0, // Stop recursing at PD index so it ends up with the PD entry
                    pt_index: pdpt_index,
                    pd_index: pml4_index,
                    pdpt_index: self.recursion_index,
                    pml4_index: self.recursion_index,
                })
                .ptr_mut::<PdTable>()
                .as_mut_unchecked()
            }
        }

        #[inline]
        fn pt(&mut self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> &mut PtTable {
            unsafe {
                VAddr::from_info(VAddrInfo {
                    page_offset: 0, // Stop recursing at PDPT index so it ends up with the PT entry
                    pt_index: pd_index,
                    pd_index: pdpt_index,
                    pdpt_index: pml4_index,
                    pml4_index: self.recursion_index,
                })
                .ptr_mut::<PtTable>()
                .as_mut_unchecked()
            }
        }
    }
}

impl<T: sealed::VirtualMapper> VirtualMap<T> {
    pub fn map(
        &mut self,
        p_addr: PAddr,
        v_addr: VAddr,
        flags: VFlags,
        mut new_page_table: impl FnMut() -> Option<PAddr>,
    ) -> Result<(), VirtualMapError> {
        if self.mapper.reserved(v_addr) {
            return Err(VirtualMapError::Reserved { p_addr, v_addr })
        }

        let VAddrInfo {
            pt_index,
            pd_index,
            pdpt_index,
            pml4_index,
            ..
        } = v_addr.info();
        unsafe {
            match &mut self.mapper.pml4().pdpt_entries[pml4_index] {
                e if !e.is_present() => *e = NodeEntry::new(Entry::WRITABLE, new_page_table().ok_or(VirtualMapError::PageTable)?),
                _ => {}
            }

            match &mut self.mapper.pdpt(pml4_index).pd_entries[pdpt_index] {
                e if !e.is_present() => *e = PdptEntry::node(NodeEntry::new(Entry::WRITABLE, new_page_table().ok_or(VirtualMapError::PageTable)?)),
                e if let UnionEntry::Leaf(e) = e.kind() => {
                    return Err(VirtualMapError::AlreadyMapped {
                        p_addr,
                        v_addr,
                        p_addr_existing: e.addr(),
                    })
                }
                _ => {}
            }

            match &mut self.mapper.pd(pml4_index, pdpt_index).pt_entries[pd_index] {
                e if !e.is_present() => *e = PdEntry::node(NodeEntry::new(Entry::WRITABLE, new_page_table().ok_or(VirtualMapError::PageTable)?)),
                e if let UnionEntry::Leaf(e) = e.kind() => {
                    return Err(VirtualMapError::AlreadyMapped {
                        p_addr,
                        v_addr,
                        p_addr_existing: e.addr(),
                    })
                }
                _ => {}
            }

            match &mut self.mapper.pt(pml4_index, pdpt_index, pd_index).phys_pages[pt_index] {
                e if e.is_present() => {
                    return Err(VirtualMapError::AlreadyMapped {
                        p_addr,
                        v_addr,
                        p_addr_existing: e.addr(),
                    })
                }
                e => *e = PtEntry::new(flags.into(), p_addr) | if flags.contains(VFlags::GLOBAL) { PtEntry::GLOBAL } else { PtEntry::empty() },
            }
        }

        Ok(())
    }
}
