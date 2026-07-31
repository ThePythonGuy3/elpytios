pub type Syscall0Fn = unsafe extern "sysv64" fn() -> usize;
pub type Syscall1Fn<A0> = unsafe extern "sysv64" fn(A0) -> usize;
pub type Syscall2Fn<A0, A1> = unsafe extern "sysv64" fn(A0, A1) -> usize;
pub type Syscall3Fn<A0, A1, A2> = unsafe extern "sysv64" fn(A0, A1, A2) -> usize;
pub type Syscall4Fn<A0, A1, A2, A3> = unsafe extern "sysv64" fn(A0, A1, A2, A3) -> usize;
pub type Syscall5Fn<A0, A1, A2, A3, A4> = unsafe extern "sysv64" fn(A0, A1, A2, A3, A4) -> usize;
pub type Syscall6Fn<A0, A1, A2, A3, A4, A5> = unsafe extern "sysv64" fn(A0, A1, A2, A3, A4, A5) -> usize;
