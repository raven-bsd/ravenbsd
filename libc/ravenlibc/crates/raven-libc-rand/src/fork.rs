// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The libc-internal fork hooks (doc 15 3.A.1.1).

/// Hooks called by the process layer around `fork`.
pub trait ForkEvents {
    /// Before the fork: the shell's lock belongs to the caller; this hook is a no-op here.
    fn prefork(&mut self);
    /// In the parent: no state change.
    fn postfork_parent(&mut self);
    /// In the child: wipe the whole generator so the next call reseeds.
    fn postfork_child(&mut self);
}

#[cfg(test)]
#[path = "fork_ut.rs"]
mod tests;
