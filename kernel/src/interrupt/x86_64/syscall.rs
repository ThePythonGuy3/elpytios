use core::arch::naked_asm;

use elpytios_abi::{FileHandle, Syscall, SyscallArg, SyscallEntry};

use crate::{
    arch::x86_64::{Msr, rdmsr, wrmsr},
    device::CpuContext,
    interrupt::x86_64::InterruptStack,
    spin_sync::SpinOnce,
};

pub unsafe extern "sysv64" fn write(_file: FileHandle, _buffer: *const u8, _len: usize) -> usize {
    Syscall::INVALID
}

pub unsafe extern "sysv64" fn read(_file: FileHandle, _buffer: *mut u8, _len: usize) -> usize {
    Syscall::INVALID
}

pub unsafe extern "sysv64" fn mem_map(file: FileHandle, offset: usize, page_count: usize, flags: usize) -> usize {
    unsafe { super::super::mem_map(file, offset, page_count, flags) }.into_usize()
}

pub unsafe extern "sysv64" fn yield_now() -> usize {
    crate::task::schedule();
    0
}

pub unsafe fn init_syscalls() {
    static mut SYSCALL_ENTRIES: [SyscallEntry; Syscall::MAX_ENTRIES] = [SyscallEntry::MISSING; Syscall::MAX_ENTRIES];
    static SYSCALL_INIT: SpinOnce = SpinOnce::new();

    SYSCALL_INIT.call_once(|| unsafe {
        SYSCALL_ENTRIES[Syscall::Write as usize] = SyscallEntry { write };
        SYSCALL_ENTRIES[Syscall::Read as usize] = SyscallEntry { read };

        SYSCALL_ENTRIES[Syscall::MemMap as usize] = SyscallEntry { mem_map };

        SYSCALL_ENTRIES[Syscall::YieldNow as usize] = SyscallEntry { yield_now };
    });

    unsafe {
        // Enable `syscall` and `sysret`
        wrmsr(Msr::Ia32Efer, rdmsr(Msr::Ia32Efer) | (1 << 0));
        // 0x08: KERNEL_CODE
        // 0x10 + 8  = 0x18: USER_DATA
        // 0x10 + 16 = 0x20: USER_CODE
        wrmsr(Msr::Ia32Star, (0x08 << 32) | (0x10 << 48));
        wrmsr(Msr::Ia32Lstar, syscall as *const () as u64);
        // Bit 9: Interrupt flag (`cli`)
        // Bit 10: Direction flag (`cld`)
        // Bit 18: Alignment check
        wrmsr(Msr::Ia32Fmask, (1 << 9) | (1 << 10) | (1 << 18));

        #[unsafe(naked)]
        pub unsafe extern "sysv64" fn syscall() -> ! {
            naked_asm!(
                // `rax` is the `syscall` entry, immediately bail if invalid
                "cmpq ${max_entries}, %rax",
                "jae 2f",

                // `rax` is now address of the handler, bail if not set (null)
                // `r12` is supposed to be treated as caller-saved (breaking the traditional Sys V, but it's not a strict requirement anyway)
                "leaq {entries}(%rip), %r12",
                "movq (%r12, %rax, 8), %rax",
                "testq %rax, %rax",
                "jz 2f",

                // Switch to kernel stack
                "swapgs",
                "movq %rsp, %r12",
                "movq %gs:{stack}, %rsp",

                "push %r12",
                "push %r11",
                "push %rcx",
                "subq $8, %rsp",

                // User uses `r10` instead of `rcx`, but Sys V expects `rcx` to be 4th arg
                "movq %r10, %rcx",
                // After this, `rax` is now the return value of the handler
                "callq *%rax",

                "addq $8, %rsp",
                "pop %rcx",
                "pop %r11",
                // Switch back to user stack
                "pop %rsp",
                "swapgs",

                "sysretq",

                "2:",
                "movq ${invalid}, %rax",
                "sysretq",

                max_entries = const Syscall::MAX_ENTRIES,
                entries = sym SYSCALL_ENTRIES,
                stack = const CpuContext::stack_addr(InterruptStack::Task),
                invalid = const Syscall::INVALID,

                options(att_syntax),
            )
        }
    }
}
