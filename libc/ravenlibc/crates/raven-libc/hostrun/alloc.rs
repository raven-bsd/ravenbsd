// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Host-run only: a tiny Linux mmap bump allocator so the tested libc paths can allocate without
// reaching Raven's `mmap` syscall.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

struct Bump;

static NEXT: AtomicUsize = AtomicUsize::new(0);
static END: AtomicUsize = AtomicUsize::new(0);

unsafe fn map(size: usize) -> usize {
    let result: isize;
    // SAFETY: Linux mmap(NULL, size, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0).
    unsafe {
        asm!("syscall", in("rax") 9usize, in("rdi") 0usize, in("rsi") size, in("rdx") 3usize, in("r10") 0x22usize, in("r8") usize::MAX, in("r9") 0usize, lateout("rax") result, lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    if result < 0 { 0 } else { result as usize }
}

unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(1).saturating_add(layout.align());
        let mut current = NEXT.load(Ordering::Relaxed);
        let end = END.load(Ordering::Relaxed);
        if current == 0 || current.saturating_add(size) > end {
            let page = (size.saturating_add(4095)) & !4095;
            let mapped = unsafe { map(page.max(4096)) };
            if mapped == 0 { return core::ptr::null_mut(); }
            current = mapped;
            END.store(mapped.saturating_add(page.max(4096)), Ordering::Relaxed);
        }
        let aligned = (current.saturating_add(layout.align() - 1)) & !(layout.align() - 1);
        NEXT.store(aligned.saturating_add(layout.size().max(1)), Ordering::Relaxed);
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let target = unsafe { self.alloc(Layout::from_size_align_unchecked(new_size, layout.align())) };
        if !target.is_null() {
            // SAFETY: the old allocation is at least `layout.size()` bytes.
            unsafe { core::ptr::copy_nonoverlapping(ptr, target, layout.size().min(new_size)) };
        }
        target
    }
}

#[global_allocator]
static ALLOCATOR: Bump = Bump;
