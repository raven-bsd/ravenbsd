// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Link-time helpers: `weak_hook!`, `alias!`, `lazy!` and the `.raven_unimpl` registry.
//!
//! Spike S-1 (see the C54 report) showed that a weak `global_asm!` hook cannot be overridden by
//! a fat-LTO consumer: `rust-lld` rejects the strong definition with `hook changed binding to
//! STB_GLOBAL`. The shipped mechanism is therefore the documented fallback, a static
//! function-pointer cell installed by `__raven_libc_init` through the generated `install`
//! function; the static archive path uses the same cell.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU8, Ordering};

/// A one-time-initialised cell usable in `no_std` static position.
///
/// `get` runs the initialiser exactly once; concurrent callers spin until the value is ready.
/// The initialiser must not panic (panic is forbidden in shipped code).
pub struct Lazy<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<T>>,
}

// SAFETY: `state` serialises publication. A `&Lazy<T>` only hands out `&T`, so `T` must be
// `Sync`; the winning thread moves `T` in, so `T` must also be `Send`.
unsafe impl<T: Send + Sync> Sync for Lazy<T> {}

const UNINIT: u8 = 0;
const INITIALISING: u8 = 1;
const READY: u8 = 2;

impl<T> Lazy<T> {
    /// A new uninitialised cell.
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(UNINIT),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Return the value, initialising it with `init` on the first call.
    pub fn get(&self, init: impl FnOnce() -> T) -> &T {
        if self.state.load(Ordering::Acquire) == READY {
            // SAFETY: `READY` is published with Release after the value was written.
            return unsafe { (*self.value.get()).assume_init_ref() };
        }
        if self
            .state
            .compare_exchange(UNINIT, INITIALISING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            // SAFETY: this thread won the initialisation race and has exclusive access to the
            // uninitialised slot.
            unsafe {
                (*self.value.get()).write(init());
            }
            self.state.store(READY, Ordering::Release);
        } else {
            while self.state.load(Ordering::Acquire) != READY {
                core::hint::spin_loop();
            }
        }
        // SAFETY: `READY` is published with Release after the value was written.
        unsafe { (*self.value.get()).assume_init_ref() }
    }
}

impl<T> Default for Lazy<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Define a hook with a static function-pointer cell and a dummy fallback.
///
/// The generated function calls the installed hook, or the dummy when no hook is installed.
/// `__raven_libc_init` (C57/C61) calls `<name>::install(Some(strong))`; a static archive that
/// never installs the hook keeps the dummy and does not pull the strong member.
#[macro_export]
macro_rules! weak_hook {
    (
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty;
        dummy = $dummy:path;
    ) => {
        $(#[$meta])*
        $vis fn $name($($arg: $ty),*) -> $ret {
            $name::call($($arg),*)
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        $vis mod $name {
            /// The installed hook signature.
            pub type Hook = unsafe fn($($ty),*) -> $ret;

            static SLOT: core::sync::atomic::AtomicUsize =
                core::sync::atomic::AtomicUsize::new(0);

            /// Install the strong hook, or clear the slot with `None`.
            pub fn install(hook: Option<Hook>) {
                SLOT.store(
                    hook.map_or(0, |hook| hook as usize),
                    core::sync::atomic::Ordering::Release,
                );
            }

            /// Call the installed hook or the dummy.
            pub fn call($($arg: $ty),*) -> $ret {
                let slot = SLOT.load(core::sync::atomic::Ordering::Acquire);
                if slot == 0 {
                    return ($dummy)($($arg),*);
                }
                // SAFETY: only `install` stores a non-zero value, and its `Hook` contract
                // requires a function with this exact signature that outlives the process.
                let hook: Hook = unsafe { core::mem::transmute(slot) };
                // SAFETY: see above; the argument types are the generated signature.
                unsafe { hook($($arg),*) }
            }
        }
    };
}

/// Define a public alias for another item.
#[macro_export]
macro_rules! alias {
    ($(#[$meta:meta])* $vis:vis $alias:ident = $target:path;) => {
        $(#[$meta])*
        $vis use $target as $alias;
    };
}

/// Define a lazily initialised module-local value: `NAME::get()`.
#[macro_export]
macro_rules! lazy {
    (
        $(#[$meta:meta])*
        $vis:vis static $name:ident: $ty:ty = $init:expr;
    ) => {
        $(#[$meta])*
        #[allow(non_snake_case)]
        $vis mod $name {
            #[allow(unused_imports)]
            use super::*;

            static CELL: $crate::macros::Lazy<$ty> = $crate::macros::Lazy::new();

            /// Return the value, initialising it on first use.
            pub fn get() -> &'static $ty {
                CELL.get(|| $init)
            }
        }
    };
}

/// Define an exported stub that sets `errno` and returns `-1`.
///
/// The stub also places a [`crate::unimpl::UnimplRecord`] in its own `.raven_unimpl` section so
/// `xtask libc-pack` (C66) can generate `<raven/stubs.h>`.
#[macro_export]
macro_rules! unimplemented_symbol {
    ($name:ident, gap = $gap:literal, errno = $errno:expr) => {
        $crate::unimplemented_symbol!(
            $name,
            fn $name() -> i32,
            gap = $gap,
            errno = $errno
        );
    };
    (
        $name:ident,
        fn $fname:ident($($arg:ident: $ty:ty),* $(,)?) -> i32,
        gap = $gap:literal,
        errno = $errno:expr
    ) => {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        pub mod $name {
            /// Packed metadata read by `xtask libc-pack`.
            #[used]
            #[unsafe(link_section = ".raven_unimpl")]
            pub static RECORD: $crate::unimpl::UnimplRecord =
                $crate::unimpl::UnimplRecord::new(stringify!($name), $gap, $errno);
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn $fname($($arg: $ty),*) -> i32 {
            let _ = &$name::RECORD;
            $(let _ = &$arg;)*
            $crate::unimpl::fail($errno)
        }
    };
}

#[cfg(test)]
#[path = "macros_ut.rs"]
mod tests;
