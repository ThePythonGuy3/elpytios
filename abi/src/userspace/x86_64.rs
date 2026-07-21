use core::arch::asm;

#[inline(always)]
pub unsafe fn syscall0(sys: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") sys => ret,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
    }
    ret
}

#[inline(always)]
pub unsafe fn syscall1(sys: usize, a0: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") sys => ret,

            in("rdi") a0,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
    }
    ret
}

#[inline(always)]
pub unsafe fn syscall2(sys: usize, a0: usize, a1: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") sys => ret,

            in("rdi") a0,
            in("rsi") a1,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
    }
    ret
}

#[inline(always)]
pub unsafe fn syscall3(sys: usize, a0: usize, a1: usize, a2: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") sys => ret,

            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
    }
    ret
}

#[inline(always)]
pub unsafe fn syscall4(sys: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") sys => ret,

            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
    }
    ret
}

#[inline(always)]
pub unsafe fn syscall5(sys: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") sys => ret,

            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8")  a4,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
    }
    ret
}

#[inline(always)]
pub unsafe fn syscall6(sys: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") sys => ret,

            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8")  a4,
            in("r9")  a5,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
    }
    ret
}
