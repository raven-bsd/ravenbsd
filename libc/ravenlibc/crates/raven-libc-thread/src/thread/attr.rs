// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_attr_init.c, pthread_attr_destroy.c, pthread_attr_get.c, pthread_attr_setdetachstate.c, pthread_attr_setguardsize.c, pthread_attr_setinheritsched.c, pthread_attr_setschedparam.c, pthread_attr_setschedpolicy.c, pthread_attr_setscope.c, pthread_attr_setstack.c, pthread_attr_setstacksize.c, default_attr.c, pthread_setattr_default_np.c, pthread_getattr_np.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor files have no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! Thread attributes (`pthread_attr_t`, 56 bytes).
//!
//! The first three 64-bit words are stack size, guard size and stack address; the four `int`
//! words at bytes 24..40 are detach state, inherit-scheduler, policy and priority. All-zero is a
//! valid static initialiser; `init` fills the process defaults.

use crate::kernel::ThreadKernel;
use crate::libc_state::LIBC;
use crate::limits::{
    DEFAULT_GUARD_MAX, DEFAULT_STACK_MAX, EINVAL, ENOTSUP, Errno, PTHREAD_CREATE_DETACHED,
    PTHREAD_CREATE_JOINABLE, PTHREAD_EXPLICIT_SCHED, PTHREAD_INHERIT_SCHED, PTHREAD_SCOPE_PROCESS,
    PTHREAD_SCOPE_SYSTEM, PTHREAD_STACK_MIN, SCHED_OTHER,
};
use crate::raw;

/// `pthread_attr_t` (56 bytes).
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct Attr {
    /// The fourteen words of the C object.
    pub words: [u32; 14],
}

const _: () = assert!(core::mem::size_of::<Attr>() == 56);
const _: () = assert!(core::mem::align_of::<Attr>() == 8);

impl Attr {
    /// A zeroed attribute object.
    #[must_use]
    pub const fn new() -> Self {
        Self { words: [0; 14] }
    }

    /// `_a_stacksize`.
    #[must_use]
    pub fn stacksize(&self) -> usize {
        raw::load_usize(&self.words, 0)
    }

    /// `_a_stacksize` setter (no validation).
    pub fn set_stacksize_raw(&mut self, value: usize) {
        raw::store_usize(&mut self.words, 0, value);
    }

    /// `_a_guardsize`.
    #[must_use]
    pub fn guardsize(&self) -> usize {
        raw::load_usize(&self.words, 2)
    }

    /// `_a_stackaddr` (the high address of the application stack, zero for none).
    #[must_use]
    pub fn stackaddr(&self) -> usize {
        raw::load_usize(&self.words, 4)
    }

    /// `_a_detach`.
    #[must_use]
    pub fn detach_state(&self) -> i32 {
        self.words[6] as i32
    }

    /// `_a_sched`.
    #[must_use]
    pub fn inheritsched(&self) -> i32 {
        self.words[7] as i32
    }

    /// `_a_policy`.
    #[must_use]
    pub fn policy(&self) -> i32 {
        self.words[8] as i32
    }

    /// `_a_prio`.
    #[must_use]
    pub fn priority(&self) -> i32 {
        self.words[9] as i32
    }

    /// Set the detach state without validation (create uses the raw accessor).
    pub fn set_detach_raw(&mut self, value: i32) {
        self.words[6] = value as u32;
    }

    /// Whether the attribute was explicitly zeroed (used by `setattr_default_np`).
    #[must_use]
    pub fn only_defaults(&self) -> bool {
        self.words[6..].iter().all(|word| *word == 0)
            && self.stackaddr() == 0
            && self.stacksize() != 0
    }
}

impl Default for Attr {
    fn default() -> Self {
        Self::new()
    }
}

/// `pthread_attr_init`.
pub fn init() -> Attr {
    let mut attr = Attr::new();
    attr.set_stacksize_raw(LIBC.default_stack());
    raw::store_usize(&mut attr.words, 2, LIBC.default_guard());
    attr
}

/// `pthread_attr_destroy`.
pub fn destroy(_attr: &Attr) {}

/// `pthread_attr_setdetachstate`.
///
/// # Errors
///
/// `EINVAL` outside joinable/detached.
pub fn setdetachstate(attr: &mut Attr, state: i32) -> Result<(), Errno> {
    if state != PTHREAD_CREATE_JOINABLE && state != PTHREAD_CREATE_DETACHED {
        return Err(EINVAL);
    }
    attr.set_detach_raw(state);
    Ok(())
}

/// `pthread_attr_getdetachstate`.
#[must_use]
pub fn getdetachstate(attr: &Attr) -> i32 {
    attr.detach_state()
}

/// `pthread_attr_setguardsize`.
///
/// # Errors
///
/// `EINVAL` when the size would overflow an eighth of the address space.
pub fn setguardsize(attr: &mut Attr, size: usize) -> Result<(), Errno> {
    if size > usize::MAX / 8 {
        return Err(EINVAL);
    }
    raw::store_usize(&mut attr.words, 2, size);
    Ok(())
}

/// `pthread_attr_getguardsize`.
#[must_use]
pub fn getguardsize(attr: &Attr) -> usize {
    attr.guardsize()
}

/// `pthread_attr_setstacksize`.
///
/// # Errors
///
/// `EINVAL` below `PTHREAD_STACK_MIN` or above a quarter of the address space.
pub fn setstacksize(attr: &mut Attr, size: usize) -> Result<(), Errno> {
    if !(PTHREAD_STACK_MIN..=usize::MAX / 4).contains(&size) {
        return Err(EINVAL);
    }
    raw::store_usize(&mut attr.words, 4, 0);
    attr.set_stacksize_raw(size);
    Ok(())
}

/// `pthread_attr_getstacksize`.
#[must_use]
pub fn getstacksize(attr: &Attr) -> usize {
    attr.stacksize()
}

/// `pthread_attr_setstack`.
///
/// # Errors
///
/// `EINVAL` for a size below `PTHREAD_STACK_MIN`.
pub fn setstack(attr: &mut Attr, addr: usize, size: usize) -> Result<(), Errno> {
    if size < PTHREAD_STACK_MIN {
        return Err(EINVAL);
    }
    raw::store_usize(&mut attr.words, 4, addr.wrapping_add(size));
    attr.set_stacksize_raw(size);
    Ok(())
}

/// `pthread_attr_getstack`.
///
/// # Errors
///
/// `EINVAL` when no application stack was set.
pub fn getstack(attr: &Attr) -> Result<(usize, usize), Errno> {
    let high = attr.stackaddr();
    if high == 0 {
        return Err(EINVAL);
    }
    let size = attr.stacksize();
    Ok((high.wrapping_sub(size), size))
}

/// `pthread_attr_setinheritsched`.
///
/// # Errors
///
/// `EINVAL` outside inherit/explicit.
pub fn setinheritsched(attr: &mut Attr, inherit: i32) -> Result<(), Errno> {
    if inherit != PTHREAD_INHERIT_SCHED && inherit != PTHREAD_EXPLICIT_SCHED {
        return Err(EINVAL);
    }
    attr.words[7] = inherit as u32;
    Ok(())
}

/// `pthread_attr_getinheritsched`.
#[must_use]
pub fn getinheritsched(attr: &Attr) -> i32 {
    attr.inheritsched()
}

/// `pthread_attr_setschedparam`.
pub fn setschedparam(attr: &mut Attr, priority: i32) {
    attr.words[9] = priority as u32;
}

/// `pthread_attr_getschedparam`.
#[must_use]
pub fn getschedparam(attr: &Attr) -> i32 {
    attr.priority()
}

/// `pthread_attr_setschedpolicy`.
///
/// # Errors
///
/// `ENOTSUP` for anything but `SCHED_OTHER` without PG-11.
pub fn setschedpolicy<K: ThreadKernel>(
    kernel: &K,
    attr: &mut Attr,
    policy: i32,
) -> Result<(), Errno> {
    if policy != SCHED_OTHER && !kernel.caps().sched {
        return Err(ENOTSUP);
    }
    attr.words[8] = policy as u32;
    Ok(())
}

/// `pthread_attr_getschedpolicy`.
#[must_use]
pub fn getschedpolicy(attr: &Attr) -> i32 {
    attr.policy()
}

/// `pthread_attr_setscope`.
///
/// # Errors
///
/// `EINVAL` for an unknown scope, `ENOTSUP` for `PTHREAD_SCOPE_PROCESS`.
pub fn setscope(attr: &mut Attr, scope: i32) -> Result<(), Errno> {
    let _ = attr;
    match scope {
        PTHREAD_SCOPE_SYSTEM => Ok(()),
        PTHREAD_SCOPE_PROCESS => Err(ENOTSUP),
        _ => Err(EINVAL),
    }
}

/// `pthread_attr_getscope`.
#[must_use]
pub fn getscope(_attr: &Attr) -> i32 {
    PTHREAD_SCOPE_SYSTEM
}

/// `pthread_setattr_default_np`.
///
/// # Errors
///
/// `EINVAL` when the attribute carries anything but stack and guard size.
pub fn setattr_default_np(attr: &Attr) -> Result<(), Errno> {
    if !attr.only_defaults() {
        return Err(EINVAL);
    }
    LIBC.set_defaults(
        attr.stacksize().min(DEFAULT_STACK_MAX),
        attr.guardsize().min(DEFAULT_GUARD_MAX),
    );
    Ok(())
}

/// `pthread_getattr_default_np`.
#[must_use]
pub fn getattr_default_np() -> Attr {
    init()
}

/// `pthread_getattr_np`: the main thread's stack bounds come from the TCB (no `mremap` probe).
#[must_use]
pub fn getattr_np(tcb: &raven_libc_tls::Pthread) -> Attr {
    let mut attr = Attr::new();
    attr.set_detach_raw(if tcb.detach_state() >= raven_libc_tls::tcb::DT_DETACHED {
        PTHREAD_CREATE_DETACHED
    } else {
        PTHREAD_CREATE_JOINABLE
    });
    raw::store_usize(&mut attr.words, 2, tcb.guard_size);
    let (base, size) = if tcb.stack_base != 0 {
        (tcb.stack_base, tcb.stack_size)
    } else {
        // OPEN-thr-2: the spawn auxv is not wired yet; report the documented 8 MiB window below
        // the thread pointer instead of probing the mapping (T-thr-20).
        (
            tcb.self_ptr.wrapping_sub(crate::limits::DEFAULT_STACK_SIZE),
            crate::limits::DEFAULT_STACK_SIZE,
        )
    };
    raw::store_usize(&mut attr.words, 4, base.wrapping_add(size));
    attr.set_stacksize_raw(size);
    attr
}

#[cfg(test)]
#[path = "attr_ut.rs"]
mod tests;
