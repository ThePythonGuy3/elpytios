use core::arch::asm;

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum Msr {
    /// - Read-write register.
    /// - Bit 10: x2APIC enable (turns off MMIO, maps registers to MSR).
    /// - Bit 11: APIC global enable (must be 1 for APIC functionality).
    /// - Bit 12-51: [`PAddr::addr()`](elpytios_bootinfo::PAddr::addr) without bits 0..=11 (i.e.
    ///   must be page-aligned).
    Ia32ApicBase = 0x001b,
    /// - Read-write register.
    /// - Bit 0: `syscall` and `sysret` enable.
    /// - Bit 8: Long-mode enable.
    Ia32Efer = 0xc000_0080,
    /// - Read-write register.
    /// - Bit 0-31:
    /// - Bit 32-63:
    Ia32Star = 0xc000_0081,
    /// - Read-write register.
    /// - Bit 0-63: `syscall` entry stub naked function pointer.
    Ia32Lstar = 0xc000_0082,
    /// - Read-write register.
    /// -
    Ia32Fmask = 0xc000_0084,
    /// - Read-only register.
    /// - Bit 0-31: Unique 32-bit physical hardware ID.
    X2ApicId = 0x802,
    /// - Read-only register.
    /// - Bit 0-7: Version number.
    /// - Bit 16-23: Max LVT entries.
    X2ApicVersion = 0x803,
    /// - Write-only register.
    /// - Write a dummy 0 to clear in-service flag.
    X2ApicEoi = 0x80b,
    /// - Read-write register.
    /// - Bit 0-7: Set fallback handler vector.
    /// - Bit 8: APIC enable in software.
    X2ApicSivr = 0x80f,
    /// - Read-write register.
    /// - Bit 0-7: Vector.
    /// - Bit 8-10: Delivery mode (000=Fixed, 100=INIT, 101=SIPI).
    /// - Bit 14: Assert flag.
    /// - Bit 32-63: Target core destination APIC ID (as specified in [`Self::X2ApicId`]).
    X2ApicIcr = 0x830,
    /// - Read-write register.
    /// - Bit 0-7: Vector.
    /// - Bit 17-18: Mode (00=One-shot, 01=Periodic).
    X2ApicLvtTimer = 0x832,
}

#[inline(always)]
pub unsafe fn rdmsr(address: Msr) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",

            in("ecx") address as u32,
            out("eax") low,
            out("edx") high,

            options(nomem, nostack, preserves_flags)
        );
    }

    ((high as u64) << 32) | (low as u64)
}

#[inline(always)]
pub unsafe fn wrmsr(address: Msr, value: u64) {
    unsafe {
        asm!(
            "wrmsr",

            in("ecx") address as u32,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,

            options(nomem, nostack, preserves_flags)
        );
    }
}
