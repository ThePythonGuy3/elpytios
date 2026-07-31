use alloc::boxed::Box;
use core::arch::asm;

use bitflags::bitflags;

use crate::{device::CpuContext, interrupt::x86_64::TssEntry};

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct GdtEntry(pub(super) u64);
bitflags! {
    impl GdtEntry: u64 {
        const ACCESSED        = 1 << 40;
        const WRITABLE        = 1 << 41;
        const EXECUTABLE      = 1 << 43;
        const DESCRIPTOR_TYPE = 1 << 44;

        const DPL_0           = 0 << 45;
        const DPL_3           = 3 << 45;

        const PRESENT         = 1 << 47;
        const LONG_MODE       = 1 << 53;
    }
}

impl GdtEntry {
    pub const NULL: Self = Self(0);

    pub const KERNEL_CODE: Self = Self(Self::PRESENT.0 | Self::DESCRIPTOR_TYPE.0 | Self::EXECUTABLE.0 | Self::LONG_MODE.0 | Self::DPL_0.0);
    pub const KERNEL_DATA: Self = Self(Self::PRESENT.0 | Self::DESCRIPTOR_TYPE.0 | Self::WRITABLE.0 | Self::DPL_0.0);

    pub const USER_CODE: Self = Self(Self::PRESENT.0 | Self::DESCRIPTOR_TYPE.0 | Self::EXECUTABLE.0 | Self::LONG_MODE.0 | Self::DPL_3.0);
    pub const USER_DATA: Self = Self(Self::PRESENT.0 | Self::DESCRIPTOR_TYPE.0 | Self::WRITABLE.0 | Self::DPL_3.0);
}

pub unsafe fn init_gdt(cpu: &'static CpuContext) {
    #[repr(C, packed)]
    struct GdtPointer {
        limit: u16,
        base: *mut GdtEntry,
    }

    let [tss_lower, tss_upper] = TssEntry::new(&cpu.tss).to_gdt_entries();
    unsafe {
        let entries = Box::leak(Box::new([
            GdtEntry::NULL,        // 0x00
            GdtEntry::KERNEL_CODE, // 0x08
            GdtEntry::KERNEL_DATA, // 0x10
            GdtEntry::USER_DATA,   // 0x18
            GdtEntry::USER_CODE,   // 0x20
            tss_lower,             // 0x28
            tss_upper,
        ]));

        let ptr = GdtPointer {
            limit: u16::try_from(size_of_val(entries) - 1).unwrap(),
            base: (&raw mut *entries).cast(),
        };

        asm!(
            "lgdt ({ptr})",
            // TSS selector at 0x28
            "movw $0x28, %ax",
            "ltr %ax",

            // `KERNEL_DATA` selector is 0x10
            "movw $0x10, %ax",
            "movw %ax, %ds",
            "movw %ax, %es",
            "movw %ax, %ss",
            // `KERNEL_CODE` selector is 0x08
            "push $0x08",
            // Perform a far jump, loading the GDT entries
            "leaq 2f(%rip), %rax",
            "push %rax",
            "lretq",
            "2:",

            ptr = in(reg) &ptr,
            out("rax") _,

            options(att_syntax),
        );
    }
}
