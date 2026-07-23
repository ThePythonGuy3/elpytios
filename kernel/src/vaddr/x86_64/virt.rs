use core::fmt;

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

impl<T> From<*const T> for VAddr {
    #[inline]
    fn from(value: *const T) -> Self {
        Self(value as usize)
    }
}

impl<T> From<*mut T> for VAddr {
    #[inline]
    fn from(value: *mut T) -> Self {
        Self(value as usize)
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
#[repr(transparent)]
pub struct VirtualMapBuilder<T: Fn() -> Option<PAddr>> {
    map: VirtualMap<LocalMapper<T>>,
}

impl<T: Fn() -> Option<PAddr>> VirtualMapBuilder<T> {
    /// # Safety
    /// - `page_table_new` must return a [`PAGE_SIZE`]-aligned physical address that is completely
    ///   free to be written to (nothing else "owns" it).
    /// - `page_table_phys` must be one such pointer that satisfies to be a return value of
    ///   `page_table_new`
    /// - `page_table_ptr` must convert physical addresses returned by `new_page_table` into a
    ///   pointer that points to a page table.
    #[inline]
    pub const unsafe fn new(page_table_phys: PAddr, page_table_new: T, page_table_ptr: unsafe fn(PAddr) -> *mut ()) -> Self {
        Self {
            map: VirtualMap {
                mapper: LocalMapper {
                    page_table_phys,
                    page_table_new,
                    page_table_ptr,
                },
            },
        }
    }

    #[inline]
    pub fn map(&mut self, p_addr: PAddr, v_addr: VAddr, page_count: usize, flags: VFlags) -> Result<(), VirtualMapError> {
        unsafe { self.map.map(p_addr, v_addr, page_count, flags) }
    }

    /// # Safety
    /// Direct-map offset must have been set and usable by the time *any* methods in the returned
    /// [`VirtualMap`] is called.
    pub unsafe fn finish(self) -> VirtualMap {
        VirtualMap {
            mapper: unsafe { sealed::OffsetMapper::new(phys_to_virt(self.map.mapper.page_table_phys).ptr_mut()) },
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct LocalMapper<T: Fn() -> Option<PAddr>> {
    page_table_phys: PAddr,
    page_table_new: T,
    page_table_ptr: unsafe fn(PAddr) -> *mut (),
}

unsafe impl<T: Fn() -> Option<PAddr>> sealed::VirtualMapper for LocalMapper<T> {
    #[inline]
    fn new_page_table(&self) -> Option<PAddr> {
        (self.page_table_new)().inspect(|&addr| unsafe { (self.page_table_ptr)(addr).cast::<u8>().write_bytes(0, PAGE_SIZE) })
    }

    #[inline]
    fn pml4(&self) -> *mut Pml4Table {
        unsafe { (self.page_table_ptr)(self.page_table_phys).cast() }
    }

    #[inline]
    unsafe fn pdpt(&self, pml4_index: usize) -> *mut PdptTable {
        unsafe {
            (self.page_table_ptr)((*self.pml4()).pml4_to_pdpt[pml4_index].child_addr())
                .cast::<PdptTable>()
                .as_mut_unchecked()
        }
    }

    #[inline]
    unsafe fn pd(&self, pml4_index: usize, pdpt_index: usize) -> *mut PdTable {
        unsafe {
            (self.page_table_ptr)(match (*self.pdpt(pml4_index)).pdpt_to_pd[pdpt_index].kind() {
                UnionEntry::Node(e) => e.child_addr(),
                UnionEntry::Leaf(..) => unreachable!("PDPT entry is a huge page entry"),
            })
            .cast::<PdTable>()
            .as_mut_unchecked()
        }
    }

    #[inline]
    unsafe fn pt(&self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> *mut PtTable {
        unsafe {
            (self.page_table_ptr)(match (*self.pd(pml4_index, pdpt_index)).pd_to_pt[pd_index].kind() {
                UnionEntry::Node(e) => e.child_addr(),
                UnionEntry::Leaf(..) => unreachable!("PD entry is a huge page entry"),
            })
            .cast::<PtTable>()
            .as_mut_unchecked()
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
    /// There must never be concurrent (multithreaded) calls to this method that have the same
    /// virtual page occupied by `v_addr`.
    pub unsafe fn map(&self, mut p_addr: PAddr, mut v_addr: VAddr, mut page_count: usize, flags: VFlags) -> Result<(), VirtualMapError> {
        while page_count > 0 {
            let VAddrInfo {
                pt_index,
                pd_index,
                pdpt_index,
                pml4_index,
                ..
            } = v_addr.info();

            unsafe {
                match &raw mut (*self.mapper.pml4()).pml4_to_pdpt[pml4_index] {
                    e if !(*e).is_present() => e.write(NodeEntry::new(
                        Entry::WRITABLE,
                        self.mapper.new_page_table().ok_or(VirtualMapError::PageTable)?,
                    )),
                    _ => {}
                }

                match &raw mut (*self.mapper.pdpt(pml4_index)).pdpt_to_pd[pdpt_index] {
                    e if !(*e).is_present() => {
                        if pd_index == 0 && page_count >= 512 * 512 {
                            e.write(PdptEntry::leaf(PdptLeafEntry::new(flags.into(), p_addr) | flags.into()));
                            p_addr = p_addr.byte_add(512 * 512 * PAGE_SIZE);
                            v_addr = v_addr.byte_add(512 * 512 * PAGE_SIZE);
                            page_count -= 512 * 512;
                            continue
                        } else {
                            e.write(PdptEntry::node(NodeEntry::new(
                                Entry::WRITABLE,
                                self.mapper.new_page_table().ok_or(VirtualMapError::PageTable)?,
                            )))
                        }
                    }
                    e if let UnionEntry::Leaf(e) = (*e).kind() => {
                        return Err(VirtualMapError::AlreadyMapped {
                            p_addr,
                            v_addr,
                            p_addr_existing: e.addr(),
                        })
                    }
                    _ => {}
                }

                match &raw mut (*self.mapper.pd(pml4_index, pdpt_index)).pd_to_pt[pd_index] {
                    e if !(*e).is_present() => {
                        if pt_index == 0 && page_count >= 512 {
                            e.write(PdEntry::leaf(PdLeafEntry::new(flags.into(), p_addr) | flags.into()));
                            p_addr = p_addr.byte_add(512 * PAGE_SIZE);
                            v_addr = v_addr.byte_add(512 * PAGE_SIZE);
                            page_count -= 512;
                            continue
                        } else {
                            e.write(PdEntry::node(NodeEntry::new(
                                Entry::WRITABLE,
                                self.mapper.new_page_table().ok_or(VirtualMapError::PageTable)?,
                            )))
                        }
                    }
                    e if let UnionEntry::Leaf(e) = (*e).kind() => {
                        return Err(VirtualMapError::AlreadyMapped {
                            p_addr,
                            v_addr,
                            p_addr_existing: e.addr(),
                        })
                    }
                    _ => {}
                }

                match &raw mut (*self.mapper.pt(pml4_index, pdpt_index, pd_index)).phys_pages[pt_index] {
                    e if (*e).is_present() => {
                        return Err(VirtualMapError::AlreadyMapped {
                            p_addr,
                            v_addr,
                            p_addr_existing: (*e).addr(),
                        })
                    }
                    e => {
                        e.write(PtEntry::new(flags.into(), p_addr) | flags.into());
                        p_addr = p_addr.byte_add(PAGE_SIZE);
                        v_addr = v_addr.byte_add(PAGE_SIZE);
                        page_count -= 1;
                    }
                }
            }
        }

        Ok(())
    }
}

impl VirtualMap<sealed::OffsetMapper> {
    /// # Safety
    /// - [`map`](Self::map) must not be called for higher-half addresses on the returned mapper
    /// - [`map`](Self::map) must not be called for lower-half addresses on the `self` mapper.
    pub unsafe fn for_userspace(&self) -> (Self, PAddr) {
        use sealed::VirtualMapper;

        let pml4_phys = self
            .mapper
            .new_page_table()
            .expect("Couldn't allocate a new page for userspace virtual map");
        let pml4 = phys_to_virt(pml4_phys).ptr_mut::<Pml4Table>();

        unsafe {
            pml4.write(self.mapper.pml4().read());
            (
                Self {
                    mapper: sealed::OffsetMapper::new(pml4),
                },
                pml4_phys,
            )
        }
    }
}

mod sealed {
    use core::hint::unreachable_unchecked;

    use super::*;
    use crate::statics::get_phys_alloc;

    #[allow(unused_variables, reason = "Available for implementors, not defaults")]
    pub unsafe trait VirtualMapper {
        fn new_page_table(&self) -> Option<PAddr>;

        fn pml4(&self) -> *mut Pml4Table;

        /// # Safety
        /// - `pml4_index` must be within `0..512` (exclusive).
        unsafe fn pdpt(&self, pml4_index: usize) -> *mut PdptTable;

        /// # Safety:
        /// - [`Self::pdpt()`] to the given indices must return a node entry, not leaf.
        /// - `pml4_index` and `pdpt_index` must be within `0..512` (exclusive).
        unsafe fn pd(&self, pml4_index: usize, pdpt_index: usize) -> *mut PdTable;

        /// # Safety:
        /// - [`Self::pd()`] to the given indices must return a node entry, not leaf.
        /// - `pml4_index`, `pdpt_index`, and `pd_index` must be within `0..512` (exclusive).
        unsafe fn pt(&self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> *mut PtTable;
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
        fn new_page_table(&self) -> Option<PAddr> {
            get_phys_alloc()
                .lock()
                .alloc(0)
                .ok()
                .inspect(|&addr| unsafe { phys_to_virt(addr).ptr_mut::<u8>().write_bytes(0, PAGE_SIZE) })
        }

        #[inline]
        fn pml4(&self) -> *mut Pml4Table {
            self.pml4
        }

        #[inline]
        unsafe fn pdpt(&self, pml4_index: usize) -> *mut PdptTable {
            unsafe { phys_to_virt((*self.pml4()).pml4_to_pdpt.get_unchecked_mut(pml4_index).child_addr()).ptr_mut() }
        }

        #[inline]
        unsafe fn pd(&self, pml4_index: usize, pdpt_index: usize) -> *mut PdTable {
            unsafe {
                phys_to_virt(match (*self.pdpt(pml4_index)).pdpt_to_pd.get_unchecked_mut(pdpt_index).kind() {
                    UnionEntry::Leaf(..) => unreachable_unchecked(),
                    UnionEntry::Node(e) => e.child_addr(),
                })
                .ptr_mut()
            }
        }

        #[inline]
        unsafe fn pt(&self, pml4_index: usize, pdpt_index: usize, pd_index: usize) -> *mut PtTable {
            unsafe {
                phys_to_virt(match (*self.pd(pml4_index, pdpt_index)).pd_to_pt.get_unchecked_mut(pd_index).kind() {
                    UnionEntry::Leaf(..) => unreachable_unchecked(),
                    UnionEntry::Node(e) => e.child_addr(),
                })
                .ptr_mut()
            }
        }
    }
}
