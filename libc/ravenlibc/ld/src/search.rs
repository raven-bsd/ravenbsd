// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Search paths, the library-name alias table and the `$ORIGIN` rules.

/// The ring-2 aliases that all resolve to `libc.so.1`.
pub const ALIASES: [&[u8]; 9] = [
    b"libm.so.1",
    b"libm.so.6",
    b"libpthread.so.0",
    b"libdl.so.2",
    b"librt.so.1",
    b"libutil.so.1",
    b"libresolv.so.2",
    b"libcrypt.so.1",
    b"libexecinfo.so.1",
];

/// The default search path.
pub const DEFAULT_DIRS: [&[u8]; 3] = [b"/lib", b"/usr/lib", b"/usr/local/lib"];

/// Map a requested name through the ring-2 alias table.
#[must_use]
pub fn canonical(name: &[u8]) -> &[u8] {
    if ALIASES.contains(&name) {
        b"libc.so.1"
    } else {
        name
    }
}

/// Join one directory and a name into `out`; returns the length.
pub fn join(dir: &[u8], name: &[u8], out: &mut [u8]) -> Option<usize> {
    if dir.is_empty() {
        return None;
    }
    let total = dir.len().checked_add(1)?.checked_add(name.len())?;
    let target = out.get_mut(..total)?;
    let head = target.get_mut(..dir.len())?;
    head.copy_from_slice(dir);
    let slash = target.get_mut(dir.len())?;
    *slash = b'/';
    let start = dir.len().checked_add(1)?;
    target.get_mut(start..)?.copy_from_slice(name);
    Some(total)
}

/// Whether `name` is absolute.
#[must_use]
pub fn is_absolute(name: &[u8]) -> bool {
    name.first() == Some(&b'/')
}

/// Whether `name` contains a slash.
#[must_use]
pub fn has_slash(name: &[u8]) -> bool {
    name.contains(&b'/')
}

/// Whether `name` contains the `$ORIGIN` token.
#[must_use]
pub fn has_origin(name: &[u8]) -> bool {
    name.windows(7).any(|window| window == b"$ORIGIN")
}

#[cfg(test)]
#[path = "search_ut.rs"]
mod tests;
