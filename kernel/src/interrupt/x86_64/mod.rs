use core::{arch::naked_asm, mem::offset_of};

use crate::device::CpuContext;

mod gdt;
mod idt;
mod syscall;
mod tss;
pub use gdt::*;
pub use idt::*;
pub use syscall::*;
pub use tss::*;

mod sealed {
    pub trait InterruptError: Sized {
        const STACK_ADJUST: usize;
    }

    impl InterruptError for () {
        const STACK_ADJUST: usize = 0;
    }

    impl InterruptError for u64 {
        const STACK_ADJUST: usize = 8;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InterruptFrame<Error: sealed::InterruptError = ()> {
    // General-purpose registers, pushed by software
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    // Interupt info, pushed by hardware
    pub error: Error,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl<Error: sealed::InterruptError> InterruptFrame<Error> {
    pub const STACK_ADJUST: usize = Error::STACK_ADJUST;
}

impl<Error: sealed::InterruptError> InterruptFrame<Error> {
    /// # Safety
    /// - Invoke this function with `call` instruction directly.
    /// - `%rsp` must point to [`Self::error`] before the `call` instruction, which is guaranteed
    ///   inside interrupt handlers.
    /// - After this function returns, `%rsp` is now a 16 bytes-aligned address.
    #[unsafe(naked)]
    pub unsafe extern "sysv64" fn save() -> ! {
        naked_asm!(
            "subq $({error} - {rax} + {adj} - 8), %rsp", // `%rsp->r15` is currently the return address

            "movq %rax, ({adj} + {rax})(%rsp)",
            "movq ({adj} + {r15})(%rsp), %rax", // `%rax` is now clobbered, containing the return address
            "movq %rcx, ({adj} + {rcx})(%rsp)",
            "movq %rdx, ({adj} + {rdx})(%rsp)",
            "movq %rsi, ({adj} + {rsi})(%rsp)",
            "movq %rdi, ({adj} + {rdi})(%rsp)",
            "movq %r8,  ({adj} + {r8})(%rsp)",
            "movq %r9,  ({adj} + {r9})(%rsp)",
            "movq %r10, ({adj} + {r10})(%rsp)",
            "movq %r11, ({adj} + {r11})(%rsp)",
            "movq %rbx, ({adj} + {rbx})(%rsp)",
            "movq %rbp, ({adj} + {rbp})(%rsp)",
            "movq %r12, ({adj} + {r12})(%rsp)",
            "movq %r13, ({adj} + {r13})(%rsp)",
            "movq %r14, ({adj} + {r14})(%rsp)",
            "movq %r15, ({adj} + {r15})(%rsp)",

            "jmpq *%rax",

            adj = const Error::STACK_ADJUST,

            rax = const offset_of!(Self, rax),
            rcx = const offset_of!(Self, rcx),
            rdx = const offset_of!(Self, rdx),
            rsi = const offset_of!(Self, rsi),
            rdi = const offset_of!(Self, rdi),
            r8 = const offset_of!(Self, r8),
            r9 = const offset_of!(Self, r9),
            r10 = const offset_of!(Self, r10),
            r11 = const offset_of!(Self, r11),
            rbx = const offset_of!(Self, rbx),
            rbp = const offset_of!(Self, rbp),
            r12 = const offset_of!(Self, r12),
            r13 = const offset_of!(Self, r13),
            r14 = const offset_of!(Self, r14),
            r15 = const offset_of!(Self, r15),
            error = const offset_of!(Self, error),

            options(att_syntax),
        )
    }

    /// # Safety
    /// - Invoke this function with `call` instruction directly.
    /// - `%rsp` must be restored to wherever it points to after [`Self::push`] returns.
    /// - After this function returns, `%rsp` now points to [`Self::error`], which is convenient for
    ///   `iretq`s (in case the interrupt has an error, you must `addq $8, %rsp` as well to clear
    ///   the error code).
    #[unsafe(naked)]
    pub unsafe extern "sysv64" fn load() -> ! {
        naked_asm!(
            "addq $8, %rsp",

            "movq ({adj} + {rax})(%rsp), %rax",
            "movq ({adj} + {rcx})(%rsp), %rcx",
            "movq ({adj} + {rdx})(%rsp), %rdx",
            "movq ({adj} + {rsi})(%rsp), %rsi",
            "movq ({adj} + {rdi})(%rsp), %rdi",
            "movq ({adj} + {r8})(%rsp),  %r8",
            "movq ({adj} + {r9})(%rsp),  %r9",
            "movq ({adj} + {r10})(%rsp), %r10",
            "movq ({adj} + {r11})(%rsp), %r11",
            "movq ({adj} + {rbx})(%rsp), %rbx",
            "movq ({adj} + {rbp})(%rsp), %rbp",
            "movq ({adj} + {r12})(%rsp), %r12",
            "movq ({adj} + {r13})(%rsp), %r13",
            "movq ({adj} + {r14})(%rsp), %r14",
            "movq ({adj} + {r15})(%rsp), %r15",

            "addq $({error} - {rax} + {adj}), %rsp",
            "jmpq *-({error} - {rax} + {adj} + 8)(%rsp)",

            adj = const Error::STACK_ADJUST,

            rax = const offset_of!(Self, rax),
            rcx = const offset_of!(Self, rcx),
            rdx = const offset_of!(Self, rdx),
            rsi = const offset_of!(Self, rsi),
            rdi = const offset_of!(Self, rdi),
            r8 = const offset_of!(Self, r8),
            r9 = const offset_of!(Self, r9),
            r10 = const offset_of!(Self, r10),
            r11 = const offset_of!(Self, r11),
            rbx = const offset_of!(Self, rbx),
            rbp = const offset_of!(Self, rbp),
            r12 = const offset_of!(Self, r12),
            r13 = const offset_of!(Self, r13),
            r14 = const offset_of!(Self, r14),
            r15 = const offset_of!(Self, r15),
            error = const offset_of!(Self, error),

            options(att_syntax),
        )
    }
}

/// # Safety
/// Only call this once per CPU core in setup phase after higher-half addressing is finished.
pub unsafe fn init_interrupts(cpu: &'static CpuContext) {
    unsafe {
        init_gdt(cpu);
        init_idt();
        init_syscalls();
    }
}
