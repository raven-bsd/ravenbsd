// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::pages`].

#![allow(clippy::indexing_slicing)]

use super::*;

use host::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn default_provider_uses_the_abi_page_size() {
    let provider = SysPageProvider::new();
    assert_eq!(provider.page_size(), 4096);
    assert_eq!(SysPageProvider::default().page_size(), 4096);
}

#[test]
fn explicit_page_size_rejects_anything_else() {
    assert!(SysPageProvider::with_page_size(4096).is_some());
    assert!(SysPageProvider::with_page_size(8192).is_none());
    assert!(SysPageProvider::with_page_size(0).is_none());
}

#[test]
fn provider_can_be_built_from_the_auxv() {
    let mut words = host::vec![0usize, 0usize, 0usize];
    words.push(crate::auxv::AT_PAGESZ);
    words.push(4096);
    words.push(crate::auxv::AT_NULL);
    words.push(0);
    let mut argc = 0usize;
    let mut argv = core::ptr::null();
    let mut envp = core::ptr::null();
    // SAFETY: the words form a well-formed C8 stack.
    let auxv =
        unsafe { crate::auxv::AuxVec::from_stack(words.as_ptr(), &mut argc, &mut argv, &mut envp) }
            .unwrap();
    assert_eq!(
        SysPageProvider::from_auxv(&auxv).map(|provider| provider.page_size()),
        Some(4096)
    );
}

#[test]
fn rounding_and_alignment_helpers_are_bounded() {
    assert_eq!(round_up(1, 4096), Some(4096));
    assert_eq!(round_up(4096, 4096), Some(4096));
    assert_eq!(round_up(4097, 4096), Some(8192));
    assert_eq!(round_up(usize::MAX, 4096), None);
    assert!(page_aligned(0, 4096));
    assert!(page_aligned(8192, 4096));
    assert!(!page_aligned(1, 4096));
    assert!(!page_aligned(4096, 0));
}

#[test]
fn provider_rejects_invalid_requests_before_any_syscall() {
    let provider = SysPageProvider::new();
    assert_eq!(provider.map_pages(0, 4096, 3), None);
    assert_eq!(provider.map_pages(4096, 0, 3), None);
    assert_eq!(provider.map_pages(4096, 3, 3), None);
    assert_eq!(provider.map_exact(1, 4096), None);
    assert_eq!(provider.map_exact(4096, 0), None);
    assert!(!provider.unmap(0x1000, 0));
    assert!(!provider.protect(0x1000, 0, 3));
    assert!(provider.seal(0x1000, 4096));
}

#[test]
fn registry_tracks_and_forgets_whole_mappings() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let addr = 0x7000_0000_0000usize;
    assert!(record_mapping(addr, 8192));
    assert!(contains_range(addr, 8192));
    assert!(contains_range(addr + 4096, 4096));
    assert!(!contains_range(addr + 8192, 4096));
    assert!(!forget_mapping(addr, 4096), "the length must match exactly");
    assert!(forget_mapping(addr, 8192));
    assert!(!contains_range(addr, 4096));
}

#[cfg(feature = "host-fake")]
#[test]
fn map_and_unmap_return_to_the_starting_accounting() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let before = crate::fake::mapped_bytes();
    let provider = SysPageProvider::new();
    let addr = provider.map_pages(4096, 4096, 3).unwrap();
    assert_eq!(crate::fake::mapped_bytes(), before + 4096);
    assert!(provider.unmap(addr, 4096));
    assert_eq!(crate::fake::mapped_bytes(), before);
    assert_eq!(tracked_mappings(), 0);
}

#[cfg(feature = "host-fake")]
#[test]
fn alignment_above_a_page_is_satisfied() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let provider = SysPageProvider::new();
    let addr = provider.map_pages(4096, 64 * 1024, 3).unwrap();
    assert_eq!(addr % (64 * 1024), 0);
    assert_eq!(crate::fake::mapped_bytes(), 4096);
    assert!(provider.unmap(addr, 4096));
    assert_eq!(crate::fake::mapped_bytes(), 0);
}

#[cfg(feature = "host-fake")]
#[test]
fn first_touch_memory_is_zeroed() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let provider = SysPageProvider::new();
    let addr = provider.map_pages(8192, 4096, 3).unwrap();
    let bytes = crate::fake::peek(addr, 8192).unwrap();
    assert!(bytes.iter().all(|&byte| byte == 0));
    assert!(provider.unmap(addr, 8192));
}

#[cfg(feature = "host-fake")]
#[test]
fn map_exact_honours_the_hint_and_rejects_a_busy_range() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let provider = SysPageProvider::new();
    let hint = 0x0000_7000_0010_0000usize;
    let addr = provider.map_exact(hint, 8192).unwrap();
    assert_eq!(addr, hint);
    assert_eq!(provider.map_exact(hint, 4096), None);
    assert!(provider.unmap(addr, 8192));
}

#[cfg(feature = "host-fake")]
#[test]
fn protect_requires_a_tracked_range() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let provider = SysPageProvider::new();
    let addr = provider.map_pages(3 * 4096, 4096, 3).unwrap();
    assert!(provider.protect(addr, 3 * 4096, 1));
    assert!(provider.protect(addr + 4096, 4096, 1));
    assert!(!provider.protect(addr, 4 * 4096, 1));
    assert!(provider.unmap(addr, 3 * 4096));
}

#[cfg(feature = "host-fake")]
#[test]
fn ten_thousand_random_operations_leave_the_accounting_unchanged() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let provider = SysPageProvider::new();
    let before = crate::fake::mapped_bytes();
    let mut live: host::vec::Vec<(usize, usize)> = host::vec::Vec::new();
    let mut seed = 0xC54_u64;
    for _ in 0..10_000 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let choice = (seed >> 33) % 3;
        match choice {
            0 if live.len() < 8 => {
                let pages = ((seed >> 40) % 4 + 1) as usize;
                let align = if seed & 1 == 0 { 4096 } else { 65536 };
                if let Some(addr) = provider.map_pages(pages * 4096, align, 3) {
                    live.push((addr, pages * 4096));
                }
            }
            1 => {
                if let Some((addr, len)) = live.pop() {
                    assert!(provider.unmap(addr, len));
                }
            }
            _ => {
                if let Some(&(addr, len)) = live.first() {
                    let _ = provider.protect(addr, len, if seed & 2 == 0 { 1 } else { 3 });
                }
            }
        }
    }
    while let Some((addr, len)) = live.pop() {
        assert!(provider.unmap(addr, len));
    }
    assert_eq!(crate::fake::mapped_bytes(), before);
    assert_eq!(tracked_mappings(), 0);
}
