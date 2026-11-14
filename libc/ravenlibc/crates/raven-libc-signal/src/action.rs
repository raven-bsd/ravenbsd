// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/sigaction.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
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

//! The `sigaction` rules of musl `src/signal/sigaction.c`.
//!
//! Kept from the donor: the rejection of 32..34 (`sig-32U < 3`), the `SIGKILL`/`SIGSTOP`
//! refusal, the handler-set tracking (`a_or_l`), the `__eintr_valid_flag` write when a handler is
//! installed without `SA_RESTART`, and the SIGABRT serialisation. The kernel interaction and the
//! `SA_RESTORER`/`__restore_rt` setup are deleted: Raven's kernel only kicks (PG-7) and there is
//! no restorer. `SIGABRT` uses the same table writer lock as every other signal, which is what
//! musl's separate abort lock achieves.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::dispositions::{DispTable, Disposition, SA_RESTART, SA_RESTORER};
use crate::errors::EINVAL_ERR;
use crate::sigset::APP_MASK;
use raven_libc_abi::errno::Errno;

/// `__eintr_valid_flag`: some handler was installed without `SA_RESTART`.
static EINTR_VALID: AtomicU32 = AtomicU32::new(0);

/// Whether an `EINTR` from a blocking call can be real (musl `sigaction.c:42-44`).
#[must_use]
pub fn eintr_valid() -> bool {
    EINTR_VALID.load(Ordering::Acquire) != 0
}

/// Clear the flag (the shell's start-up path; tests).
pub fn clear_eintr_valid() {
    EINTR_VALID.store(0, Ordering::Release);
}

/// Install, replace or query one disposition.
pub fn sigaction(
    table: &DispTable,
    sig: i32,
    new: Option<&Disposition>,
    old: Option<&mut Disposition>,
) -> Result<(), Errno> {
    if !(1..=64).contains(&sig) || (32..=34).contains(&sig) {
        return Err(EINVAL_ERR);
    }
    if new.is_some()
        && (sig == raven_libc_abi::generated::signal::SIGKILL
            || sig == raven_libc_abi::generated::signal::SIGSTOP)
    {
        return Err(EINVAL_ERR);
    }
    if let Some(requested) = new {
        let flags = requested.flags & !SA_RESTORER;
        let disposition = Disposition {
            handler: requested.handler,
            mask: requested.mask & APP_MASK,
            flags,
        };
        if disposition.handler > 1 {
            table.note_handler(sig);
            if flags & SA_RESTART == 0 {
                EINTR_VALID.store(1, Ordering::Release);
            }
        }
        let previous = table.write(sig, disposition);
        if let Some(out) = old {
            *out = previous;
        }
    } else {
        let previous = table.read(sig);
        if let Some(out) = old {
            *out = previous;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "action_ut.rs"]
mod action_ut;
