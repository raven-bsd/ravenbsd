// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux syscall dispatch: number lookup, handlers, `-ENOSYS` and counters.
//!
//! The dispatcher is architecture-specific. [`Dispatcher::register`] resolves a syscall name
//! through [`raven_linux_abi::sysnums`] and stores the handler under the architecture's number;
//! the table is a [`alloc::collections::BTreeMap`], so lookup is `O(log n)` and insertion order
//! does not affect reports.
//!
//! Counters are plain `u64` and the dispatcher is single-threaded by design. A thread-safe
//! wrapper (or per-CPU counters) is future work; callers that share a dispatcher must provide
//! their own synchronisation.
//!
//! Unregistered syscalls return `-ENOSYS` and are counted in
//! [`Dispatcher::unimplemented_hits`], with the syscall name when the architecture's table knows
//! the number.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;

pub use raven_abi::errno::Errno;
pub use raven_linux_abi::Arch;

/// Linux syscall arguments as captured by the trap entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyscallArgs {
    /// Architecture-specific syscall number.
    pub nr: usize,
    /// Up to six raw argument words.
    pub a: [usize; 6],
}

impl SyscallArgs {
    /// Argument `i` as a Linux `int`: the low 32 bits, sign-extended.
    pub fn int(&self, i: usize) -> i32 {
        self.a.get(i).copied().unwrap_or(0) as u32 as i32
    }

    /// Argument `i` as a Linux `long`/`ssize_t`: pointer-sized, sign-extended to 64 bits.
    pub fn long(&self, i: usize) -> i64 {
        self.a.get(i).copied().unwrap_or(0) as isize as i64
    }

    /// Argument `i` as a user pointer-sized value.
    pub fn ptr(&self, i: usize) -> usize {
        self.a.get(i).copied().unwrap_or(0)
    }

    /// Argument `i` as a file descriptor (`int`).
    pub fn fd(&self, i: usize) -> i32 {
        self.int(i)
    }

    /// Argument `i` as a size (`size_t`).
    pub fn size(&self, i: usize) -> usize {
        self.a.get(i).copied().unwrap_or(0)
    }
}

/// Per-process state passed to handlers.
///
/// Handlers downcast this to their concrete context type through
/// [`LinuxContext::as_any_mut`].
pub trait LinuxContext {
    /// Return the concrete context for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// A syscall implementation.
pub type Handler = fn(&mut dyn LinuxContext, &SyscallArgs) -> isize;

/// Failure to register a handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterError {
    /// The architecture's syscall table has no such name.
    UnknownName,
    /// A handler for that syscall number is already registered.
    Duplicate,
}

impl fmt::Display for RegisterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownName => formatter.write_str("unknown syscall name"),
            Self::Duplicate => formatter.write_str("syscall is already registered"),
        }
    }
}

impl core::error::Error for RegisterError {}

struct Entry {
    name: &'static str,
    handler: Handler,
    calls: u64,
    errors: u64,
}

/// Architecture-specific syscall dispatcher.
pub struct Dispatcher {
    arch: Arch,
    entries: BTreeMap<usize, Entry>,
    unimplemented: BTreeMap<usize, (Option<&'static str>, u64)>,
}

impl Dispatcher {
    /// Create an empty dispatcher for `arch`.
    pub fn new(arch: Arch) -> Self {
        Self {
            arch,
            entries: BTreeMap::new(),
            unimplemented: BTreeMap::new(),
        }
    }

    /// Register `handler` under the architecture-specific number for `name`.
    pub fn register(&mut self, name: &str, handler: Handler) -> Result<(), RegisterError> {
        let number =
            raven_linux_abi::sysnums::number(self.arch, name).ok_or(RegisterError::UnknownName)?;
        let canonical =
            raven_linux_abi::sysnums::name(self.arch, number).ok_or(RegisterError::UnknownName)?;
        let number = number as usize;
        if self.entries.contains_key(&number) {
            return Err(RegisterError::Duplicate);
        }
        self.entries.insert(
            number,
            Entry {
                name: canonical,
                handler,
                calls: 0,
                errors: 0,
            },
        );
        Ok(())
    }

    /// Dispatch one syscall and count it.
    ///
    /// Registered handlers receive `ctx` and `args`; their return value is passed through
    /// unchanged and a negative value increments the error counter. Unregistered numbers return
    /// `-ENOSYS` and increment the matching unimplemented hit.
    pub fn dispatch(&mut self, ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
        if let Some(entry) = self.entries.get_mut(&args.nr) {
            entry.calls = entry.calls.saturating_add(1);
            let result = (entry.handler)(ctx, args);
            if result < 0 {
                entry.errors = entry.errors.saturating_add(1);
            }
            return result;
        }

        let name = self.name_for(args.nr);
        let hit = self.unimplemented.entry(args.nr).or_insert((name, 0));
        hit.1 = hit.1.saturating_add(1);
        -(Errno::ENOSYS.0 as isize)
    }

    /// Implemented syscall names, sorted.
    pub fn implemented_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self.entries.values().map(|entry| entry.name).collect();
        names.sort_unstable();
        names
    }

    /// Per-handler call and error counters, sorted by syscall name.
    pub fn stats(&self) -> Vec<(&'static str, u64, u64)> {
        let mut stats: Vec<(&'static str, u64, u64)> = self
            .entries
            .values()
            .map(|entry| (entry.name, entry.calls, entry.errors))
            .collect();
        stats.sort_unstable_by_key(|(name, _, _)| *name);
        stats
    }

    /// Unimplemented hits by number, with the name when known, sorted by number.
    pub fn unimplemented_hits(&self) -> Vec<(usize, Option<&'static str>, u64)> {
        self.unimplemented
            .iter()
            .map(|(&number, &(name, calls))| (number, name, calls))
            .collect()
    }

    fn name_for(&self, number: usize) -> Option<&'static str> {
        u32::try_from(number)
            .ok()
            .and_then(|number| raven_linux_abi::sysnums::name(self.arch, number))
    }
}

/// Convert a handler result into the Linux syscall return convention.
///
/// `Ok(n)` is returned as `n`; `Err(errno)` becomes `-errno`. A success value above
/// `isize::MAX` is a handler bug: it trips `debug_assert!` in debug builds and is clamped to
/// `-EOVERFLOW` in release builds.
pub fn ret(result: Result<usize, Errno>) -> isize {
    match result {
        Ok(value) => {
            debug_assert!(
                value <= isize::MAX as usize,
                "syscall success value exceeds isize::MAX"
            );
            if value > isize::MAX as usize {
                -(Errno::EOVERFLOW.0 as isize)
            } else {
                value as isize
            }
        }
        Err(error) => -(error.0 as isize),
    }
}
