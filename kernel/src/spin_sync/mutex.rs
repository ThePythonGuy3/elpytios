use core::{
    cell::SyncUnsafeCell,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering::*},
};

#[derive(Debug)]
pub struct SpinMutex<T: ?Sized> {
    locked: AtomicBool,
    value: SyncUnsafeCell<T>,
}

impl<T: ?Sized> SpinMutex<T> {
    #[inline]
    pub const fn new(value: T) -> Self
    where T: Sized {
        Self {
            locked: AtomicBool::new(false),
            value: SyncUnsafeCell::new(value),
        }
    }

    #[inline]
    pub fn lock(&self) -> SpinMutexGuard<'_, T> {
        loop {
            match self.locked.compare_exchange_weak(false, true, Acquire, Relaxed) {
                Ok(..) => {
                    break SpinMutexGuard {
                        locked: &self.locked,
                        value: unsafe { self.value.get().as_mut_unchecked() },
                    }
                }
                Err(..) => spin_loop(),
            }
        }
    }

    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

#[derive(Debug)]
pub struct SpinMutexGuard<'a, T: ?Sized> {
    locked: &'a AtomicBool,
    value: &'a mut T,
}

impl<T: ?Sized> Deref for SpinMutexGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<T: ?Sized> DerefMut for SpinMutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<T: ?Sized> Drop for SpinMutexGuard<'_, T> {
    fn drop(&mut self) {
        cfg_select! {
            debug_assertions => {
                if !self.locked.swap(false, Release) {
                    unreachable!("Spin-mutex incorrectly unlocked!")
                }
            }
            not(debug_assertions) => {
                self.locked.store(false, Release);
            }
        }
    }
}
