use core::{
    hint::{cold_path, spin_loop, unreachable_unchecked},
    sync::atomic::{
        AtomicU8,
        Ordering::{Acquire, Relaxed, Release},
    },
};

#[derive(Debug)]
pub struct SpinOnce(AtomicU8);
impl SpinOnce {
    const UNINIT: u8 = 0;
    const LOCKED: u8 = 1;
    const INIT: u8 = 2;

    #[inline]
    pub const fn new() -> Self {
        Self(AtomicU8::new(Self::UNINIT))
    }

    #[inline]
    pub fn call_once<T>(&self, f: impl FnOnce() -> T) -> Option<T> {
        loop {
            match self.0.compare_exchange_weak(Self::UNINIT, Self::LOCKED, Acquire, Relaxed) {
                Ok(..) => {
                    cold_path();
                    let result = f();

                    self.0.store(Self::INIT, Release);
                    break Some(result)
                }
                Err(Self::LOCKED) => {
                    cold_path();
                    spin_loop();
                }
                Err(Self::INIT) => break None,
                Err(..) => unsafe { unreachable_unchecked() },
            }
        }
    }
}
