use alloc::boxed::Box;
use core::{
    arch::{asm, naked_asm},
    mem::{self, size_of_val_raw},
};

use bitflags::bitflags;
use bytemuck::Zeroable;
use elpytios_abi::{Syscall, SyscallReadFn, SyscallWriteFn};

use crate::{
    arch::x86_64::{Msr, rdmsr, wrmsr},
    device::CpuContext,
    spin_sync::SpinOnce,
    swap_ctx,
};

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct GdtEntry(u64);
bitflags! {
    impl GdtEntry: u64 {
        const ACCESSED        = 1 << 40;
        const WRITABLE        = 1 << 41;
        const EXECUTABLE      = 1 << 43;
        const DESCRIPTOR_TYPE = 1 << 44;

        const DPL_0           = 0 << 45;
        const DPL_1           = 1 << 45;
        const DPL_2           = 2 << 45;
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

pub unsafe fn init_gdt() {
    #[repr(C, packed)]
    struct GdtPointer {
        limit: u16,
        base: *mut GdtEntry,
    }

    unsafe {
        let entries = Box::leak(Box::new([
            GdtEntry::NULL,        // 0x00
            GdtEntry::KERNEL_CODE, // 0x08
            GdtEntry::KERNEL_DATA, // 0x10
            GdtEntry::USER_DATA,   // 0x18
            GdtEntry::USER_CODE,   // 0x20
        ]));

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

const INTERRUPT_CLOBBERED: usize = 9 * size_of::<usize>();
macro_rules! interrupt_clobbered {
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
        interrupt_clobbered!(push),

        "mov rdi, [rsp + {clobbered}]",
        "call {handle}",

        interrupt_clobbered!(pop),
        "add rsp, 8",
        "iretq",

        clobbered = const INTERRUPT_CLOBBERED,
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
        interrupt_clobbered!(push),

        "mov rdi, [rsp + {clobbered}]",
        "call {handle}",

        interrupt_clobbered!(pop),
        "add rsp, 8",
        "iretq",

        clobbered = const INTERRUPT_CLOBBERED,
        handle = sym handle,
    )
}

pub unsafe extern "sysv64" fn syscall_write(file: usize, buffer: usize, len: usize) -> usize {
    Syscall::INVALID
}

pub unsafe extern "sysv64" fn syscall_read(file: usize, buffer: usize, len: usize) -> usize {
    Syscall::INVALID
}

pub unsafe fn init_syscalls() {
    #[derive(Clone, Copy)]
    #[repr(C)]
    union SyscallEntry {
        missing: pattern_type!(usize is 0..=0),
        syscall_write: SyscallWriteFn,
        syscall_read: SyscallReadFn,
    }

    static mut SYSCALL_ENTRIES: [SyscallEntry; Syscall::MAX_ENTRIES] = [SyscallEntry {
        missing: unsafe { mem::transmute::<usize, _>(0) },
    }; Syscall::MAX_ENTRIES];
    static SYSCALL_INIT: SpinOnce = SpinOnce::new();

    SYSCALL_INIT.call_once(|| unsafe {
        SYSCALL_ENTRIES[Syscall::Write as usize] = SyscallEntry { syscall_write };
        SYSCALL_ENTRIES[Syscall::Read as usize] = SyscallEntry { syscall_read };
    });

    unsafe {
        // Enable `syscall` and `sysret`
        wrmsr(Msr::Ia32Efer, rdmsr(Msr::Ia32Efer) | (1 << 0));
        // 0x08: KERNEL_CODE
        // 0x10 + 8  = 0x18: USER_DATA
        // 0x10 + 16 = 0x20: USER_CODE
        wrmsr(Msr::Ia32Star, (0x08 << 32) | (0x10 << 48));
        wrmsr(Msr::Ia32Lstar, syscall as *const () as u64);
        // - Bit 9: Interrupt flag (`cli`)
        // - Bit 10: Direction flag (`cld`)
        // - Bit 18: Alignment check
        wrmsr(Msr::Ia32Fmask, (1 << 9) | (1 << 10) | (1 << 18));

        #[unsafe(naked)]
        pub unsafe extern "sysv64" fn syscall() -> ! {
            naked_asm!(
                // `rax` is the `syscall` entry, immediately bail if invalid
                "cmp rax, {max_entries}",
                "jae 2f",

                // `rax` is now address of the handler, bail if not set (null)
                "lea r12, [rip + {entries}]",
                "mov rax, [r12 + rax * {entry_size}]",
                "test rax, rax",
                "jz 2f",

                swap_ctx!(user => kernel),
                "sti",
                "push r11",
                "push rcx",

                // User uses `r10` instead of `rcx`, but Sys V expects `rcx` to be 4th arg
                "mov rcx, r10",
                // After this, `rax` is now the return value of the handler
                "call rax",

                "pop rcx",
                "pop r11",
                "cli",
                swap_ctx!(kernel => user),

                "sysretq",

                "2:",
                "mov rax, {invalid}",
                "sysretq",

                max_entries = const Syscall::MAX_ENTRIES,
                entries = sym SYSCALL_ENTRIES,
                entry_size = const size_of::<SyscallEntry>(),
                invalid = const Syscall::INVALID,

                kernel_stack_offset = const CpuContext::KERNEL_STACK,
                user_stack_offset = const CpuContext::USER_STACK,
            )
        }
    }
}

/// # Safety
/// Only call this once per CPU core in setup phase after higher-half addressing is finished.
pub unsafe fn init_interrupts() {
    static mut IDT_ENTRIES: [IdtEntry; 256] = [bytemuck::zeroed(); 256];
    static IDT_INIT: SpinOnce = SpinOnce::new();

    IDT_INIT.call_once(|| unsafe {
        IDT_ENTRIES[IdtIndex::DoubleFault as usize] = IdtEntry::new(double_fault);
        IDT_ENTRIES[IdtIndex::PageFault as usize] = IdtEntry::new(page_fault);
    });

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

        init_syscalls();
    }
}
