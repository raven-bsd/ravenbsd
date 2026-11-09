// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A schedulable thread.
//!
//! S08 creates exactly one thread per process. The kernel stack is a heap-backed 16 KiB region
//! with its lowest page unmapped as a guard.

use alloc::sync::Arc;

use super::Process;

/// One thread in a process.
#[allow(dead_code)] // Stored for the S09 scheduler; S08 only enters the first thread.
pub struct Thread {
    /// Owning process.
    pub process: Arc<Process>,
    /// Bottom of the reserved 5-page kernel-stack allocation (the guard page).
    pub kernel_stack_base: u64,
    /// Top of the mapped 4-page kernel stack.
    pub kernel_stack_top: u64,
    /// User entry point.
    pub entry: u64,
    /// Initial user stack pointer.
    pub user_stack: u64,
}
