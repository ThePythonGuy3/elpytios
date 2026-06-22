//! Implementation of the `thread_local` macro.
//!
//! There are three different thread-local implementations:
//! * Some targets lack threading support, and hence have only one thread, so the TLS data is stored
//!   in a normal `static`.
//! * Some targets support TLS natively via the dynamic linker and C runtime.
//! * On some targets, the OS provides a library-based TLS implementation. The TLS data is
//!   heap-allocated and referenced using a TLS key.
//!
//! Each implementation provides a macro which generates the `LocalKey` `const`
//! used to reference the TLS variable, along with the necessary helper structs
//! to track the initialization/destruction state of the variable.
//!
//! Additionally, this module contains abstractions for the OS interfaces used
//! for these implementations.

#![cfg_attr(test, allow(unused))]
#![doc(hidden)]
#![forbid(unsafe_op_in_unsafe_fn)]
#![unstable(feature = "thread_local_internals", reason = "internal details of the thread_local macro", issue = "none")]

//! On some targets like wasm there's no threads, so no need to generate
//! thread locals and we can instead just use plain statics!

use crate::{
    cell::{Cell, UnsafeCell},
    mem::MaybeUninit,
    ptr,
};

#[doc(hidden)]
#[allow_internal_unstable(thread_local_internals)]
#[allow_internal_unsafe]
#[unstable(feature = "thread_local_internals", issue = "none")]
#[rustc_macro_transparency = "semiopaque"]
pub macro thread_local_inner {
    // used to generate the `LocalKey` value for const-initialized thread locals
    (@key $t:ty, $(#[$align_attr:meta])*, const $init:expr) => {{
        const __RUST_STD_INTERNAL_INIT: $t = $init;

        // NOTE: Please update the shadowing test in `tests/thread.rs` if these types are renamed.
        unsafe {
            $crate::thread::LocalKey::new(|_| {
                $(#[$align_attr])*
                static __RUST_STD_INTERNAL_VAL: $crate::thread::local_impl::EagerStorage<$t> =
                    $crate::thread::local_impl::EagerStorage { value: __RUST_STD_INTERNAL_INIT };
                &__RUST_STD_INTERNAL_VAL.value
            })
        }
    }},

    // used to generate the `LocalKey` value for `thread_local!`
    (@key $t:ty, $(#[$align_attr:meta])*, $init:expr) => {{
        #[inline]
        fn __rust_std_internal_init_fn() -> $t { $init }

        unsafe {
            $crate::thread::LocalKey::new(|__rust_std_internal_init| {
                $(#[$align_attr])*
                static __RUST_STD_INTERNAL_VAL: $crate::thread::local_impl::LazyStorage<$t> = $crate::thread::local_impl::LazyStorage::new();
                __RUST_STD_INTERNAL_VAL.get(__rust_std_internal_init, __rust_std_internal_init_fn)
            })
        }
    }},
}

#[allow(missing_debug_implementations)]
#[repr(transparent)] // Required for correctness of `#[rustc_align_static]`
pub struct EagerStorage<T> {
    pub value: T,
}

// SAFETY: the target doesn't have threads.
unsafe impl<T> Sync for EagerStorage<T> {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    Alive,
    Destroying,
}

#[allow(missing_debug_implementations)]
#[repr(C)]
pub struct LazyStorage<T> {
    // This field must be first, for correctness of `#[rustc_align_static]`
    value: UnsafeCell<MaybeUninit<T>>,
    state: Cell<State>,
}

impl<T> LazyStorage<T> {
    pub const fn new() -> LazyStorage<T> {
        LazyStorage {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            state: Cell::new(State::Initial),
        }
    }

    /// Gets a pointer to the TLS value, potentially initializing it with the
    /// provided parameters.
    ///
    /// The resulting pointer may not be used after reentrant inialialization
    /// has occurred.
    #[inline]
    pub fn get(&'static self, i: Option<&mut Option<T>>, f: impl FnOnce() -> T) -> *const T {
        if self.state.get() == State::Alive { self.value.get() as *const T } else { self.initialize(i, f) }
    }

    #[cold]
    fn initialize(&'static self, i: Option<&mut Option<T>>, f: impl FnOnce() -> T) -> *const T {
        let value = i.and_then(Option::take).unwrap_or_else(f);

        // Destroy the old value if it is initialized
        // FIXME(#110897): maybe panic on recursive initialization.
        if self.state.get() == State::Alive {
            self.state.set(State::Destroying);
            // Safety: we check for no initialization during drop below
            unsafe {
                ptr::drop_in_place(self.value.get() as *mut T);
            }
            self.state.set(State::Initial);
        }

        // Guard against initialization during drop
        if self.state.get() == State::Destroying {
            panic!("Attempted to initialize thread-local while it is being dropped");
        }

        unsafe {
            self.value.get().write(MaybeUninit::new(value));
        }
        self.state.set(State::Alive);

        self.value.get() as *const T
    }
}

// SAFETY: the target doesn't have threads.
unsafe impl<T> Sync for LazyStorage<T> {}

#[rustc_macro_transparency = "semiopaque"]
pub(crate) macro local_pointer {
    () => {},
    ($vis:vis static $name:ident; $($rest:tt)*) => {
        $vis static $name: $crate::sys::thread_local::LocalPointer = $crate::sys::thread_local::LocalPointer::__new();
        $crate::sys::thread_local::local_pointer! { $($rest)* }
    },
}

pub(crate) struct LocalPointer {
    p: Cell<*mut ()>,
}

impl LocalPointer {
    pub const fn __new() -> LocalPointer {
        LocalPointer {
            p: Cell::new(ptr::null_mut()),
        }
    }

    pub fn get(&self) -> *mut () {
        self.p.get()
    }

    pub fn set(&self, p: *mut ()) {
        self.p.set(p)
    }
}

// SAFETY: the target doesn't have threads.
unsafe impl Sync for LocalPointer {}

/// The native TLS implementation needs a way to register destructors for its data.
/// This module contains platform-specific implementations of that register.
///
/// It turns out however that most platforms don't have a way to register a
/// destructor for each variable. On these platforms, we keep track of the
/// destructors ourselves and register (through the [`guard`] module) only a
/// single callback that runs all of the destructors in the list.
pub(crate) mod destructors {}

/// This module provides a way to schedule the execution of the destructor list
/// and the [runtime cleanup](crate::rt::thread_cleanup) function. Calling `enable`
/// sets up the current thread to ensure that these functions are called at the right times.
pub(crate) mod guard {
    mod key;
    pub(crate) use key::enable;
}

/// `const`-creatable TLS keys.
///
/// Most OSs without native TLS will provide a library-based way to create TLS
/// storage. For each TLS variable, we create a key, which can then be used to
/// reference an entry in a thread-local table. This then associates each key
/// with a pointer which we can get and set to store our data.
pub(crate) mod key {
    mod racy;
    #[cfg(test)]
    mod tests;
    #[cfg(test)]
    pub(super) use racy::get;
    pub(super) use racy::{Key, LazyKey, set};
    use racy::{create, destroy};
}

/// Run a callback in a scenario which must not unwind (such as a `extern "C"
/// fn` declared in a user crate). If the callback unwinds anyway, then
/// `rtabort` with a message about thread local panicking on drop.
#[inline]
#[allow(dead_code)]
fn abort_on_dtor_unwind(f: impl FnOnce()) {
    // Using a guard like this is lower cost.
    let guard = DtorUnwindGuard;
    f();
    core::mem::forget(guard);

    struct DtorUnwindGuard;
    impl Drop for DtorUnwindGuard {
        #[inline]
        fn drop(&mut self) {
            // This is not terribly descriptive, but it doesn't need to be as we'll
            // already have printed a panic message at this point.
            rtabort!("thread local panicked on drop");
        }
    }
}
