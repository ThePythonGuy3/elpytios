#[inline(always)]
pub unsafe fn syscall0(sys: usize) -> usize {
    unimplemented!("{sys}")
}

#[inline(always)]
pub unsafe fn syscall1(sys: usize, a0: usize) -> usize {
    unimplemented!("{sys}, {a0}")
}

#[inline(always)]
pub unsafe fn syscall2(sys: usize, a0: usize, a1: usize) -> usize {
    unimplemented!("{sys}, {a0}, {a1}")
}

#[inline(always)]
pub unsafe fn syscall3(sys: usize, a0: usize, a1: usize, a2: usize) -> usize {
    unimplemented!("{sys}, {a0}, {a1}, {a2}")
}

#[inline(always)]
pub unsafe fn syscall4(sys: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> usize {
    unimplemented!("{sys}, {a0}, {a1}, {a2}, {a3}")
}

#[inline(always)]
pub unsafe fn syscall5(sys: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> usize {
    unimplemented!("{sys}, {a0}, {a1}, {a2}, {a3}, {a4}")
}

#[inline(always)]
pub unsafe fn syscall6(sys: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> usize {
    unimplemented!("{sys}, {a0}, {a1}, {a2}, {a3}, {a4}, {a5}")
}
