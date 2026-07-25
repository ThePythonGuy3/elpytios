pub type Syscall0Fn<R> = unsafe extern "sysv64" fn() -> R;
pub type Syscall1Fn<A0, R> = unsafe extern "sysv64" fn(A0) -> R;
pub type Syscall2Fn<A0, A1, R> = unsafe extern "sysv64" fn(A0, A1) -> R;
pub type Syscall3Fn<A0, A1, A2, R> = unsafe extern "sysv64" fn(A0, A1, A2) -> R;
pub type Syscall4Fn<A0, A1, A2, A3, R> = unsafe extern "sysv64" fn(A0, A1, A2, A3) -> R;
pub type Syscall5Fn<A0, A1, A2, A3, A4, R> = unsafe extern "sysv64" fn(A0, A1, A2, A3, A4) -> R;
pub type Syscall6Fn<A0, A1, A2, A3, A4, A5, R> = unsafe extern "sysv64" fn(A0, A1, A2, A3, A4, A5) -> R;
