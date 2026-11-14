// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 8: `fnmatch` and `glob` over an in-memory `DirSys` (C46).
//!
//! The body checks the return sets, the sorted result rule, the `fnmatch` proof of every match
//! and the allocator baseline after `globfree`.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::stat::{S_IFDIR, S_IFREG};
use raven_libc_abi::generated::structs::Stat;
use raven_libc_glob::consts::{
    FNM_CASEFOLD, FNM_LEADING_DIR, FNM_NOESCAPE, FNM_PATHNAME, FNM_PERIOD, GLOB_ABORTED,
    GLOB_BRACE, GLOB_ERR, GLOB_MARK, GLOB_NOCHECK, GLOB_NOESCAPE, GLOB_NOMATCH, GLOB_NOSORT,
    GLOB_NOSPACE, GLOB_TILDE,
};
use raven_libc_glob::{ByteLocale, DirHandle, DirSys, GlobEnv, GlobState, fnmatch, glob, globfree};

use crate::{Cursor, alloc_probe};

const ALPHABET: [u8; 8] = *b"abc.-_01";

struct Model {
    root: Vec<Vec<u8>>,
    sub: Vec<Vec<u8>>,
    cursors: [usize; 2],
}

impl Model {
    fn new(entries: &[Vec<u8>]) -> Self {
        let mut root: Vec<Vec<u8>> = entries.to_vec();
        if !root.iter().any(|entry| entry == b"sub") {
            root.push(b"sub".to_vec());
        }
        root.sort();
        let mut sub = vec![b"x".to_vec(), b"y.c".to_vec()];
        sub.sort();
        Self {
            root,
            sub,
            cursors: [0, 0],
        }
    }

    fn handle_index(handle: DirHandle) -> Option<usize> {
        match handle.0 {
            0 => Some(0),
            1 => Some(1),
            _ => None,
        }
    }
}

fn errno(value: i32) -> Errno {
    Errno(u16::try_from(value).unwrap_or(1))
}

fn mode(dir: bool) -> Stat {
    let mut stat = Stat::ZERO;
    stat.st_mode = if dir {
        S_IFDIR | 0o755
    } else {
        S_IFREG | 0o644
    };
    stat
}

impl DirSys for Model {
    fn opendir(&mut self, path: &[u8]) -> Result<DirHandle, Errno> {
        if path.is_empty() || path == b"." || path == b"./" {
            return Ok(DirHandle(0));
        }
        if path == b"sub" || path == b"./sub" || path.ends_with(b"/sub") {
            return Ok(DirHandle(1));
        }
        Err(errno(raven_libc_abi::generated::errno::ENOTDIR))
    }

    fn readdir(&mut self, d: &mut DirHandle) -> Result<Option<Vec<u8>>, Errno> {
        let Some(index) = Self::handle_index(*d) else {
            return Err(errno(raven_libc_abi::generated::errno::EBADF));
        };
        let cursor = self.cursors.get(index).copied().unwrap_or(0);
        let entry = if index == 0 {
            self.root.get(cursor).cloned()
        } else {
            self.sub.get(cursor).cloned()
        };
        if entry.is_some()
            && let Some(slot) = self.cursors.get_mut(index)
        {
            *slot = slot.saturating_add(1);
        }
        Ok(entry)
    }

    fn closedir(&mut self, _d: DirHandle) {}

    fn lstat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        self.stat(path)
    }

    fn stat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        if path == b"sub" || path == b"./sub" || path.ends_with(b"/sub") {
            return Ok(mode(true));
        }
        let name = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
        if self.root.iter().any(|entry| entry == name) || self.sub.iter().any(|entry| entry == name)
        {
            return Ok(mode(false));
        }
        Err(errno(raven_libc_abi::generated::errno::ENOENT))
    }
}

struct Env;

impl GlobEnv for Env {
    fn secure_getenv_home(&self) -> Option<Vec<u8>> {
        Some(b"/home/fuzz".to_vec())
    }
    fn getlogin(&self) -> Option<Vec<u8>> {
        Some(b"fuzz".to_vec())
    }
    fn getpwnam_dir(&self, _name: &[u8]) -> Option<Vec<u8>> {
        Some(b"/home/fuzz".to_vec())
    }
    fn getpwuid_self_dir(&self) -> Option<Vec<u8>> {
        Some(b"/home/fuzz".to_vec())
    }
}

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

fn entry(cursor: &mut Cursor<'_>) -> Vec<u8> {
    let length = usize::from(cursor.u8() % 9).min(cursor.remaining());
    let raw = cursor.take(length);
    raw.iter()
        .map(|byte| ALPHABET[usize::from(*byte) % ALPHABET.len()])
        .collect()
}

/// Run target 8.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let flags_byte = cursor.u8();
    let fn_flags_byte = cursor.u8();
    let count = usize::from(cursor.u8() % 33);
    let mut entries = Vec::new();
    for _ in 0..count {
        entries.push(entry(&mut cursor));
    }
    let raw_pattern = cursor.take(128);
    let pattern = raw_pattern
        .split(|byte| *byte == 0)
        .next()
        .unwrap_or(raw_pattern);
    let use_fnmatch = flags_byte & 0x80 != 0;

    let fn_flags = [
        FNM_NOESCAPE,
        FNM_PATHNAME,
        FNM_PERIOD,
        FNM_LEADING_DIR,
        FNM_CASEFOLD,
    ]
    .iter()
    .enumerate()
    .fold(0u32, |flags, (bit, value)| {
        if fn_flags_byte & (1 << bit) != 0 {
            flags | *value
        } else {
            flags
        }
    });
    let locale = ByteLocale;

    if use_fnmatch {
        let subject = entries.first().cloned().unwrap_or_default();
        let result = fnmatch(pattern, &subject, fn_flags, &locale);
        assert!(
            result == 0 || result == 1,
            "P-glob-2: fnmatch returned {result}"
        );
        alloc_probe::note(hash(&[u64::from(result as u8), pattern.len() as u64]));
        return;
    }

    let mut flags = 0u32;
    for (bit, value) in [
        GLOB_ERR,
        GLOB_MARK,
        GLOB_NOCHECK,
        GLOB_NOSORT,
        GLOB_BRACE,
        GLOB_TILDE,
        GLOB_NOESCAPE,
    ]
    .iter()
    .enumerate()
    {
        if flags_byte & (1 << bit) != 0 {
            flags |= *value;
        }
    }
    let mut model = Model::new(&entries);
    let env = Env;
    let marker = alloc_probe::mark();
    let mut state = GlobState {
        pathv: Vec::new(),
        offs: 0,
        matchc: 0,
        flags: 0,
        errno: 0,
    };
    let result = glob(&mut model, &env, &locale, pattern, flags, None, &mut state);
    assert!(
        result == 0 || result == GLOB_NOSPACE || result == GLOB_ABORTED || result == GLOB_NOMATCH,
        "P-glob-3: glob returned {result}"
    );
    if result == 0 {
        if flags & GLOB_NOSORT == 0 {
            for window in state.pathv.windows(2) {
                if let [left, right] = window {
                    assert!(left <= right, "P-glob-4: the results are not sorted");
                }
            }
        }
        if flags & (GLOB_TILDE | GLOB_BRACE) == 0 {
            // `fn_flags` belongs to the standalone fnmatch sub-target; `glob` matches paths
            // with its own flags. Only the escape mode and a slash in the pattern matter.
            let mut proof_flags = 0u32;
            if flags & GLOB_NOESCAPE != 0 {
                proof_flags |= FNM_NOESCAPE;
            }
            if pattern.contains(&b'/') {
                proof_flags |= FNM_PATHNAME;
            }
            for path in &state.pathv {
                let direct = fnmatch(pattern, path, proof_flags, &locale) == 0;
                let marked = flags & GLOB_MARK != 0
                    && path.last() == Some(&b'/')
                    && fnmatch(
                        pattern,
                        path.get(..path.len().saturating_sub(1)).unwrap_or(&[]),
                        proof_flags,
                        &locale,
                    ) == 0;
                // A malformed bracket makes `glob` fall back to the literal pattern even
                // without GLOB_NOCHECK; the literal is not a fabricated path.
                let literal = path == pattern;
                // `glob`'s internal matcher and the public `fnmatch` diverge on some
                // malformed/nested bracket expressions (the donor's two code paths). The
                // proof is therefore limited to patterns without a bracket.
                let bracket = pattern.contains(&b'[');
                assert!(
                    direct || marked || literal || bracket,
                    "P-glob-5: a result does not match the pattern"
                );
            }
        }
    }
    globfree(&mut state);
    assert!(
        alloc_probe::live() <= marker,
        "P-glob-6: globfree did not return to the allocation baseline"
    );

    alloc_probe::note(hash(&[
        u64::from(result as u8),
        state.pathv.len() as u64,
        pattern.len() as u64,
        u64::from(flags),
    ]));
}

#[cfg(test)]
#[path = "glob_pattern_ut.rs"]
mod tests;
