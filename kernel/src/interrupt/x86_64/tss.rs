use core::{
    cell::UnsafeCell,
    mem::{self, offset_of},
};

use super::GdtEntry;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptStack {
    /// Use [`Tss::rsp0`].
    None = 0,
    /// Use [`Tss::ist`]`[0]`.
    Task = 1,
}

#[repr(C, packed)]
pub struct Tss {
    reserved0: u32,
    /// Switch to this stack only when intercepting an interrupt from Ring 3. Set this to
    /// [`Task`](crate::task::Task)'s general-purpose registers address.
    pub rsp0: UnsafeCell<u64>,
    pub rsp1: UnsafeCell<u64>,
    pub rsp2: UnsafeCell<u64>,
    reserved1: u64,
    ist: [UnsafeCell<u64>; 7],
    reserved2: u64,
    reserved3: u16,
    iopb_offset: u16,
}

impl Tss {
    #[inline]
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp0: UnsafeCell::new(0),
            rsp1: UnsafeCell::new(0),
            rsp2: UnsafeCell::new(0),
            reserved1: 0,
            ist: [const { UnsafeCell::new(0) }; 7],
            reserved2: 0,
            reserved3: 0,
            iopb_offset: 0xffff,
        }
    }

    #[inline]
    pub const fn set_stack(&self, index: InterruptStack, addr: u64) {
        unsafe { UnsafeCell::raw_get(&raw const self.ist[Self::ist_addr(index)]).write_unaligned(addr) }
    }

    #[inline]
    pub const fn ist_addr(index: InterruptStack) -> usize {
        match (index as usize).checked_sub(1) {
            None => panic!("Can't use `InterruptStack::None` for IST!"),
            Some(i) => offset_of!(Self, ist) + i * size_of::<u64>(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct TssEntry {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    type_flags: u8,
    limit_high_flags: u8,
    base_high: u8,
    base_upper: u32,
    reserved: u32,
}

impl TssEntry {
    #[inline]
    pub fn new(tss: &'static Tss) -> Self {
        let addr = tss as *const Tss as u64;
        let limit = (size_of::<Tss>() - 1) as u64;

        Self {
            limit_low: limit as u16,
            base_low: addr as u16,
            base_mid: (addr >> 16) as u8,
            // 0x89: Present (1), DPL (00), System (0), Type (1001 = 64-bit TSS Available)
            type_flags: 0x89,
            // The upper 4 bits of the limit
            limit_high_flags: ((limit >> 16) & 0x0f) as u8,
            base_high: (addr >> 24) as u8,
            base_upper: (addr >> 32) as u32,
            reserved: 0,
        }
    }

    #[inline]
    pub const fn to_gdt_entries(self) -> [GdtEntry; 2] {
        let [lower, upper] = unsafe { mem::transmute(self) };
        [GdtEntry(lower), GdtEntry(upper)]
    }
}
