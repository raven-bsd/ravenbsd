// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `MALLOC_OPTIONS` letter parser shared by both backends (OpenBSD `malloc.3` letters).

/// The runtime-tunable subset of OpenBSD's `malloc_readonly`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Number of pools, 2..=32.
    pub malloc_mutexes: usize,
    /// Junk level, 0..=2.
    pub def_malloc_junk: u8,
    /// Page-cache slots, 0..=256.
    pub def_maxcache: usize,
    /// Chunk canaries.
    pub chunk_canaries: bool,
    /// Extensive double-free check.
    pub malloc_freecheck: bool,
    /// `PROT_NONE` freed pages.
    pub malloc_freeunmap: bool,
    /// Always move on `realloc`.
    pub malloc_realloc: bool,
    /// Abort instead of returning `NULL`.
    pub malloc_xmalloc: bool,
    /// Guard bytes after a page allocation.
    pub malloc_guard: usize,
    /// Native `recallocarray`/`freezero` paths.
    pub internal_funcs: bool,
    /// Verbose diagnostics.
    pub malloc_verbose: bool,
    /// Statistics (accepted and ignored; this port has no `MALLOC_STATS`).
    pub malloc_stats: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            malloc_mutexes: 8,
            def_malloc_junk: 1,
            def_maxcache: 64,
            chunk_canaries: false,
            malloc_freecheck: false,
            malloc_freeunmap: false,
            malloc_realloc: false,
            malloc_xmalloc: false,
            malloc_guard: 0,
            internal_funcs: true,
            malloc_verbose: false,
            malloc_stats: false,
        }
    }
}

impl Options {
    /// Apply one letter. Returns `false` for an unknown letter.
    pub fn apply(&mut self, letter: u8) -> bool {
        match letter {
            b'+' => {
                self.malloc_mutexes = self.malloc_mutexes.saturating_mul(2).min(32);
            }
            b'-' => {
                self.malloc_mutexes = (self.malloc_mutexes / 2).max(2);
            }
            b'>' => {
                self.def_maxcache = self.def_maxcache.saturating_mul(2).min(256);
            }
            b'<' => {
                self.def_maxcache /= 2;
            }
            b'c' => self.chunk_canaries = false,
            b'C' => self.chunk_canaries = true,
            b'f' => {
                self.malloc_freecheck = false;
                self.malloc_freeunmap = false;
            }
            b'F' => {
                self.malloc_freecheck = true;
                self.malloc_freeunmap = true;
            }
            b'g' => self.malloc_guard = 0,
            b'G' => self.malloc_guard = crate::consts::PAGE_SIZE,
            b'j' => self.def_malloc_junk = self.def_malloc_junk.saturating_sub(1),
            b'J' => self.def_malloc_junk = self.def_malloc_junk.saturating_add(1).min(2),
            b'r' => self.malloc_realloc = false,
            b'R' => self.malloc_realloc = true,
            b'u' => self.malloc_freeunmap = false,
            b'U' => self.malloc_freeunmap = true,
            b'x' => self.malloc_xmalloc = false,
            b'X' => self.malloc_xmalloc = true,
            b'd' => self.malloc_stats = false,
            b'D' | b'1' | b'2' | b'3' | b'4' => self.malloc_stats = true,
            b'v' => self.malloc_verbose = false,
            b'V' => self.malloc_verbose = true,
            _ => return false,
        }
        true
    }

    /// Apply a whole string; returns the first unknown letter.
    pub fn parse(&mut self, text: &[u8]) -> Option<u8> {
        let mut first_unknown = None;
        let mut index = 0usize;
        while let Some(&letter) = text.get(index) {
            match letter {
                b'S' => {
                    for setting in b"CFGJ" {
                        let _ = self.apply(*setting);
                    }
                    self.def_maxcache = 0;
                }
                b's' => {
                    for setting in b"cfgj" {
                        let _ = self.apply(*setting);
                    }
                    self.def_maxcache = 64;
                }
                _ => {
                    if !self.apply(letter) && first_unknown.is_none() {
                        first_unknown = Some(letter);
                    }
                }
            }
            index = index.wrapping_add(1);
        }
        first_unknown
    }
}

#[cfg(test)]
#[path = "options_ut.rs"]
mod tests;
