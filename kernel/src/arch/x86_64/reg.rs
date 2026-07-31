use core::{
    alloc::Layout,
    arch::{
        asm,
        x86_64::{__cpuid_count, _xrstor64, _xsave64, _xsavec64, _xsaveopt64, _xsetbv},
    },
    mem::{Alignment, MaybeUninit},
};

#[derive(Debug, Clone, Copy)]
pub struct ExtendedRegisterLayout {
    layout: Layout,
    save: unsafe fn(to: *mut u8, save_mask: u64),
    mask: ExtendedRegisterMask,
}

impl !Send for ExtendedRegisterLayout {}
impl !Sync for ExtendedRegisterLayout {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ExtendedRegisterMask(u64);

#[repr(C, align(64))]
pub struct ExtendedRegisters([MaybeUninit<u8>]);

impl ExtendedRegisterLayout {
    pub const ALIGNMENT: Alignment = unsafe { Alignment::new_unchecked(1 << 6) };

    /// # Safety
    /// Must only be called once per CPU core.
    #[inline]
    pub unsafe fn new() -> Self {
        // Enable `xsave` and `xstor`
        // x86_64 guarantees support for these instructions, so no need to check
        unsafe {
            asm!(
                "movq %cr4, {tmp}",
                "orq $(1 << 18), {tmp}",
                "movq {tmp}, %cr4",

                tmp = out(reg) _,
                options(att_syntax, nomem, nostack, preserves_flags),
            );
        }

        // Enable all supported features to xcr0
        let leaf0 = __cpuid_count(0xd, 0);
        let mask = ExtendedRegisterMask((leaf0.edx as u64) << 32 | (leaf0.eax as u64));
        unsafe { _xsetbv(0, mask.0) }

        // Query how many bytes the extended registers would take
        let leaf1 = __cpuid_count(0xd, 1);
        let (size, save): (u32, unsafe fn(to: *mut u8, save_mask: u64)) = if leaf1.eax & (1 << 1) != 0 {
            (leaf1.ebx, _xsavec64)
        } else if leaf1.eax & (1 << 0) != 0 {
            (leaf0.ebx, _xsaveopt64)
        } else {
            (leaf0.ebx, _xsave64)
        };

        let layout = Layout::from_size_alignment(size as usize, Self::ALIGNMENT).expect("Extended register buffer size too large");
        Self { layout, save, mask }
    }

    #[inline]
    pub fn layout(&self) -> Layout {
        self.layout
    }

    #[inline]
    pub fn mask(&self) -> ExtendedRegisterMask {
        self.mask
    }

    #[inline(always)]
    pub unsafe fn save(&self, mask: ExtendedRegisterMask, to: *mut ExtendedRegisters) {
        unsafe { (self.save)((&raw mut (*to).0).as_mut_ptr().cast(), mask.0) }
    }

    #[inline(always)]
    pub unsafe fn load(&self, mask: ExtendedRegisterMask, from: *const ExtendedRegisters) {
        unsafe { _xrstor64((&raw const (*from).0).as_ptr().cast(), mask.0) }
    }
}
