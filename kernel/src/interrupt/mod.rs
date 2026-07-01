pub use imp::init_interrupts;

cfg_select! {
    target_arch = "x86_64" => {
        mod x86_64;
        use x86_64 as imp;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}

#[inline]
unsafe fn page_fault(
    ptr: *mut (),
    _not_present: bool,
    _caused_by_write: bool,
    _triggered_by_user: bool,
    _overwritten_reserved_bits: bool,
    _instruction_fetch_violation: bool,
) {
    panic!("Page fault at address {ptr:p}");
}
