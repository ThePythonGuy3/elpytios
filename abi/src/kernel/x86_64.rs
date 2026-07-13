pub type Syscall0Fn = unsafe extern "sysv64" fn() -> usize;
pub type Syscall1Fn = unsafe extern "sysv64" fn(usize) -> usize;
pub type Syscall2Fn = unsafe extern "sysv64" fn(usize, usize) -> usize;
pub type Syscall3Fn = unsafe extern "sysv64" fn(usize, usize, usize) -> usize;
pub type Syscall4Fn = unsafe extern "sysv64" fn(usize, usize, usize, usize) -> usize;
pub type Syscall5Fn = unsafe extern "sysv64" fn(usize, usize, usize, usize, usize) -> usize;
pub type Syscall6Fn = unsafe extern "sysv64" fn(usize, usize, usize, usize, usize, usize) -> usize;
