// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Names that resolve inside libc instead of going to `vfsd`.
//!
//! `/dev/fd/N`, `/proc/self/fd/N`, `/dev/stdin`, `/dev/stdout` and `/dev/stderr` are duplicates
//! of an existing descriptor: the new fd shares the OFD, which is what shell process substitution
//! expects. Everything else under `/dev` and `/proc` is a normal `vfsd` path.

/// A special name that refers to an existing descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialName {
    /// `/dev/fd/N` or `/proc/self/fd/N`.
    Fd(i32),
    /// `/dev/stdin`.
    Stdin,
    /// `/dev/stdout`.
    Stdout,
    /// `/dev/stderr`.
    Stderr,
}

/// Parse a special path, returning `None` for everything else.
pub fn special_fd(path: &[u8]) -> Option<SpecialName> {
    match path {
        b"/dev/stdin" => Some(SpecialName::Stdin),
        b"/dev/stdout" => Some(SpecialName::Stdout),
        b"/dev/stderr" => Some(SpecialName::Stderr),
        _ => {
            let rest = path
                .strip_prefix(b"/dev/fd/")
                .or_else(|| path.strip_prefix(b"/proc/self/fd/"))?;
            core::str::from_utf8(rest)
                .ok()?
                .parse::<i32>()
                .ok()
                .map(SpecialName::Fd)
        }
    }
}

/// The numeric descriptor a special name stands for.
pub const fn target_fd(name: SpecialName) -> i32 {
    match name {
        SpecialName::Fd(fd) => fd,
        SpecialName::Stdin => 0,
        SpecialName::Stdout => 1,
        SpecialName::Stderr => 2,
    }
}
