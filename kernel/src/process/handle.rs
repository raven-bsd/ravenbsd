// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel objects stored in a process handle table.

use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use raven_handles::{HandleTable, LineageNode};
use spin::Mutex;

use crate::driver::DmaBuffer;
use crate::driver::irq::Irq;
use crate::event::{EventQueue, Timer, UserEvent};
#[cfg(target_arch = "x86_64")]
use crate::ipc::{PendingRequest, SchemeFile, SchemeServer};
use crate::memory::frame;

use super::Process;
use super::thread::Thread;

/// Next pager object id (the page wait queue key).
static NEXT_OBJECT_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// Live pager-backed objects (S17 leak evidence).
static PAGER_OBJECTS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Live pager-backed objects.
pub fn pager_objects() -> u64 {
    PAGER_OBJECTS.load(core::sync::atomic::Ordering::Acquire)
}

/// The process that owns a pager-backed object and the handle value it uses as the pager key.
#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
pub struct PagerEndpoint {
    process: Weak<Process>,
}

struct MemState {
    frames: Vec<Option<u64>>,
    pages: raven_pager::PagerObject,
}

/// A memory object.
///
/// Anonymous objects are eager: every frame is allocated and present. Pager-backed objects are
/// lazy: `frames` holds `None` until the pager supplies the page, and `pages` is the pure page
/// state machine. The state lock is a leaf; callers take it, release it and only then wake the
/// page wait queue, so it never nests with the address-space lock or the wait-queue lock.
#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
pub struct MemObject {
    id: u64,
    pages: usize,
    state: Mutex<MemState>,
    waiters: crate::sched::wait::WaitQueue,
    pager: Option<PagerEndpoint>,
    pager_handle: core::sync::atomic::AtomicU32,
}

#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
impl MemObject {
    /// Allocate `pages` zeroed frames. On failure every frame allocated so far is returned.
    pub fn new(pages: u64) -> Result<Arc<MemObject>, Errno> {
        let pages = usize::try_from(pages).map_err(|_| Errno::ENOMEM)?;
        let page_states = raven_pager::PagerObject::present(pages).ok_or(Errno::ENOMEM)?;
        let mut frames: Vec<Option<u64>> = Vec::new();
        frames.try_reserve(pages).map_err(|_| Errno::ENOMEM)?;
        let mut index = 0u64;
        while index < pages as u64 {
            let Some(frame) = frame::alloc_zeroed_frame() else {
                for physical in frames.iter().flatten() {
                    let _ = frame::free_frame(frame::PhysAddr(*physical));
                }
                return Err(Errno::ENOMEM);
            };
            frames.push(Some(frame.0));
            index = index.saturating_add(1);
        }
        Ok(Arc::new(MemObject {
            id: NEXT_OBJECT_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
            pages,
            state: Mutex::new(MemState {
                frames,
                pages: page_states,
            }),
            waiters: crate::sched::wait::WaitQueue::new(),
            pager: None,
            pager_handle: core::sync::atomic::AtomicU32::new(0),
        }))
    }

    /// Create a lazy pager-backed object; the caller becomes the pager.
    pub fn new_paged(pages: u64, pager: &Arc<Process>) -> Result<Arc<MemObject>, Errno> {
        let pages = usize::try_from(pages).map_err(|_| Errno::ENOMEM)?;
        let page_states = raven_pager::PagerObject::new(pages).ok_or(Errno::ENOMEM)?;
        let mut frames: Vec<Option<u64>> = Vec::new();
        frames.try_reserve(pages).map_err(|_| Errno::ENOMEM)?;
        frames.resize(pages, None);
        PAGER_OBJECTS.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
        Ok(Arc::new(MemObject {
            id: NEXT_OBJECT_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
            pages,
            state: Mutex::new(MemState {
                frames,
                pages: page_states,
            }),
            waiters: crate::sched::wait::WaitQueue::new(),
            pager: Some(PagerEndpoint {
                process: Arc::downgrade(pager),
            }),
            pager_handle: core::sync::atomic::AtomicU32::new(0),
        }))
    }

    /// Set the pager key (the handle value the pager received from `MEMOBJ_CREATE`).
    pub fn set_pager_handle(&self, handle: u32) {
        self.pager_handle
            .store(handle, core::sync::atomic::Ordering::Release);
    }

    /// True for a pager-backed object.
    pub fn is_paged(&self) -> bool {
        self.pager.is_some()
    }

    /// The pager process, if it is still alive.
    pub fn pager_process(&self) -> Option<Arc<Process>> {
        self.pager
            .as_ref()
            .and_then(|endpoint| endpoint.process.upgrade())
    }

    /// The pager's object handle value.
    pub fn pager_handle(&self) -> u32 {
        self.pager_handle
            .load(core::sync::atomic::Ordering::Acquire)
    }

    /// True when `process` is this object's registered pager.
    pub fn is_pager(&self, process: &Arc<Process>) -> bool {
        self.pager_process()
            .is_some_and(|pager| Arc::ptr_eq(&pager, process))
    }

    /// Allocate page-rounded frames and copy `bytes` into them. Padding stays zeroed.
    pub fn from_bytes(bytes: &[u8]) -> Result<Arc<MemObject>, Errno> {
        let pages = bytes.len().div_ceil(frame::PAGE_SIZE as usize);
        let pages = u64::try_from(pages.max(1)).map_err(|_| Errno::EINVAL)?;
        let object = MemObject::new(pages)?;
        let mut index = 0u64;
        let mut copied = 0usize;
        while copied < bytes.len() {
            let Some(physical) = object.frame_at(index) else {
                return Err(Errno::ENOMEM);
            };
            let destination = frame::phys_to_virt(frame::PhysAddr(physical));
            if destination.is_null() {
                return Err(Errno::ENOMEM);
            }
            let chunk = core::cmp::min(
                frame::PAGE_SIZE as usize,
                bytes.len().saturating_sub(copied),
            );
            // SAFETY: `destination` is a fresh frame mapped writable through the HHDM and `chunk`
            // stays within both the frame and the remaining source bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr().add(copied), destination, chunk);
            }
            copied = copied.saturating_add(chunk);
            index = index.saturating_add(1);
        }
        Ok(object)
    }

    /// Length in bytes.
    pub fn len(&self) -> u64 {
        u64::try_from(self.pages)
            .unwrap_or(u64::MAX)
            .saturating_mul(frame::PAGE_SIZE)
    }

    /// Physical frame for `index` if the page is present.
    pub fn frame_at(&self, index: u64) -> Option<u64> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.state.lock().frames.get(index).copied().flatten())
    }

    /// Page state (pager-backed objects).
    pub fn page_state(&self, index: u64) -> Option<raven_pager::PageState> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.state.lock().pages.state(index))
    }

    /// True when the page can be mapped now.
    pub fn page_ready(&self, index: u64) -> bool {
        usize::try_from(index)
            .ok()
            .is_some_and(|index| self.state.lock().pages.ready(index))
    }

    /// Mark a fault's page: the first fault owns the PageIn, later faults wait.
    pub fn begin_page_in(&self, index: u64) -> raven_pager::PageIn {
        usize::try_from(index)
            .ok()
            .map(|index| self.state.lock().pages.begin_page_in(index))
            .unwrap_or(raven_pager::PageIn::Failed)
    }

    /// Supply one page from the pager's user buffer.
    pub fn supply_page(&self, index: u64, data: &[u8]) -> Result<(), Errno> {
        if data.is_empty() || data.len() > frame::PAGE_SIZE as usize {
            return Err(Errno::EINVAL);
        }
        let index = usize::try_from(index).map_err(|_| Errno::EINVAL)?;
        let mut state = self.state.lock();
        let current = state.pages.state(index);
        if !matches!(
            current,
            Some(raven_pager::PageState::Absent | raven_pager::PageState::InFlight)
        ) {
            return Err(Errno::EBUSY);
        }
        let Some(slot) = state.frames.get_mut(index) else {
            return Err(Errno::EINVAL);
        };
        let physical = match *slot {
            Some(physical) => physical,
            None => {
                let Some(frame) = frame::alloc_zeroed_frame() else {
                    return Err(Errno::ENOMEM);
                };
                *slot = Some(frame.0);
                frame.0
            }
        };
        let destination = frame::phys_to_virt(frame::PhysAddr(physical));
        if destination.is_null() {
            return Err(Errno::ENOMEM);
        }
        // SAFETY: `destination` is a whole live frame; zero the page and copy at most one page of
        // caller data, so no stale bytes remain and no write leaves the frame.
        unsafe {
            core::ptr::write_bytes(destination, 0, frame::PAGE_SIZE as usize);
            core::ptr::copy_nonoverlapping(data.as_ptr(), destination, data.len());
        }
        if !state.pages.supply(index) {
            return Err(Errno::EBUSY);
        }
        drop(state);
        let _ = self.waiters.wake(self.id, index as u64, usize::MAX);
        Ok(())
    }

    /// Fail a page and wake every waiter with the failed state.
    pub fn fail_page(&self, index: u64) {
        let index = match usize::try_from(index) {
            Ok(index) => index,
            Err(_) => return,
        };
        let failed = {
            let mut state = self.state.lock();
            state.pages.fail(index)
        };
        if failed {
            let _ = self.waiters.wake(self.id, index as u64, usize::MAX);
        }
    }

    /// Mark a present page dirty.
    pub fn mark_dirty(&self, index: u64) {
        if let Ok(index) = usize::try_from(index) {
            let _ = self.state.lock().pages.mark_dirty(index);
        }
    }

    /// Start writeback of a dirty page.
    pub fn begin_page_out(&self, index: u64) -> raven_pager::PageOut {
        usize::try_from(index)
            .ok()
            .map(|index| self.state.lock().pages.begin_page_out(index))
            .unwrap_or(raven_pager::PageOut::Clean)
    }

    /// Complete a writeback.
    pub fn page_out_done(&self, index: u64) {
        if let Ok(index) = usize::try_from(index) {
            let _ = self.state.lock().pages.page_out_done(index);
        }
    }

    /// A failed writeback leaves the page dirty.
    pub fn page_out_fail(&self, index: u64) {
        if let Ok(index) = usize::try_from(index) {
            let _ = self.state.lock().pages.page_out_fail(index);
        }
    }

    /// Number of dirty pages (evidence).
    #[allow(dead_code)] // diagnostic API for the pager evidence
    pub fn dirty_pages(&self) -> usize {
        self.state.lock().pages.dirty_pages()
    }

    /// Number of in-flight page requests (evidence).
    #[allow(dead_code)] // diagnostic API for the pager evidence
    pub fn in_flight(&self) -> usize {
        self.state.lock().pages.in_flight()
    }

    /// Page wait queue (the kernel's pager service publishes under it).
    pub fn page_waiters(&self) -> &crate::sched::wait::WaitQueue {
        &self.waiters
    }

    /// Wait-queue key for this object.
    pub fn object_id(&self) -> u64 {
        self.id
    }
}

impl Drop for MemObject {
    fn drop(&mut self) {
        if self.pager.is_some() {
            PAGER_OBJECTS.fetch_sub(1, core::sync::atomic::Ordering::AcqRel);
        }
        let frames = core::mem::take(&mut self.state.lock().frames);
        for physical in frames.into_iter().flatten() {
            let _ = frame::free_frame(frame::PhysAddr(physical));
        }
    }
}

/// A device-memory window a driver can map (`MMIO_CREATE`). Immutable after creation.
pub struct MmioRegion {
    /// Physical start address (page aligned).
    phys: u64,
    /// Length in bytes (page multiple, non-zero).
    len: u64,
    /// Creation attributes; only 0 is defined today.
    attrs: u32,
}

impl MmioRegion {
    /// Construct a region. Callers must have validated the range.
    pub const fn new(phys: u64, len: u64, attrs: u32) -> Self {
        Self { phys, len, attrs }
    }

    /// Physical start address.
    pub const fn phys(&self) -> u64 {
        self.phys
    }

    /// Length in bytes.
    pub const fn len(&self) -> u64 {
        self.len
    }

    /// Creation attributes.
    #[allow(dead_code)] // Reported to drivers by later tickets.
    pub const fn attrs(&self) -> u32 {
        self.attrs
    }
}

/// Objects a handle can refer to in S08.
#[allow(dead_code)] // Object access is used by later MAP/THREAD tickets; PID 1 only closes/dups now.
#[derive(Clone)]
pub enum Object {
    /// The process itself (well-known handle 0).
    Process(Arc<Process>),
    /// The boot initfs memory object (well-known handle 1).
    MemObjectInitfs,
    /// An anonymous memory object created by `MEMOBJ_CREATE`.
    Memory(Arc<MemObject>),
    /// Root namespace placeholder (well-known handle 2).
    Namespace,
    /// A thread object (created by later tickets).
    Thread(Arc<Thread>),
    /// A wait-for-many event queue.
    EventQueue(Arc<EventQueue>),
    /// A monotonic timer event source.
    Timer(Arc<Timer>),
    /// A user-space doorbell event source.
    UserEvent(Arc<UserEvent>),
    /// The root device capability held by the first process (`Handle::ROOT_DEVICE`).
    RootDevice,
    /// A device-memory window created by `MMIO_CREATE`.
    MmioRegion(Arc<MmioRegion>),
    /// A read-only boot-time blob (`BOOT_BLOB`).
    BootBlob(Arc<MemObject>),
    /// A physically contiguous device buffer (`DMA_ALLOC`).
    DmaBuffer(Arc<DmaBuffer>),
    /// A device interrupt source (`IRQ_CREATE`).
    Irq(Arc<Irq>),
    /// An open scheme file.
    #[cfg(target_arch = "x86_64")]
    SchemeFile(Arc<SchemeFile>),
    /// A registered scheme server capability.
    #[cfg(target_arch = "x86_64")]
    SchemeServer(Arc<SchemeServer>),
    /// A server-side request handle returned by `RECV`.
    #[cfg(target_arch = "x86_64")]
    SchemeRequest(Arc<PendingRequest>),
}

/// The kernel's handle table type.
pub type Table = HandleTable<Object>;

/// Weak registry of every derived lineage node, used to propagate a revoke across processes.
///
/// Root nodes do not need an entry: a revoke resolves its target from the caller's table and only
/// derived nodes (dup/transfer/attachment) can be descendants. The list is pruned on every use;
/// it holds no strong reference, so process exit and handle close drop their nodes normally.
static LINEAGE: Mutex<Vec<Weak<LineageNode>>> = Mutex::new(Vec::new());

/// Register a derived node for the next cross-process revocation walk.
pub fn register_lineage(node: Arc<LineageNode>) {
    let mut registry = LINEAGE.lock();
    registry.retain(|entry| entry.strong_count() > 0);
    registry.push(Arc::downgrade(&node));
}

/// Propagate `mask` from `target` to every live registered descendant.
///
/// The snapshot is taken under the registry lock and released before any node is touched, so a
/// node's own state (atomics/parent chain) is never modified while the registry lock is held.
pub fn revoke_descendants(target: &Arc<LineageNode>, mask: Rights) {
    let nodes: Vec<Arc<LineageNode>> = {
        let mut registry = LINEAGE.lock();
        registry.retain(|entry| entry.strong_count() > 0);
        registry.iter().filter_map(Weak::upgrade).collect()
    };
    for node in nodes {
        if !Arc::ptr_eq(&node, target) && node.is_descendant_of(target) {
            node.revoke(mask);
        }
    }
}

/// Live lineage nodes (S16 churn evidence).
pub fn lineage_nodes() -> u64 {
    raven_handles::live_lineage_nodes()
}

/// Install the well-known handles 0, 1 and 2 (and 3 for the first process) in a fresh process.
///
/// Every process gets `READ|MAP|EXEC` on the initfs; only the first process (the spawner) also
/// gets `TRANSFER` and the `RootDevice` capability at `Handle::ROOT_DEVICE`. The initfs is an
/// immutable archive of the system binaries, so read-exec is not a write or data-exfiltration
/// capability; the old PID-1-only `EXEC` restriction made any non-PID-1 shell unable to spawn.
pub fn install_initial(
    table: &mut Table,
    process: &Arc<Process>,
    initfs_exec: bool,
) -> Result<(), Errno> {
    table.insert(Object::Process(Arc::clone(process)), Rights::ALL)?;
    let mut initfs = Rights::READ.union(Rights::MAP).union(Rights::EXEC);
    if initfs_exec {
        initfs = initfs.union(Rights::TRANSFER);
    }
    table.insert(Object::MemObjectInitfs, initfs)?;
    table.insert(Object::Namespace, Rights::READ)?;
    if initfs_exec {
        let root = Rights::ADMIN.union(Rights::DUP).union(Rights::TRANSFER);
        table.insert(Object::RootDevice, root)?;
    }
    Ok(())
}
