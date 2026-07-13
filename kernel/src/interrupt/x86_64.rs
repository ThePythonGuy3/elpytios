use alloc::boxed::Box;
use core::{
    arch::{asm, naked_asm},
    hint::{cold_path, spin_loop},
    mem::size_of_val_raw,
    sync::atomic::{
        AtomicU8,
        Ordering::{Acquire, Relaxed, Release},
    },
};

use bitflags::bitflags;
use bytemuck::Zeroable;

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
    #[repr(C, packed)]
    struct GdtPointer {
        limit: u16,
        base: *mut GdtEntry,
    }

    unsafe {
        let entries = Box::leak(Box::new([GdtEntry::NULL, GdtEntry::KERNEL_CODE, GdtEntry::KERNEL_DATA]));
        let ptr = GdtPointer {
            limit: u16::try_from(size_of_val(entries) - 1).unwrap(),
            base: (&raw mut *entries).cast(),
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

#[derive(Copy, Clone, Zeroable)]
#[repr(C, packed)]
pub struct IdtEntry {
    pointer_low: u16,
    gdt_selector: u16,
    options: IdtOptions,
    pointer_middle: u16,
    pointer_high: u32,
    reserved: u32,
}

impl IdtEntry {
    #[inline]
    pub unsafe fn new(handler: unsafe extern "sysv64" fn() -> !) -> Self {
        let addr = handler as usize;
        Self {
            pointer_low: addr as u16,
            pointer_middle: (addr >> 16) as u16,
            pointer_high: (addr >> 32) as u32,
            gdt_selector: 0x08, // `KERNEL_CODE` selector
            options: IdtOptions(IdtOptions::PRESENT.0 | IdtOptions::DPL_RING_0.0 | IdtOptions::TYPE_INTERRUPT.0),
            reserved: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Zeroable)]
#[repr(transparent)]
pub struct IdtOptions(u16);
bitflags! {
    impl IdtOptions: u16 {
        const PRESENT        = 1 << 15;

        const DPL_RING_0     = 0 << 12;
        const DPL_RING_3     = 3 << 12;

        const TYPE_INTERRUPT = 0xe << 8;
        const TYPE_TRAP      = 0xf << 8;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(usize)]
pub enum IdtIndex {
    // Hard-coded by CPU
    DoubleFault = 8,
    PageFault = 14,
}

const CLOBBERED: usize = 9 * size_of::<usize>();
macro_rules! clobbered {
    (push) => {
        r#"
        push rax
        push rcx
        push rdx
        push rsi
        push rdi
        push r8
        push r9
        push r10
        push r11
        "#
    };
    (pop) => {
        r#"
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rdx
        pop rcx
        pop rax
        "#
    };
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn double_fault() -> ! {
    unsafe extern "sysv64" fn handle(code: usize) -> ! {
        panic!("Double-fault caught (Hardware error code: {code})")
    }

    naked_asm!(
        clobbered!(push),

        "mov rdi, [rsp + {clobbered}]",
        "call {handle}",

        clobbered!(pop),
        "add rsp, 8",
        "iretq",

        clobbered = const CLOBBERED,
        handle = sym handle,
    )
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn page_fault() -> ! {
    #[repr(transparent)]
    struct ErrorCode(usize);
    bitflags! {
        impl ErrorCode: usize {
            // 0=protection violation, 1=not present
            const NOT_PRESENT = 1 << 0;
            // 0=caused by read, 1=caused by read
            const IS_WRITE    = 1 << 1;
            // 0=triggered in ring 0, 1=triggered in ring 3
            const IS_USER     = 1 << 2;
            // overwrote reserved bits in page table
            const RESERVED    = 1 << 3;
            // instruction fetch violation
            const EXECUTE     = 1 << 4;
        }
    }

    unsafe extern "sysv64" fn handle(code: ErrorCode) {
        unsafe {
            let ptr: *mut ();
            asm!("mov {ptr}, cr2", ptr = out(reg) ptr);

            super::page_fault(
                ptr,
                code.contains(ErrorCode::NOT_PRESENT),
                code.contains(ErrorCode::IS_WRITE),
                code.contains(ErrorCode::IS_USER),
                code.contains(ErrorCode::RESERVED),
                code.contains(ErrorCode::EXECUTE),
            );
        }
    }

    naked_asm!(
        clobbered!(push),

        "mov rdi, [rsp + {clobbered}]",
        "call {handle}",

        clobbered!(pop),
        "add rsp, 8",
        "iretq",

        clobbered = const CLOBBERED,
        handle = sym handle,
    )
}

/// # Safety
/// Only call this once per CPU core in setup phase after higher-half addressing is finished.
pub unsafe fn init_interrupts() {
    static mut IDT_ENTRIES: [IdtEntry; 256] = [bytemuck::zeroed(); 256];
    static IDT_STATE: AtomicU8 = AtomicU8::new(UNINIT);

    const UNINIT: u8 = 0;
    const LOCKED: u8 = 1;
    const INIT: u8 = 2;

    loop {
        match IDT_STATE.compare_exchange_weak(UNINIT, LOCKED, Acquire, Relaxed) {
            Ok(..) => unsafe {
                cold_path();
                IDT_ENTRIES[IdtIndex::DoubleFault as usize] = IdtEntry::new(double_fault);
                IDT_ENTRIES[IdtIndex::PageFault as usize] = IdtEntry::new(page_fault);

                IDT_STATE.store(INIT, Release);
            },
            Err(INIT) => break,
            Err(..) => {
                cold_path();
                spin_loop();
            }
        }
    }

    #[repr(C, packed)]
    struct IdtPointer {
        limit: u16,
        base: *mut IdtEntry,
    }

    unsafe {
        // Global descriptor table is x86-specific
        init_gdt();

        let ptr = IdtPointer {
            limit: u16::try_from(size_of_val_raw(&raw const IDT_ENTRIES) - 1).unwrap(),
            base: (&raw mut IDT_ENTRIES).cast(),
        };

        asm!(
            "lidt [{ptr}]",
            ptr = in(reg) &ptr,
        );
    }
}
