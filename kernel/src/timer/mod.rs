use core::time::Duration;

cfg_select! {
    target_arch = "x86_64" => {
        mod x86_64;
        use x86_64 as imp;
        pub use imp::Timer as TimerX86_64;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}

#[repr(transparent)]
pub struct Timer {
    pub inner: imp::Timer,
}

impl !Send for imp::Timer {}
impl !Sync for imp::Timer {}

impl Timer {
    #[inline]
    pub fn new() -> Self {
        Self { inner: imp::Timer::new() }
    }

    #[inline]
    pub fn busy_wait(&self, duration: Duration) {
        self.inner.busy_wait(duration);
    }

    /// Schedules a timer interrupt.
    ///
    /// # Notes
    /// Passing the function to be called by the interrupt is platform-specific.
    #[inline]
    pub fn schedule(&self, duration: Duration) {
        self.inner.schedule(duration);
    }
}
