use elpytios_abi::Syscall;

use crate::{
    ffi::CStr,
    io,
    num::NonZero,
    thread::ThreadInit,
    time::{Duration, Instant},
};

// Silence dead code warnings for the otherwise unused ThreadInit::init() call.
#[expect(dead_code)]
fn dummy_init_call(init: Box<ThreadInit>) {
    drop(init.init());
}

pub struct Thread(!);

pub const DEFAULT_MIN_STACK_SIZE: usize = 64 * 1024;

impl Thread {
    // unsafe: see thread::Builder::spawn_unchecked for safety requirements
    pub unsafe fn new(_stack: usize, _init: Box<ThreadInit>) -> io::Result<Thread> {
        Err(io::Error::UNSUPPORTED_PLATFORM)
    }

    pub fn join(self) {
        self.0
    }
}

pub fn available_parallelism() -> io::Result<NonZero<usize>> {
    Err(io::Error::UNKNOWN_THREAD_COUNT)
}

pub fn current_os_id() -> Option<u64> {
    None
}

#[inline]
pub fn yield_now() {
    unsafe {
        Syscall::yield_now();
    }
}

pub fn set_name(_name: &CStr) {
    // nope
}

pub fn sleep(_dur: Duration) {
    panic!("can't sleep");
}

pub fn sleep_until(deadline: Instant) {
    // The clock source used for `sleep` might not be the same used for `Instant`.
    // Since this function *must not* return before the deadline, we recheck the
    // time after every call to `sleep`. See #149935 for an example of this
    // occurring on older Windows systems.
    while let Some(delay) = deadline.checked_duration_since(Instant::now()) {
        // Sleep for the estimated time remaining until the deadline.
        //
        // If your system has a better way of estimating the delay time or
        // provides a way to sleep until an absolute time, specialize this
        // function for your system.
        sleep(delay);
    }
}
