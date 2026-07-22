use core::{
    alloc::Layout,
    arch::{
        asm,
        x86_64::{__cpuid_count, _xrstor64, _xsave64, _xsavec64, _xsetbv},
    },
    hint::spin_loop,
    mem::Alignment,
    time::Duration,
};

#[inline(always)]
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
            "outb %al, %dx",
            in("al") value,
            in("dx") port,

            options(att_syntax, nomem, nostack, preserves_flags)
        )
    }
}

#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    unsafe {
        let value: u8;
        asm!(
            "inb %dx, %al",
            in("dx") port,
            out("al") value,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
        value
    }
}

pub fn pit_delay(mut duration: Duration) {
    const TICK_PER_SECOND: u64 = 1_193_182;
    const TICK_MAX: u64 = 1 << u16::BITS;
    const NANO_PER_SECOND: u64 = 1_000_000_000;
    const WAIT_MAX: Duration = Duration::from_nanos(TICK_MAX * NANO_PER_SECOND / TICK_PER_SECOND);

    while duration > Duration::ZERO {
        let wait = duration.min(WAIT_MAX);
        duration -= wait;

        let ticks = (wait.subsec_nanos() as u64 * TICK_PER_SECOND + (NANO_PER_SECOND - 1)) / NANO_PER_SECOND;
        let ticks = match ticks {
            0 => continue,
            t @ 1..TICK_MAX => t as u16,
            TICK_MAX => 0,
            _ => unreachable!("Tick arithmetics should ensure max wait doesn't exceed {TICK_MAX}"),
        };

        unsafe {
            let mut port_b = inb(0x61);
            port_b &= 0xFC;
            outb(0x61, port_b);

            outb(0x43, 0b10110000);

            outb(0x42, (ticks & 0xff) as u8);
            outb(0x42, ((ticks >> 8) & 0xff) as u8);

            outb(0x61, port_b | 0x01);
            loop {
                if (inb(0x61) & 0x20) != 0 {
                    break
                }
                spin_loop();
            }

            outb(0x61, port_b);
        }
    }
}

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
    /// - Bit 0-63: Pointer to CPU-local data.
    Ia32FsBase = 0xc0000100,
    /// - Read-write register.
    /// - Bit 0-63: Pointer to CPU-local data.
    Ia32GsBase = 0xc0000101,
    /// - Read-write register.
    /// - Bit 0: `syscall` and `sysret` enable.
    /// - Bit 8: Long-mode enable.
    Ia32Efer = 0xc000_0080,
    /// - Read-write register.
    /// - Bit 32-47: `KERNEL_CODE` GDT selector.
    /// - Bit 48-63: USER_BASE GDT selector (+8 must be `USER_DATA`, +16 must be `USER_CODE`).
    Ia32Star = 0xc000_0081,
    /// - Read-write register.
    /// - Bit 0-63: `syscall` entry stub naked function pointer.
    Ia32Lstar = 0xc000_0082,
    /// - Read-write register.
    /// - Bit 9: Interrupt flag (`cli`).
    /// - Bit 10: Direction flag (`cld`).
    /// - Bit 18: Alignment check.
    Ia32Fmask = 0xc000_0084,
    /// - Read-only register.
    /// - Bit 0-31: Unique 32-bit physical hardware ID.
    Ia32X2ApicId = 0x802,
    /// - Read-only register.
    /// - Bit 0-7: Version number.
    /// - Bit 16-23: Max LVT entries.
    Ia32X2ApicVersion = 0x803,
    /// - Write-only register.
    /// - Write a dummy 0 to clear in-service flag.
    Ia32X2ApicEoi = 0x80b,
    /// - Read-write register.
    /// - Bit 0-7: Set fallback handler vector.
    /// - Bit 8: APIC enable in software.
    Ia32X2ApicSivr = 0x80f,
    /// - Read-write register.
    /// - Bit 0-7: Vector.
    /// - Bit 8-10: Delivery mode (100=NMI, 101=Init, 110=Startup).
    /// - Bit 14: Assert flag.
    /// - Bit 32-63: Target core destination APIC ID (as specified in [`Self::X2ApicId`]).
    Ia32X2ApicIcr = 0x830,
    /// - Read-write register.
    /// - Bit 0-7: Vector.
    /// - Bit 17-18: Mode (00=One-shot, 01=Periodic).
    Ia32X2ApicLvtTimer = 0x832,
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

            options(att_syntax, nomem, nostack, preserves_flags)
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

            options(att_syntax, nomem, nostack, preserves_flags)
        );
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExtendedRegisters {
    layout: Layout,
    save: unsafe fn(to: *mut u8, save_mask: u64),
    mask: ExtendedRegisterMask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ExtendedRegisterMask(u64);

impl !Send for ExtendedRegisters {}
impl !Sync for ExtendedRegisters {}

#[repr(C, align(64))]
pub struct ExtendedRegisterBuffer([u8]);

impl ExtendedRegisters {
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

        let cpuid = __cpuid_count(0xd, 0);

        // Query how many bytes the extended registers would take
        let layout = Layout::from_size_alignment((cpuid.ebx as usize).max(1), Self::ALIGNMENT).expect("Extended register buffer size too large");
        // Enable all supported features to xcr0
        let mask = ExtendedRegisterMask((cpuid.edx as u64) << 32 | (cpuid.eax as u64));
        unsafe { _xsetbv(0, mask.0) }

        let cpuid = __cpuid_count(0xd, 1);
        if cpuid.eax & (1 << 1) != 0 {
            Self {
                layout,
                save: _xsavec64,
                mask,
            }
        } else {
            Self {
                layout,
                save: _xsave64,
                mask,
            }
        }
    }

    #[inline]
    pub fn layout(&self) -> Layout {
        self.layout
    }

    #[inline]
    pub fn mask(&self) -> ExtendedRegisterMask {
        self.mask
    }

    #[inline]
    pub unsafe fn save(&self, mask: ExtendedRegisterMask, to: &mut ExtendedRegisterBuffer) {
        unsafe { (self.save)(to.0.as_mut_ptr(), mask.0) }
    }

    #[inline]
    pub unsafe fn load(&self, mask: ExtendedRegisterMask, from: &ExtendedRegisterBuffer) {
        unsafe { _xrstor64(from.0.as_ptr(), mask.0) }
    }
}
