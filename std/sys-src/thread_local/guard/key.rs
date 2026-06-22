//! A lot of UNIX platforms don't have a specialized way to register TLS
//! destructors for native TLS. Instead, we use one TLS key with a destructor
//! that will run all native TLS destructors in the destructor list.

use crate::{
    ptr,
    sys::thread_local::key::{LazyKey, set},
};

pub fn enable() {
    const DEFER: *mut u8 = ptr::without_provenance_mut(1);
    const RUN: *mut u8 = ptr::without_provenance_mut(2);

    static CLEANUP: LazyKey = LazyKey::new(Some(run));

    unsafe { set(CLEANUP.force(), DEFER) }

    unsafe extern "C" fn run(state: *mut u8) {
        if state == DEFER {
            // Make sure that this function is run again in the next round of
            // TLS destruction. If there is no further round, there will be leaks,
            // but that's okay, `thread_cleanup` is not guaranteed to be called.
            unsafe { set(CLEANUP.force(), RUN) }
        } else {
            debug_assert_eq!(state, RUN);
            // If the state is still RUN in the next round of TLS destruction,
            // it means that no other TLS destructors defined by this runtime
            // have been run, as they would have set the state to DEFER.
            crate::rt::thread_cleanup();
        }
    }
}
