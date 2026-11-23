// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K434 allocator reuse tests.
//!
//! The SSH soak grew vfsd by exactly 20 user chunks per session because a small
//! allocation could consume an entire freed 64 KiB block and the tail was never
//! returned to any free list. These tests pin the reuse contract with a counting
//! page source: a repeated mixed-size pattern must not map a chunk per cycle.

use alloc_crate::alloc::GlobalAlloc;
use alloc_crate::sync::Arc;
use core::alloc::Layout;
use core::sync::atomic::{AtomicUsize, Ordering};

use super::{Allocator, PageSource};

struct CountingSource {
    mapped: Arc<AtomicUsize>,
}

impl PageSource for CountingSource {
    fn map(&mut self, len: usize) -> Option<*mut u8> {
        let mut chunk = alloc_crate::vec![0u8; len + 64].into_boxed_slice();
        let base = chunk.as_mut_ptr() as usize;
        let pointer = (base + 63) & !63;
        // The allocator never unmaps, and this test source keeps the chunks alive for the
        // allocator's lifetime just like the production `MAP` source.
        core::mem::forget(chunk);
        self.mapped.fetch_add(len, Ordering::Relaxed);
        Some(pointer as *mut u8)
    }

    fn unmap(&mut self, _pointer: *mut u8, _len: usize) {}
}

fn with_allocator<T>(body: impl FnOnce(&Allocator<CountingSource>, &Arc<AtomicUsize>) -> T) -> T {
    let mapped = Arc::new(AtomicUsize::new(0));
    let allocator = Allocator::new(CountingSource {
        mapped: Arc::clone(&mapped),
    });
    body(&allocator, &mapped)
}

#[test]
fn mixed_size_churn_does_not_map_a_chunk_per_cycle() {
    with_allocator(|allocator, mapped| {
        let small = Layout::from_size_align(32, 8).unwrap();
        let large = Layout::from_size_align(64 * 1024, 8).unwrap();
        // SAFETY: `allocator` is live and every pointer is handed back with the same layout.
        unsafe {
            let first = allocator.alloc(large);
            assert!(!first.is_null());
            allocator.dealloc(first, large);
            let baseline = mapped.load(Ordering::Relaxed);
            for cycle in 0..64 {
                let small_pointer = allocator.alloc(small);
                let large_pointer = allocator.alloc(large);
                assert!(!small_pointer.is_null(), "small allocation {cycle}");
                assert!(!large_pointer.is_null(), "large allocation {cycle}");
                allocator.dealloc(small_pointer, small);
                allocator.dealloc(large_pointer, large);
            }
            let grown = mapped.load(Ordering::Relaxed).saturating_sub(baseline);
            assert!(
                grown <= 2 * 80 * 1024,
                "allocator mapped {grown} extra bytes over 64 mixed cycles"
            );
        }
    });
}
