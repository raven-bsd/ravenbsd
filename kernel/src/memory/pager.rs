// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pager fault service: the address-space lock has been released by `process::handle_page_fault`.
//!
//! The first fault on an absent page owns one PageIn request; later faults wait on the object's
//! page queue. The supplied page is mapped after the wait. PageOut is the symmetric writeback used
//! by unmap; the pager reads the object frame itself (it holds the object handle).

#![cfg(target_arch = "x86_64")]

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_pager::{PAGE_IN, PageIn, PageOut};

use crate::memory::uspace::PagerFault;
use crate::process::FaultOutcome;
use crate::process::Process;
use crate::process::handle::MemObject;
use crate::sched;

/// Resolve a pager fault. `Bus` is a file-backed page-in beyond EOF: POSIX raises SIGBUS, while
/// the pager error/death paths for an anonymous pager and a short supply stay SIGSEGV.
pub fn service(process: &Arc<Process>, fault: PagerFault) -> FaultOutcome {
    let object = fault.object;
    let page = fault.page;
    match object.begin_page_in(page) {
        PageIn::Present => fault_map(process, fault.cr2),
        PageIn::Failed => FaultOutcome::Segv,
        PageIn::Wait => {
            if wait_page(&object, page) {
                fault_map(process, fault.cr2)
            } else {
                FaultOutcome::Segv
            }
        }
        PageIn::Send => {
            let result = crate::ipc::router::pager_call(process, &object, page, PAGE_IN);
            match result {
                Ok(0) if object.page_ready(page) => fault_map(process, fault.cr2),
                Ok(status)
                    if status == -i32::from(Errno::ENXIO.0)
                        || status == i32::from(Errno::ENXIO.0) =>
                {
                    object.fail_page(page);
                    FaultOutcome::Bus
                }
                Err(Errno::ENXIO) => {
                    object.fail_page(page);
                    FaultOutcome::Bus
                }
                _ => {
                    object.fail_page(page);
                    FaultOutcome::Segv
                }
            }
        }
    }
}

/// Map the supplied page and report the retry outcome.
fn fault_map(process: &Arc<Process>, cr2: u64) -> FaultOutcome {
    if map(process, cr2) {
        FaultOutcome::Retry
    } else {
        FaultOutcome::Segv
    }
}

/// Wait for another fault's in-flight supply on `page`.
fn wait_page(object: &Arc<MemObject>, page: u64) -> bool {
    let token = match sched::begin_block() {
        Ok(token) => token,
        Err(_) => return false,
    };
    let queued = object
        .page_waiters()
        .enqueue_if(object.object_id(), page, || Ok(!object.page_ready(page)));
    match queued {
        Ok(true) => {
            let _ = sched::block_current();
            sched::end_block(token);
            object.page_ready(page)
        }
        Ok(false) => {
            sched::end_block(token);
            object.page_ready(page)
        }
        Err(_) => {
            sched::end_block(token);
            false
        }
    }
}

/// Re-acquire the address space and map the supplied page.
fn map(process: &Arc<Process>, cr2: u64) -> bool {
    let mut guard = process.address_space.lock();
    let Some(address_space) = guard.as_mut() else {
        return false;
    };
    address_space.map_pager_page(cr2).is_ok()
}

/// Write one dirty page back through the pager after the address-space lock is released.
pub fn page_out(process: &Arc<Process>, object: &Arc<MemObject>, page: u64) -> Result<(), Errno> {
    match object.begin_page_out(page) {
        PageOut::Clean => Ok(()),
        // Another caller owns the writeback; the frame stays in the object and the pager will
        // finish it, so this unmap can proceed.
        PageOut::Wait => Ok(()),
        PageOut::Send => {
            let status =
                crate::ipc::router::pager_call(process, object, page, raven_pager::PAGE_OUT)?;
            if status == 0 {
                object.page_out_done(page);
                Ok(())
            } else {
                object.page_out_fail(page);
                Err(Errno::EIO)
            }
        }
    }
}
