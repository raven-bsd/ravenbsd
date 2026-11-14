// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The hostile `FmtEnv` of targets 1 and 2.
//!
//! The environment is deliberately unfriendly: it may return the largest legal length every
//! time, records any request outside its fixed 256-byte arena, caps the store log at 64 entries
//! and fails every `alloc_store` after the first four calls. A friendly environment would hide
//! the out-of-bounds-length class (C43 item 7).

use raven_libc_fmt::{FloatKind, FmtEnv, StoreSize, VaReader};

/// Arena size in bytes.
pub const ARENA_LEN: usize = 256;
/// Maximum number of recorded stores.
pub const STORE_LOG_MAX: usize = 64;
/// Allocator successes before it starts failing.
pub const ALLOC_SUCCESSES: u32 = 4;

/// One recorded store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Store {
    /// `%n` or a `scanf` integer.
    Integer {
        /// Target arena offset.
        ptr: usize,
        /// The stored width in bytes.
        size: u8,
        /// The value.
        value: u64,
    },
    /// A byte string.
    Chars {
        /// Target arena offset.
        ptr: usize,
        /// Length without the NUL.
        len: usize,
    },
    /// A wide string.
    Wide {
        /// Target arena offset.
        ptr: usize,
        /// Number of 32-bit units without the NUL.
        len: usize,
    },
    /// A floating-point value.
    Float {
        /// Target arena offset.
        ptr: usize,
    },
    /// An allocated `%m` result.
    Alloc {
        /// Destination pointer.
        ptr: usize,
        /// Byte count including the NUL.
        len: usize,
        /// Whether the allocation succeeded.
        ok: bool,
    },
}

/// A hostile environment over a fixed arena.
#[derive(Clone, Debug)]
pub struct FuzzEnv {
    arena: [u8; ARENA_LEN],
    words: [u64; 16],
    pos: usize,
    stores: Vec<Store>,
    dropped_stores: usize,
    out_of_arena: usize,
    alloc_calls: u32,
}

impl FuzzEnv {
    /// Build from the cursor's argument words and arena bytes.
    pub fn from_cursor(cursor: &mut crate::Cursor<'_>) -> Self {
        let mut arena = [0u8; ARENA_LEN];
        let raw = cursor.take(ARENA_LEN);
        if let Some(target) = arena.get_mut(..raw.len()) {
            target.copy_from_slice(raw);
        }
        let mut words = [0u64; 16];
        for word in &mut words {
            let low = u64::from(cursor.u32());
            let high = u64::from(cursor.u32());
            *word = low | (high << 32);
        }
        Self {
            arena,
            words,
            pos: 0,
            stores: Vec::new(),
            dropped_stores: 0,
            out_of_arena: 0,
            alloc_calls: 0,
        }
    }

    /// The arena bytes.
    #[must_use]
    pub fn arena(&self) -> &[u8; ARENA_LEN] {
        &self.arena
    }

    /// The recorded stores.
    #[must_use]
    pub fn stores(&self) -> &[Store] {
        &self.stores
    }

    /// Number of stores the cap refused.
    #[must_use]
    pub fn dropped_stores(&self) -> usize {
        self.dropped_stores
    }

    /// Number of out-of-arena requests.
    #[must_use]
    pub fn out_of_arena(&self) -> usize {
        self.out_of_arena
    }

    /// Number of `alloc_store` calls.
    #[must_use]
    pub fn alloc_calls(&self) -> u32 {
        self.alloc_calls
    }

    fn push(&mut self, store: Store) {
        if self.stores.len() < STORE_LOG_MAX {
            self.stores.push(store);
        } else {
            self.dropped_stores = self.dropped_stores.saturating_add(1);
        }
    }

    fn note_out_of_arena(&mut self) {
        self.out_of_arena = self.out_of_arena.saturating_add(1);
    }

    fn in_arena(&self, ptr: usize) -> bool {
        ptr < ARENA_LEN
    }
}

impl VaReader for FuzzEnv {
    fn next_u64(&mut self) -> u64 {
        let value = self.words.get(self.pos).copied().unwrap_or(0);
        self.pos = self.pos.saturating_add(1);
        value
    }

    fn next_f64_bits(&mut self) -> u64 {
        self.next_u64()
    }

    fn next_ld_bits(&mut self) -> u128 {
        u128::from(self.next_u64()) | (u128::from(self.next_u64()) << 64)
    }

    fn clone_state(&self) -> Self {
        self.clone()
    }
}

impl FmtEnv for FuzzEnv {
    fn cstr_len(&mut self, ptr: usize, max: usize) -> usize {
        if !self.in_arena(ptr) {
            return 0;
        }
        let limit = max.min(ARENA_LEN.saturating_sub(ptr));
        let mut length = 0usize;
        while length < limit {
            if self.arena.get(ptr.saturating_add(length)).copied() == Some(0) {
                return length;
            }
            length = length.saturating_add(1);
        }
        length
    }

    fn read_bytes(&mut self, ptr: usize, offset: usize, buf: &mut [u8]) -> usize {
        if !self.in_arena(ptr) {
            self.note_out_of_arena();
            return 0;
        }
        let start = ptr.saturating_add(offset);
        if !self.in_arena(start) {
            self.note_out_of_arena();
            return 0;
        }
        let available = ARENA_LEN.saturating_sub(start).min(buf.len());
        if let (Some(source), Some(target)) = (
            self.arena.get(start..start.saturating_add(available)),
            buf.get_mut(..available),
        ) {
            target.copy_from_slice(source);
        }
        available
    }

    fn wstr_len(&mut self, ptr: usize, max: usize) -> usize {
        if !self.in_arena(ptr) {
            return 0;
        }
        let limit = max.min(ARENA_LEN.saturating_sub(ptr) / 4);
        let mut length = 0usize;
        while length < limit {
            let at = ptr.saturating_add(length.saturating_mul(4));
            let mut bytes = [0u8; 4];
            let copied = self.read_bytes(at, 0, &mut bytes);
            if copied < 4 || u32::from_le_bytes(bytes) == 0 {
                return length;
            }
            length = length.saturating_add(1);
        }
        length
    }

    fn read_wchars(&mut self, ptr: usize, offset: usize, buf: &mut [u32]) -> usize {
        if !self.in_arena(ptr) {
            return 0;
        }
        let start = ptr.saturating_add(offset.saturating_mul(4));
        if !self.in_arena(start) {
            return 0;
        }
        let available = (ARENA_LEN.saturating_sub(start) / 4).min(buf.len());
        for (index, slot) in buf.iter_mut().take(available).enumerate() {
            let at = start.saturating_add(index.saturating_mul(4));
            let mut bytes = [0u8; 4];
            if self.read_bytes(at, 0, &mut bytes) == 4 {
                *slot = u32::from_le_bytes(bytes);
            }
        }
        available
    }

    fn store_count(&mut self, ptr: usize, size: StoreSize, value: i64) {
        self.push(Store::Integer {
            ptr,
            size: size.bytes() as u8,
            value: value as u64,
        });
    }

    fn errno_text(&mut self, errnum: i32, buf: &mut [u8; 64]) -> usize {
        let mut copy = *b"errno:00";
        if let Some(byte) = copy.get_mut(7) {
            *byte = b'0'.saturating_add((errnum as u8) % 10);
        }
        buf.fill(0);
        if let Some(target) = buf.get_mut(..copy.len()) {
            target.copy_from_slice(&copy);
        }
        copy.len()
    }

    fn store_int(&mut self, ptr: usize, size: StoreSize, value: u64) {
        self.push(Store::Integer {
            ptr,
            size: size.bytes() as u8,
            value,
        });
    }

    fn store_chars(&mut self, ptr: usize, data: &[u8]) {
        self.push(Store::Chars {
            ptr,
            len: data.len(),
        });
    }

    fn store_wchars(&mut self, ptr: usize, data: &[u32]) {
        self.push(Store::Wide {
            ptr,
            len: data.len(),
        });
    }

    fn store_float_bits(&mut self, ptr: usize, _kind: FloatKind, _bits: u128) {
        self.push(Store::Float { ptr });
    }

    fn alloc_store(&mut self, dest_ptr: usize, bytes: &[u8], _wide: bool) -> bool {
        let ok = self.alloc_calls < ALLOC_SUCCESSES;
        self.alloc_calls = self.alloc_calls.saturating_add(1);
        self.push(Store::Alloc {
            ptr: dest_ptr,
            len: bytes.len(),
            ok,
        });
        ok
    }
}

#[cfg(test)]
#[path = "env_ut.rs"]
mod tests;
