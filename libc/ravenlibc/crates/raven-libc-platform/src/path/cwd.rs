// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The process-global current working directory.
//!
//! Raven has no kernel cwd. Libc keeps one absolute string per process, initialised from `PWD`
//! when it is absolute and valid and otherwise `/`. `chdir` verifies the target with an
//! `O_DIRECTORY` open before storing it; `fchdir` stores the OFD's remembered absolute path.

use alloc::vec::Vec;
use core::cell::RefCell;

use crate::path::resolve::{DEFAULT_LIMITS, Limits, resolve};

/// Process-global cwd cell. The shell wraps the whole [`crate::Platform`] in its futex lock;
/// the `RefCell` only prevents aliasing inside one thread.
pub struct Cwd {
    value: RefCell<Vec<u8>>,
}

impl Default for Cwd {
    fn default() -> Self {
        Self::new()
    }
}

impl Cwd {
    /// Start at `/`.
    pub fn new() -> Self {
        Self {
            value: RefCell::new(alloc::vec![b'/']),
        }
    }

    /// Start at `PWD` when it is absolute and resolvable, else `/`.
    pub fn from_pwd(pwd: Option<&[u8]>) -> Self {
        let cwd = Self::new();
        if let Some(candidate) = pwd
            && candidate.starts_with(b"/")
            && let Ok(resolved) = resolve(b"/", candidate, false, &DEFAULT_LIMITS)
        {
            *cwd.value.borrow_mut() = resolved.abs;
        }
        cwd
    }

    /// The current absolute path.
    pub fn get(&self) -> Vec<u8> {
        self.value.borrow().clone()
    }

    /// Replace the cwd; `path` must already be absolute and normalised.
    pub fn set(&self, path: &[u8]) {
        *self.value.borrow_mut() = path.to_vec();
    }

    /// Resolve a user path against the cwd.
    pub fn resolve(
        &self,
        path: &[u8],
        empty_ok: bool,
        limits: &Limits,
    ) -> Result<crate::path::resolve::Resolved, crate::path::resolve::PathReason> {
        let cwd = self.value.borrow();
        resolve(&cwd, path, empty_ok, limits)
    }
}
