use core::{arch::asm, mem::size_of_val_raw};

use bitflags::bitflags;

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct GdtEntry(u64);
bitflags! {
    impl GdtEntry: u64 {
        const ACCESSED        = 1 << 40;
        const WRITABLE        = 1 << 41;
        const EXECUTABLE      = 1 << 43;
        const DESCRIPTOR_TYPE = 1 << 44;
        const PRESENT         = 1 << 47;
        const LONG_MODE       = 1 << 53;
    }
}

impl GdtEntry {
    pub const NULL: Self = Self(0);
    pub const KERNEL_CODE: Self = Self(Self::PRESENT.0 | Self::DESCRIPTOR_TYPE.0 | Self::EXECUTABLE.0 | Self::LONG_MODE.0);
    pub const KERNEL_DATA: Self = Self(Self::PRESENT.0 | Self::DESCRIPTOR_TYPE.0 | Self::WRITABLE.0);
}

pub unsafe fn init_gdt() {
    /// # Safety
    /// This *must* be a `static mut`, because the CPU writes the `ACCESSED` bit to it.
    static mut GDT_ENTRIES: [GdtEntry; 3] = [GdtEntry::NULL, GdtEntry::KERNEL_CODE, GdtEntry::KERNEL_DATA];

    #[derive(Debug, Clone, Copy)]
    #[repr(C, packed)]
    struct GdtPointer {
        limit: u16,
        base: *mut GdtEntry,
    }

    unsafe {
        let ptr = GdtPointer {
            limit: u16::try_from(size_of_val_raw(&raw const GDT_ENTRIES) - 1).unwrap(),
            base: (&raw mut GDT_ENTRIES).cast(),
        };

        asm!(
            "lgdt [{ptr}]",
            // `KERNEL_DATA` selector is 0x10
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            // `KERNEL_CODE` selector is 0x08
            "push 0x08",
            // Perform a long jump, loading the GDT entries
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",

            ptr = in(reg) &ptr,
            out("rax") _,
        );
    }
}

/// # Safety
/// - Only call this once in setup phase after higher-half addressing is finished.
pub unsafe fn init_interrupts() {
    unsafe {
        init_gdt();
    }
}
