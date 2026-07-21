use alloc::boxed::Box;
use core::{
    arch::{asm, naked_asm},
    cell::Cell,
    mem::{self, offset_of, size_of_val_raw},
};

use bitflags::bitflags;
use bytemuck::Zeroable;
use elpytios_abi::{Syscall, SyscallEntry};

use crate::{
    arch::x86_64::{Msr, rdmsr, wrmsr},
    device::CpuContext,
    spin_sync::SpinOnce,
};

#[repr(C, packed)]
pub struct Tss {
    reserved0: u32,
    /// Switch to this stack only when intercepting an interrupt from Ring 3.
    pub rsp0: Cell<u64>,
    pub rsp1: Cell<u64>,
    pub rsp2: Cell<u64>,
    reserved1: u64,
    pub ist: Cell<[u64; 7]>,
    reserved2: u64,
    reserved3: u16,
    iopb_offset: u16,
}

impl Tss {
    #[inline]
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp0: Cell::new(0),
            rsp1: Cell::new(0),
            rsp2: Cell::new(0),
            reserved1: 0,
            ist: Cell::new([0; 7]),
            reserved2: 0,
            reserved3: 0,
            iopb_offset: 0xffff,
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
            "lgdt [{ptr}]",
            // TSS selector at 0x28
            "mov ax, 0x28",
            "ltr ax",

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
        const DPL_RING_1     = 1 << 12;
        const DPL_RING_2     = 2 << 12;
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

mod sealed {
    pub trait InterruptError: Sized {}

    impl InterruptError for () {}
    impl InterruptError for u64 {}
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InterruptFrame<Error: sealed::InterruptError = ()> {
    // General-purpose registers, pushed by software
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
    // Interupt info, pushed by hardware
    pub error: Error,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

macro_rules! interrupt {
    (#[$($has_error:tt)*] $handle:ident) => {
        {
            const _: unsafe extern "sysv64" fn(&mut interrupt!(type => #[$($has_error)*])) = $handle;
            naked_asm!(
                r#"
                cld
                push rax
                push rcx
                push rdx
                push rsi
                push rdi
                push r8
                push r9
                push r10
                push r11
                push rbx
                push rbp
                push r12
                push r13
                push r14
                push r15

                test qword ptr [rsp + {cs}], 3
                jz 2f
                swapgs
                2:

                lea rdi, [rsp]
                sub rsp, {rsp_adj}
                call {handle}
                add rsp, {rsp_adj}

                test qword ptr [rsp + {cs}], 3
                jz 3f
                swapgs
                3:

                pop r15
                pop r14
                pop r13
                pop r12
                pop rbp
                pop rbx
                pop r11
                pop r10
                pop r9
                pop r8
                pop rdi
                pop rsi
                pop rdx
                pop rcx
                pop rax
                "#,

                interrupt!(clear => #[$($has_error)*]),

                r#"
                sti
                iretq
                "#,

                cs = const offset_of!(interrupt!(type => #[$($has_error)*]), cs),
                rsp_adj = const interrupt!(rsp_adj => #[$($has_error)*]),
                handle = sym $handle,
            )
        }
    };
    (type => #[error]) => {
        InterruptFrame<u64>
    };
    (type => #[not(error)]) => {
        InterruptFrame<()>
    };
    (rsp_adj => #[error]) => {
        8
    };
    (rsp_adj => #[not(error)]) => {
        0
    };
    (clear => #[error]) => {
        "add rsp, 8"
    };
    (clear => #[not(error)]) => {
        ""
    };
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn double_fault() -> ! {
    unsafe extern "sysv64" fn handle(frame: &mut InterruptFrame<u64>) {
        panic!("Double-fault caught (Hardware error code: {})", frame.error)
    }

    interrupt!(
        #[error]
        handle
    )
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn page_fault() -> ! {
    #[repr(transparent)]
    struct ErrorCode(u64);
    bitflags! {
        impl ErrorCode: u64 {
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

    unsafe extern "sysv64" fn handle(frame: &mut InterruptFrame<u64>) {
        unsafe {
            let ptr: *mut ();
            asm!("mov {ptr}, cr2", ptr = out(reg) ptr);

            let code = ErrorCode(frame.error);
            super::page_fault(
                ptr,
                code.contains(ErrorCode::NOT_PRESENT),
                code.contains(ErrorCode::IS_WRITE),
                code.contains(ErrorCode::IS_USER),
                code.contains(ErrorCode::RESERVED),
                code.contains(ErrorCode::EXECUTE),
            )
        }
    }

    interrupt!(
        #[error]
        handle
    )
}

#[allow(unused, reason = "Unimplemented")]
pub unsafe extern "sysv64" fn syscall_write(file: usize, buffer: usize, len: usize) -> usize {
    Syscall::INVALID
}

#[allow(unused, reason = "Unimplemented")]
pub unsafe extern "sysv64" fn syscall_read(file: usize, buffer: usize, len: usize) -> usize {
    Syscall::INVALID
}

pub unsafe fn init_syscalls() {
    static mut SYSCALL_ENTRIES: [SyscallEntry; Syscall::MAX_ENTRIES] = [SyscallEntry::MISSING; Syscall::MAX_ENTRIES];
    static SYSCALL_INIT: SpinOnce = SpinOnce::new();

    SYSCALL_INIT.call_once(|| unsafe {
        SYSCALL_ENTRIES[Syscall::Write as usize] = SyscallEntry { write: syscall_write };
        SYSCALL_ENTRIES[Syscall::Read as usize] = SyscallEntry { read: syscall_read };
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

                // Switch to kernel stack
                "swapgs",
                "mov r12, rsp",
                "mov rsp, gs:[{kernel_stack}]",
                "and rsp, -16",

                "push r12",
                "push r11",
                "push rcx",
                "sub rsp, 8",
                "sti",

                // User uses `r10` instead of `rcx`, but Sys V expects `rcx` to be 4th arg
                "mov rcx, r10",
                // After this, `rax` is now the return value of the handler
                "call rax",

                "cli",
                "add rsp, 8",
                "pop rcx",
                "pop r11",
                // Switch back to user stack
                "pop rsp",
                "swapgs",

                "sysretq",

                "2:",
                "mov rax, {invalid}",
                "sysretq",

                max_entries = const Syscall::MAX_ENTRIES,
                entries = sym SYSCALL_ENTRIES,
                entry_size = const size_of::<SyscallEntry>(),
                kernel_stack = const offset_of!(CpuContext, tss.rsp0),
                invalid = const Syscall::INVALID,
            )
        }
    }
}

/// # Safety
/// Only call this once per CPU core in setup phase after higher-half addressing is finished.
pub unsafe fn init_interrupts(cpu: &'static CpuContext) {
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
        init_gdt(cpu);

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
