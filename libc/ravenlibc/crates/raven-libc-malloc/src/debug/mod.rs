// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The libumem-inspired debug front end.
//!
//! Implemented from the prose of doc 08 4.9 and the illumos design read for behaviour (DR-40);
//! no donor code is copied. Redzones, fill, quarantine, double-free and use-after-free detection,
//! failure injection and a leak report are provided. Per-CPU magazines, vmem arenas and slab
//! caches are explicitly out of scope (`raven-umem`).

#![allow(clippy::indexing_slicing)] // Fixed audit arrays indexed by a bound-checked cursor.

pub mod audit;
pub mod consts;
pub mod env;
pub mod guard;
pub mod redzone;
pub mod report;

use crate::compat::{Heap, Mallinfo2};
use crate::consts::{MAX_ALLOC, align_up};
use crate::debug::audit::Record;
use crate::debug::consts::{ALLOC_FILL, FREE_FILL, MAX_RECORDS, MAX_STACK, REDZONE};
use crate::debug::env::{DebugOptions, parse};
use crate::debug::report::Group;
use crate::errors::{Message, wrterror};
use crate::provider::Platform;
use crate::raw::{self, SyncCell};

/// A debug allocator over another heap.
pub struct Debug<H: Heap, X: Platform> {
    inner: H,
    platform: X,
    options: DebugOptions,
    records: SyncCell<[Record; MAX_RECORDS]>,
    used: SyncCell<usize>,
    sequence: SyncCell<u64>,
    allocations: SyncCell<usize>,
}

impl<H: Heap, X: Platform> Debug<H, X> {
    /// Wrap `inner` and read `MALLOC_DEBUG` unless the process is secure-exec.
    pub fn new(inner: H, platform: X, defaults: DebugOptions) -> Self {
        let options = if !platform.is_secure_exec()
            && let Some(environment) = platform.env_malloc_debug()
        {
            parse(environment)
        } else {
            defaults
        };
        Self {
            inner,
            platform,
            options,
            records: SyncCell::new([Record::default(); MAX_RECORDS]),
            used: SyncCell::new(0),
            sequence: SyncCell::new(1),
            allocations: SyncCell::new(0),
        }
    }

    /// The parsed debug options.
    #[must_use]
    pub const fn options(&self) -> DebugOptions {
        self.options
    }

    fn fail(&self, text: &str, pointer: usize) -> ! {
        let mut message = Message::new();
        message.push_str(text);
        message.push(b' ');
        message.push_ptr(pointer);
        wrterror(&self.platform, b"debug", &message)
    }

    fn find(&self, pointer: usize) -> Option<(usize, Record)> {
        let records = self.records.get();
        let used = self.used.get();
        let mut index = 0usize;
        while index < used {
            if records[index].matches(pointer) {
                return Some((index, records[index]));
            }
            index = index.wrapping_add(1);
        }
        None
    }

    fn store(&self, index: usize, record: Record) {
        let mut records = self.records.get();
        records[index] = record;
        self.records.set(records);
    }

    fn allocate_record(&self, record: Record) -> Option<usize> {
        let mut records = self.records.get();
        let used = self.used.get();
        let mut index = 0usize;
        while index < used {
            if !records[index].live && !records[index].quarantined {
                records[index] = record;
                self.records.set(records);
                return Some(index);
            }
            index = index.wrapping_add(1);
        }
        if used >= MAX_RECORDS {
            return None;
        }
        records[used] = record;
        self.records.set(records);
        self.used.set(used.wrapping_add(1));
        Some(used)
    }

    fn next_sequence(&self) -> u64 {
        let value = self.sequence.get();
        self.sequence.set(value.wrapping_add(1));
        value
    }

    fn evict_one(&self) {
        let mut records = self.records.get();
        let used = self.used.get();
        let mut oldest = usize::MAX;
        let mut oldest_seq = u64::MAX;
        let mut index = 0usize;
        while index < used {
            if records[index].quarantined && records[index].free_seq < oldest_seq {
                oldest_seq = records[index].free_seq;
                oldest = index;
            }
            index = index.wrapping_add(1);
        }
        if oldest == usize::MAX {
            return;
        }
        self.verify_free_fill(&records[oldest]);
        self.inner.free(records[oldest].base);
        records[oldest].live = false;
        records[oldest].quarantined = false;
        records[oldest].user = 0;
        self.records.set(records);
    }

    fn verify_free_fill(&self, record: &Record) {
        if !self.options.fill || record.size == 0 {
            return;
        }
        let mut index = 0usize;
        while index < record.size {
            // SAFETY: the record describes a quarantined allocation.
            let value = raw::read_u8(record.user.wrapping_add(index));
            if value != FREE_FILL {
                self.fail("write to free memory", record.user.wrapping_add(index));
            }
            index = index.wrapping_add(1);
        }
    }

    fn quarantine(&self, index: usize, mut record: Record) {
        if self.options.fill {
            // SAFETY: the allocation is live and `size` bytes are writable.
            raw::fill(record.user, FREE_FILL, record.size);
        }
        record.quarantined = true;
        record.free_seq = self.next_sequence();
        self.store(index, record);
        let mut quarantined = 0usize;
        let records = self.records.get();
        let used = self.used.get();
        let mut cursor = 0usize;
        while cursor < used {
            if records[cursor].quarantined {
                quarantined = quarantined.wrapping_add(1);
            }
            cursor = cursor.wrapping_add(1);
        }
        if quarantined > self.options.audit {
            self.evict_one();
        }
    }

    fn allocate(&self, size: usize, align: usize, stack: &[usize]) -> usize {
        if size > MAX_ALLOC {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        }
        let count = self.allocations.get().wrapping_add(1);
        self.allocations.set(count);
        if self.options.fail_every != 0 && count.is_multiple_of(self.options.fail_every) {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        }
        let align = align.max(crate::consts::MALLOC_ALIGN);
        let Some(redzone) = (if self.options.redzone { REDZONE } else { 0 }).checked_mul(2) else {
            return 0;
        };
        let Some(extra) = size.checked_add(redzone).and_then(|v| v.checked_add(align)) else {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        };
        let base = self.inner.memalign(align, extra);
        if base == 0 {
            return 0;
        }
        let user = if self.options.redzone {
            match align_up(base.wrapping_add(REDZONE), align) {
                Some(value) => value,
                None => {
                    self.inner.free(base);
                    return 0;
                }
            }
        } else {
            base
        };
        if self.options.redzone {
            redzone::fill(user, size);
        }
        if self.options.fill && size != 0 {
            // SAFETY: the payload is inside the inner allocation.
            raw::fill(user, ALLOC_FILL, size);
        }
        let mut record = Record {
            user,
            base,
            size,
            align,
            thread: self.platform.thread_id(),
            seq: self.next_sequence(),
            live: true,
            ..Record::default()
        };
        let limit = stack.len().min(MAX_STACK);
        let mut index = 0usize;
        while index < limit {
            record.stack[index] = stack[index];
            index = index.wrapping_add(1);
        }
        record.stack_len = limit;
        if self.allocate_record(record).is_none() {
            self.inner.free(base);
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        }
        user
    }

    fn release(&self, pointer: usize, size: Option<usize>) {
        if pointer == 0 {
            return;
        }
        let Some((index, record)) = self.find(pointer) else {
            self.fail("free: never allocated", pointer);
        };
        if record.quarantined {
            self.fail("double free", pointer);
        }
        if !record.live {
            self.fail("free: already freed", pointer);
        }
        if let Some(size) = size
            && size != record.size
        {
            self.fail("free_sized: size mismatch", pointer);
        }
        if self.options.redzone
            && let Some((side, offset)) = redzone::check(record.user, record.size)
        {
            let mut message = Message::new();
            message.push_str(side);
            message.push_str(" at ");
            message.push_ptr(record.user);
            message.push(b'+');
            message.push_dec(offset);
            message.push_str(" size ");
            message.push_dec(record.size);
            wrterror(&self.platform, b"free", &message);
        }
        if self.options.audit != 0 {
            self.quarantine(index, record);
        } else {
            self.inner.free(record.base);
            let mut cleared = record;
            cleared.live = false;
            cleared.user = 0;
            self.store(index, cleared);
        }
    }

    /// `malloc` with a caller-supplied stack.
    pub fn malloc_with_stack(&self, size: usize, stack: &[usize]) -> usize {
        self.allocate(size, crate::consts::MALLOC_ALIGN, stack)
    }

    /// Walk live records and emit grouped leak lines.
    pub fn report_leaks(&self, out: &mut dyn FnMut(&[u8])) {
        let records = self.records.get();
        let used = self.used.get();
        let mut groups = [Group::default(); MAX_RECORDS];
        let mut group_count = 0usize;
        let mut index = 0usize;
        while index < used {
            if records[index].live {
                let stack = records[index].stack_head();
                let mut found = false;
                let mut group = 0usize;
                while group < group_count {
                    if groups[group].size == records[index].size && groups[group].stack == stack {
                        groups[group].count = groups[group].count.wrapping_add(1);
                        groups[group].bytes =
                            groups[group].bytes.saturating_add(records[index].size);
                        found = true;
                        break;
                    }
                    group = group.wrapping_add(1);
                }
                if !found && group_count < groups.len() {
                    groups[group_count] = Group {
                        size: records[index].size,
                        stack,
                        count: 1,
                        bytes: records[index].size,
                    };
                    group_count = group_count.wrapping_add(1);
                }
            }
            index = index.wrapping_add(1);
        }
        let mut group = 0usize;
        while group < group_count {
            report::emit(out, &groups[group]);
            group = group.wrapping_add(1);
        }
        let mut summary = Message::new();
        summary.push_str("total ");
        summary.push_dec(group_count);
        summary.push_str(" group(s)\n");
        out(summary.as_bytes());
    }
}

impl<H: Heap, X: Platform> Heap for Debug<H, X> {
    fn malloc(&self, size: usize) -> usize {
        self.allocate(size, crate::consts::MALLOC_ALIGN, &[])
    }

    fn free(&self, p: usize) {
        self.release(p, None);
    }

    fn realloc(&self, p: usize, size: usize) -> usize {
        if p == 0 {
            return self.malloc(size);
        }
        if size == 0 {
            return self.malloc(1);
        }
        let Some((_, record)) = self.find(p) else {
            self.fail("realloc: never allocated", p);
        };
        let fresh = self.malloc(size);
        if fresh == 0 {
            return 0;
        }
        let copy = if size < record.size {
            size
        } else {
            record.size
        };
        // SAFETY: both allocations are live and the copy is bounded.
        raw::copy(fresh, p, copy);
        self.free(p);
        fresh
    }

    fn calloc(&self, n: usize, m: usize) -> usize {
        let Some(size) = crate::compat::checks::array_size(n, m) else {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        };
        let pointer = self.malloc(size);
        if pointer != 0 && size != 0 {
            // SAFETY: the allocation is live and `size` bytes are writable.
            raw::fill(pointer, 0, size);
        }
        pointer
    }

    fn memalign(&self, align: usize, size: usize) -> usize {
        if align == 0 || !align.is_power_of_two() {
            self.platform.set_errno(raven_libc_abi::errno::EINVAL);
            return 0;
        }
        self.allocate(size, align, &[])
    }

    fn usable_size(&self, p: usize) -> usize {
        if p == 0 {
            return 0;
        }
        match self.find(p) {
            Some((_, record)) => record.size,
            None => self.fail("usable_size: never allocated", p),
        }
    }

    fn trim(&self) -> bool {
        self.inner.trim()
    }

    fn info(&self) -> Mallinfo2 {
        self.inner.info()
    }

    fn free_sized(&self, p: usize, size: usize) {
        self.release(p, Some(size));
    }

    fn free_aligned_sized(&self, p: usize, _align: usize, size: usize) {
        self.release(p, Some(size));
    }

    fn prefork(&self) {
        self.inner.prefork();
    }

    fn postfork_parent(&self) {
        self.inner.postfork_parent();
    }

    fn postfork_child(&self) {
        self.inner.postfork_child();
    }
}

#[cfg(test)]
#[path = "debug_ut.rs"]
mod tests;
