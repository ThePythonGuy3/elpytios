use core::{fmt, hint::unreachable_unchecked};

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
        Self((addr.cast_signed() << 16 >> 16).cast_unsigned())
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
        write!(f, "{:#018p}", self.0 as *const ())
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct VirtualMapBuilder {
    map: VirtualMap<LocalMapper>,
    new_page_table: fn() -> Option<PAddr>,
}

impl VirtualMapBuilder {
    /// # Safety
    /// - `new_page_table` must return a [`PAGE_SIZE`](crate::PAGE_SIZE)-aligned physical address
    ///   that is:
    ///   - Completely zeroed out.
    ///   - Completely free to be written to (nothing else "owns" it).
    /// - `page_table_ptr` must convert physical addresses returned by `new_page_table` into a
    ///   pointer that points to a page table.
    #[inline]
    pub const unsafe fn new(recursion_index: usize, new_page_table: fn() -> Option<PAddr>, page_table_ptr: unsafe fn(PAddr) -> *mut ()) -> Self {
        Self {
            map: VirtualMap {
                mapper: LocalMapper {
                    table: bytemuck::zeroed(),
                    page_table_ptr,
                    recursion_index,
                },
            },
            new_page_table,
        }
    }

    #[inline]
    pub fn map(&mut self, p_addr: PAddr, v_addr: VAddr, flags: VFlags) -> Result<(), VirtualMapError> {
        unsafe { self.map.map(p_addr, v_addr, flags, self.new_page_table) }
    }

    pub fn finish(self) -> Result<(PAddr, VirtualMap), VirtualMapError> {
        let Self {
            map: VirtualMap { mut mapper },
            new_page_table,
        } = self;

        let pml4_phys = (new_page_table)().ok_or(VirtualMapError::PageTable)?;
        match mapper.table.pdpt_entries[mapper.recursion_index] {
            e if e.is_present() => unreachable!("`recursion_index` is checked in earlier methods"),
            ref mut e => *e = unsafe { NodeEntry::new(Entry::WRITABLE, pml4_phys) },
        }

        unsafe {
            (mapper.page_table_ptr)(pml4_phys).cast::<Pml4Table>().write(mapper.table);
        }

        Ok((pml4_phys, VirtualMap {
            mapper: unsafe { sealed::RecursiveMapper::new(mapper.recursion_index) },
        }))
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct LocalMapper {
    table: Pml4Table,
    page_table_ptr: unsafe fn(PAddr) -> *mut (),
    recursion_index: usize,
}

unsafe impl sealed::VirtualMapper for LocalMapper {
    #[inline]
    fn reserved(&self, v_addr: VAddr) -> bool {
        let VAddrInfo { pml4_index, .. } = v_addr.info();
        pml4_index == self.recursion_index
    }

    #[inline]
    fn pml4(&mut self) -> &mut Pml4Table {
        &mut self.table
    }

    #[inline]
    unsafe fn pdpt(&mut self, pml4_index: usize) -> &mut PdptTable {
        unsafe {
            (self.page_table_ptr)(self.table.pdpt_entries.get(pml4_index).unwrap_unchecked().child_addr())
                .cast::<PdptTable>()
                .as_mut_unchecked()
        }
    }

    #[inline]
    unsafe fn pd(&mut self, pml4_index: usize, pdpt_index: usize) -> &mut PdTable {
        unsafe {
            (self.page_table_ptr)(match self.pdpt(pml4_index).pd_entries.get(pdpt_index).unwrap_unchecked().kind() {
                UnionEntry::Node(e) => e.child_addr(),
                UnionEntry::Leaf(..) => unreachable_unchecked(),
            })
            .cast::<PdTable>()
            .as_mut_unchecked()
        }
    }

    #[inline]
    unsafe fn pt(&mut self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> &mut PtTable {
        unsafe {
            (self.page_table_ptr)(match self.pd(pml4_index, pdpt_index).pt_entries.get(pd_index).unwrap_unchecked().kind() {
                UnionEntry::Node(e) => e.child_addr(),
                UnionEntry::Leaf(..) => unreachable_unchecked(),
            })
            .cast::<PtTable>()
            .as_mut_unchecked()
        }
    }
}

#[derive(Debug)]
#[repr(transparent)]
// Note: From user-facing API perspective, `VirtualMap` must have no trait bounds.
pub struct VirtualMap<T: sealed::VirtualMapper = sealed::RecursiveMapper> {
    mapper: T,
}

impl<T: sealed::VirtualMapper> VirtualMap<T> {
    /// # Safety
    /// - `new_page_table` must return a [`PAGE_SIZE`](crate::PAGE_SIZE)-aligned physical address
    ///   that is:
    ///   - Completely zeroed out.
    ///   - Completely free to be written to (nothing else "owns" it).
    pub unsafe fn map(
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

mod sealed {
    use super::*;

    #[allow(unused_variables, reason = "Available for implementors, not defaults")]
    pub unsafe trait VirtualMapper {
        #[inline]
        fn reserved(&self, v_addr: VAddr) -> bool {
            false
        }

        fn pml4(&mut self) -> &mut Pml4Table;

        /// # Safety
        /// - [`pml4_index`] must be within `0..512` (exclusive).
        unsafe fn pdpt(&mut self, pml4_index: usize) -> &mut PdptTable;

        /// # Safety:
        /// - [`Self::pdpt()`] to the given indices must return a node entry, not leaf.
        /// - [`pml4_index`] and [`pdpt_index`] must be within `0..512` (exclusive).
        unsafe fn pd(&mut self, pml4_index: usize, pdpt_index: usize) -> &mut PdTable;

        /// # Safety:
        /// - [`Self::pd()`] to the given indices must return a node entry, not leaf.
        /// - [`pml4_index`], [`pdpt_index`], and [`pd_index`] must be within `0..512` (exclusive).
        unsafe fn pt(&mut self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> &mut PtTable;
    }

    #[derive(Debug)]
    #[repr(transparent)]
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
        unsafe fn pdpt(&mut self, pml4_index: usize) -> &mut PdptTable {
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
        unsafe fn pd(&mut self, pml4_index: usize, pdpt_index: usize) -> &mut PdTable {
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
        unsafe fn pt(&mut self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> &mut PtTable {
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
