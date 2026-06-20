/// Mitigation for <https://github.com/rust-lang/rust/issues/126600>
///
/// Mitigation is ***NOT*** implemented on this platform, either because this platform
/// is not affected, or because mitigation is not yet implemented for this platform.
#[cfg_attr(any(test, doctest), expect(dead_code))]
pub fn unique_thread_exit() {
    // Mitigation not required on platforms where `exit` is thread-safe.
}

pub fn exit(_code: i32) -> ! {
    crate::intrinsics::abort()
}
