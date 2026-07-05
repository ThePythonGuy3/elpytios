use core::{cell::RefCell, fmt, ops::DerefMut};

use bytemuck::Zeroable;
use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};

use crate::{
    statics::phys_to_virt,
    vaddr::{
        Entry, NodeEntry, PdEntry, PdLeafEntry, PdTable, PdptEntry, PdptLeafEntry, PdptTable, Pml4Table, PtEntry, PtTable, UnionEntry, VFlags,
        VirtualMapError,
    },
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
    pub const fn byte_add(self, offset: usize) -> Self {
        Self(self.0 + offset)
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

impl fmt::Pointer for VAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#018p}", self.0 as *const ())
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct VirtualMapBuilder<T: FnMut() -> Option<PAddr>> {
    map: VirtualMap<LocalMapper>,
    new_page_table: T,
}

impl<T: FnMut() -> Option<PAddr>> VirtualMapBuilder<T> {
    /// # Safety
    /// - `new_page_table` must return a [`PAGE_SIZE`](crate::PAGE_SIZE)-aligned physical address
    ///   that is:
    ///   - Completely zeroed out.
    ///   - Completely free to be written to (nothing else "owns" it).
    /// - `page_table_ptr` must convert physical addresses returned by `new_page_table` into a
    ///   pointer that points to a page table.
    #[inline]
    pub const unsafe fn new(new_page_table: T, page_table_ptr: unsafe fn(PAddr) -> *mut ()) -> Self {
        Self {
            map: VirtualMap {
                mapper: LocalMapper {
                    table: RefCell::new(bytemuck::zeroed()),
                    page_table_ptr,
                },
            },
            new_page_table,
        }
    }

    #[inline]
    pub fn map(&mut self, p_addr: PAddr, v_addr: VAddr, page_count: usize, flags: VFlags) -> Result<(), VirtualMapError> {
        unsafe { self.map.map(p_addr, v_addr, page_count, flags, &mut self.new_page_table) }
    }

    /// # Safety
    /// Direct-map offset must have been set.
    pub unsafe fn finish(self) -> Result<(PAddr, VirtualMap), VirtualMapError> {
        let Self {
            map: VirtualMap { mapper },
            mut new_page_table,
        } = self;

        let pml4_phys = (new_page_table)().ok_or(VirtualMapError::PageTable)?;
        unsafe {
            (mapper.page_table_ptr)(pml4_phys).cast::<Pml4Table>().write(mapper.table.into_inner());
        }

        Ok((pml4_phys, VirtualMap {
            mapper: unsafe { sealed::OffsetMapper::new(phys_to_virt(pml4_phys).ptr_mut()) },
        }))
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct LocalMapper {
    table: RefCell<Pml4Table>,
    page_table_ptr: unsafe fn(PAddr) -> *mut (),
}

unsafe impl sealed::VirtualMapper for LocalMapper {
    #[inline]
    fn pml4(&self) -> impl DerefMut<Target = Pml4Table> {
        self.table.borrow_mut()
    }

    #[inline]
    unsafe fn pdpt(&self, pml4_index: usize) -> &mut PdptTable {
        unsafe {
            (self.page_table_ptr)(self.pml4().pml4_to_pdpt[pml4_index].child_addr())
                .cast::<PdptTable>()
                .as_mut()
                .expect("Null pointer on PML4 entry")
        }
    }

    #[inline]
    unsafe fn pd(&self, pml4_index: usize, pdpt_index: usize) -> &mut PdTable {
        unsafe {
            (self.page_table_ptr)(match self.pdpt(pml4_index).pdpt_to_pd[pdpt_index].kind() {
                UnionEntry::Node(e) => e.child_addr(),
                UnionEntry::Leaf(..) => unreachable!("PDPT entry is a huge page entry"),
            })
            .cast::<PdTable>()
            .as_mut()
            .expect("Null pointer on PDPT entry")
        }
    }

    #[inline]
    unsafe fn pt(&self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> &mut PtTable {
        unsafe {
            (self.page_table_ptr)(match self.pd(pml4_index, pdpt_index).pd_to_pt[pd_index].kind() {
                UnionEntry::Node(e) => e.child_addr(),
                UnionEntry::Leaf(..) => unreachable!("PD entry is a huge page entry"),
            })
            .cast::<PtTable>()
            .as_mut()
            .expect("Null pointer on PD entry")
        }
    }
}

#[derive(Debug)]
#[repr(transparent)]
// Note: From user-facing API perspective, `VirtualMap` must have no trait bounds.
pub struct VirtualMap<T: sealed::VirtualMapper = sealed::OffsetMapper> {
    mapper: T,
}

impl<T: sealed::VirtualMapper> VirtualMap<T> {
    /// # Safety
    /// - `new_page_table` must return a [`PAGE_SIZE`](crate::PAGE_SIZE)-aligned physical address
    ///   that is:
    ///   - Completely zeroed out.
    ///   - Completely free to be written to (nothing else "owns" it).
    /// - There must never be concurrent (multithreaded) calls to this method that have the same
    ///   virtual page occupied by `v_addr`.
    pub unsafe fn map(
        &self,
        mut p_addr: PAddr,
        mut v_addr: VAddr,
        mut page_count: usize,
        flags: VFlags,
        mut new_page_table: impl FnMut() -> Option<PAddr>,
    ) -> Result<(), VirtualMapError> {
        while page_count > 0 {
            let VAddrInfo {
                pt_index,
                pd_index,
                pdpt_index,
                pml4_index,
                ..
            } = v_addr.info();

            unsafe {
                match &mut self.mapper.pml4().pml4_to_pdpt[pml4_index] {
                    e if !e.is_present() => *e = NodeEntry::new(Entry::WRITABLE, new_page_table().ok_or(VirtualMapError::PageTable)?),
                    _ => {}
                }

                match &mut self.mapper.pdpt(pml4_index).pdpt_to_pd[pdpt_index] {
                    e if !e.is_present() => {
                        if pd_index == 0 && page_count >= 512 * 512 {
                            *e = PdptEntry::leaf(PdptLeafEntry::new(flags.into(), p_addr) | flags.into());
                            p_addr = p_addr.byte_add(512 * 512 * PAGE_SIZE);
                            v_addr = v_addr.byte_add(512 * 512 * PAGE_SIZE);
                            page_count -= 512 * 512;
                            continue
                        } else {
                            *e = PdptEntry::node(NodeEntry::new(Entry::WRITABLE, new_page_table().ok_or(VirtualMapError::PageTable)?))
                        }
                    }
                    e if let UnionEntry::Leaf(e) = e.kind() => {
                        return Err(VirtualMapError::AlreadyMapped {
                            p_addr,
                            v_addr,
                            p_addr_existing: e.addr(),
                        })
                    }
                    _ => {}
                }

                match &mut self.mapper.pd(pml4_index, pdpt_index).pd_to_pt[pd_index] {
                    e if !e.is_present() => {
                        if pt_index == 0 && page_count >= 512 {
                            *e = PdEntry::leaf(PdLeafEntry::new(flags.into(), p_addr) | flags.into());
                            p_addr = p_addr.byte_add(512 * PAGE_SIZE);
                            v_addr = v_addr.byte_add(512 * PAGE_SIZE);
                            page_count -= 512;
                            continue
                        } else {
                            *e = PdEntry::node(NodeEntry::new(Entry::WRITABLE, new_page_table().ok_or(VirtualMapError::PageTable)?))
                        }
                    }
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
                    e => {
                        *e = PtEntry::new(flags.into(), p_addr) | flags.into();
                        p_addr = p_addr.byte_add(PAGE_SIZE);
                        v_addr = v_addr.byte_add(PAGE_SIZE);
                        page_count -= 1;
                        continue
                    }
                }
            }
        }

        Ok(())
    }
}

mod sealed {
    use core::hint::unreachable_unchecked;

    use super::*;

    #[allow(unused_variables, reason = "Available for implementors, not defaults")]
    pub unsafe trait VirtualMapper {
        fn pml4(&self) -> impl DerefMut<Target = Pml4Table>;

        /// # Safety
        /// - [`pml4_index`] must be within `0..512` (exclusive).
        unsafe fn pdpt(&self, pml4_index: usize) -> &mut PdptTable;

        /// # Safety:
        /// - [`Self::pdpt()`] to the given indices must return a node entry, not leaf.
        /// - [`pml4_index`] and [`pdpt_index`] must be within `0..512` (exclusive).
        unsafe fn pd(&self, pml4_index: usize, pdpt_index: usize) -> &mut PdTable;

        /// # Safety:
        /// - [`Self::pd()`] to the given indices must return a node entry, not leaf.
        /// - [`pml4_index`], [`pdpt_index`], and [`pd_index`] must be within `0..512` (exclusive).
        unsafe fn pt(&self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> &mut PtTable;
    }

    #[derive(Debug)]
    #[repr(transparent)]
    pub struct OffsetMapper {
        pml4: *mut Pml4Table,
    }

    impl OffsetMapper {
        #[inline]
        pub const unsafe fn new(pml4: *mut Pml4Table) -> Self {
            Self { pml4 }
        }
    }

    unsafe impl VirtualMapper for OffsetMapper {
        #[inline]
        fn pml4(&self) -> impl DerefMut<Target = Pml4Table> {
            unsafe { self.pml4.as_mut_unchecked() }
        }

        #[inline]
        unsafe fn pdpt(&self, pml4_index: usize) -> &mut PdptTable {
            let mut pml4 = self.pml4();
            unsafe {
                phys_to_virt(pml4.pml4_to_pdpt.get_unchecked_mut(pml4_index).child_addr())
                    .ptr_mut::<PdptTable>()
                    .as_mut_unchecked()
            }
        }

        #[inline]
        unsafe fn pd(&self, pml4_index: usize, pdpt_index: usize) -> &mut PdTable {
            unsafe {
                phys_to_virt(match self.pdpt(pml4_index).pdpt_to_pd.get_unchecked_mut(pdpt_index).kind() {
                    UnionEntry::Leaf(..) => unreachable_unchecked(),
                    UnionEntry::Node(e) => e.child_addr(),
                })
                .ptr_mut::<PdTable>()
                .as_mut_unchecked()
            }
        }

        #[inline]
        unsafe fn pt(&self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> &mut PtTable {
            unsafe {
                phys_to_virt(match self.pd(pml4_index, pdpt_index).pd_to_pt.get_unchecked_mut(pd_index).kind() {
                    UnionEntry::Leaf(..) => unreachable_unchecked(),
                    UnionEntry::Node(e) => e.child_addr(),
                })
                .ptr_mut::<PtTable>()
                .as_mut_unchecked()
            }
        }
    }
}
