// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-process handle table with rights, generations and a rights-derivation tree.
//!
//! A [`Handle`] packs a 24-bit table index and an 8-bit generation. Closing a slot increments its
//! generation, so a stale handle fails with `EBADF` instead of touching a reused object. The
//! generation is exhausted after 255 closes: that slot is retired and never reused, so an early
//! stale handle can never become valid again. Rights are attached to the handle;
//! [`HandleTable::dup`] can only intersect them and [`HandleTable::get_with_rights`] returns the
//! stored mask without reconstructing it from a list of known bits.
//!
//! Every live handle also has a [`LineageNode`]. A node created by `dup`, `HANDLE_TRANSFER` or a
//! scheme reply attachment points at the node it was derived from; [`HandleTable::revoke_target`]
//! records revoked bits on a node and the kernel walks the table's nodes to propagate the same
//! mask to every descendant. A later lookup that needs a revoked bit returns `EPERM`, while a bit
//! that was never granted keeps the old `EACCES`. The node holds no process or address-space
//! reference, so S36 teardown is unaffected.
//!
//! The table is generic so the kernel can store its object enum while host tests use a small test
//! object.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};

/// Maximum number of handles in one process.
pub const MAX_HANDLES: usize = 65_536;

/// Live lineage nodes (the S16 churn evidence).  The counter is process-wide; the kernel prints
/// it at process exit and the qtest checks it returns to baseline.
static LIVE_NODES: AtomicU64 = AtomicU64::new(0);

/// Number of live lineage nodes.
pub fn live_lineage_nodes() -> u64 {
    LIVE_NODES.load(Ordering::Acquire)
}

/// One node in a handle's rights-derivation tree.
///
/// `rights` is the mask granted when the handle was created (already reduced by the parent and
/// the caller's mask); `revoked` is the bit mask revoked on this node. `revoked()` walks the
/// strong parent chain, so a completed ancestor revoke is visible even when this node was
/// registered in the kernel's weak registry after the revoke snapshot.
/// `effective() = rights & !revoked()`.
pub struct LineageNode {
    rights: Rights,
    revoked: AtomicU32,
    parent: Option<Arc<LineageNode>>,
}

impl LineageNode {
    /// Create a root node for a fresh object handle.
    pub fn root(rights: Rights) -> Arc<Self> {
        Self::new(rights, None)
    }

    /// Create a node derived from `parent`.
    pub fn child(rights: Rights, parent: &Arc<LineageNode>) -> Arc<Self> {
        Self::new(rights, Some(Arc::clone(parent)))
    }

    fn new(rights: Rights, parent: Option<Arc<LineageNode>>) -> Arc<Self> {
        LIVE_NODES.fetch_add(1, Ordering::Relaxed);
        Arc::new(Self {
            rights,
            revoked: AtomicU32::new(0),
            parent,
        })
    }

    /// Rights granted at creation, before any later revocation.
    pub fn granted(&self) -> Rights {
        self.rights
    }

    /// Rights revoked by this node or any ancestor, read through the strong parent chain.
    ///
    /// The chain is the linearization point: a revoke that completed before this call is seen
    /// even if this node was registered after the revoker's registry snapshot.
    pub fn revoked(&self) -> Rights {
        let mut revoked = Rights::NONE;
        let mut current = Some(self);
        while let Some(node) = current {
            revoked = revoked.union(Rights(node.revoked.load(Ordering::Acquire)));
            current = node.parent.as_deref();
        }
        revoked
    }

    /// Rights this node was granted at creation and that this node or an ancestor revoked.
    ///
    /// This is the C17 error-split mask: a right that was never granted to this handle is not
    /// here, so the lookup reports `EACCES`, while a granted right revoked anywhere in the chain
    /// reports `EPERM`.
    pub fn revoked_effective(&self) -> Rights {
        self.rights.intersect(self.revoked())
    }

    /// Rights still usable: granted minus the revocation chain.
    pub fn effective(&self) -> Rights {
        Rights(self.rights.0 & !self.revoked().0)
    }

    /// Revoke `mask` on this node.  OR-ing is idempotent.
    pub fn revoke(&self, mask: Rights) {
        self.revoked.fetch_or(mask.0, Ordering::AcqRel);
    }

    /// True when this node is a transitive descendant of `ancestor`.
    ///
    /// The parent chain is strong, so a closed intermediate handle does not break an ancestor's
    /// revoke path to its grandchildren; the node stays alive while descendants need it.
    pub fn is_descendant_of(&self, ancestor: &Arc<LineageNode>) -> bool {
        let mut current = self.parent.as_ref();
        while let Some(node) = current {
            if Arc::ptr_eq(node, ancestor) {
                return true;
            }
            current = node.parent.as_ref();
        }
        false
    }
}

impl Drop for LineageNode {
    fn drop(&mut self) {
        LIVE_NODES.fetch_sub(1, Ordering::Relaxed);
    }
}

struct Slot<T> {
    value: Option<T>,
    rights: Rights,
    generation: u8,
    /// A slot whose generation reached 255 after a close. Its stale handles must stay invalid
    /// forever, so `insert` never reuses it.
    retired: bool,
    /// Rights-derivation node; every inserted handle has one.
    lineage: Option<Arc<LineageNode>>,
}

impl<T> Slot<T> {
    const fn empty() -> Self {
        Self {
            value: None,
            rights: Rights::NONE,
            generation: 0,
            retired: false,
            lineage: None,
        }
    }
}

/// A per-process handle table.
pub struct HandleTable<T> {
    slots: Vec<Slot<T>>,
}

impl<T> HandleTable<T> {
    /// Create an empty table.
    pub const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Number of occupied slots.
    pub fn len(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.value.is_some())
            .count()
    }

    /// True when no slot is occupied.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Visit every live slot in table-index order.
    ///
    /// The visitor receives the exact handle (index and generation), the object
    /// reference, the usable rights and the slot's lineage node. Holes and
    /// retired slots are skipped. The call is allocation-free.
    pub fn for_each_live(&self, mut visit: impl FnMut(Handle, &T, Rights, &Arc<LineageNode>)) {
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.retired {
                continue;
            }
            let (Some(value), Some(lineage)) = (slot.value.as_ref(), slot.lineage.as_ref()) else {
                continue;
            };
            visit(
                Handle::new(index as u32, slot.generation),
                value,
                slot.rights,
                lineage,
            );
        }
    }
}

impl<T: Clone> HandleTable<T> {
    /// Insert an object with `rights`, returning its handle.
    ///
    /// The handle gets a fresh root lineage node.
    ///
    /// Free slots are reused in table order; a retired slot is skipped permanently.
    pub fn insert(&mut self, value: T, rights: Rights) -> Result<Handle, Errno> {
        let node = LineageNode::root(rights);
        let index = self.allocate_index()?;
        Ok(self.fill(index, value, rights, node))
    }

    /// Insert an object whose rights derive from `parent`.
    ///
    /// The child node keeps the **granted** mask: `parent.granted() & rights`, where callers
    /// pass the current effective mask or a queued handle's send-time mask. The slot stores the
    /// **currently effective** mask, `parent.effective() & granted`.
    ///
    /// This distinction is C17 for a queued `SCM_RIGHTS` handle: a right the sender held at send
    /// time stays granted on the installed child, so an ancestor revoke that completed before
    /// the install reports `EPERM`; a right the parent never had is absent from the granted mask
    /// and reports `EACCES`. The returned handle's node is available through
    /// [`Self::lineage_node`] for the kernel's cross-process revocation registry.
    pub fn insert_derived(
        &mut self,
        value: T,
        rights: Rights,
        parent: &Arc<LineageNode>,
    ) -> Result<Handle, Errno> {
        let granted = parent.granted().intersect(rights);
        let effective = parent.effective().intersect(granted);
        let node = LineageNode::child(granted, parent);
        let index = self.allocate_index()?;
        Ok(self.fill(index, value, effective, node))
    }

    /// Copy every slot of `other` into this empty table.
    ///
    /// Indices, generations, rights, retired flags and holes are preserved; each live object is
    /// cloned through `clone` and each live slot shares the exact `Arc<LineageNode>` with the
    /// source, so a later revocation is visible in both tables. On a clone error the destination
    /// is left unchanged.
    pub fn fork_into(
        &mut self,
        other: &HandleTable<T>,
        mut clone: impl FnMut(&T) -> Result<T, Errno>,
    ) -> Result<(), Errno> {
        if !self.is_empty() {
            return Err(Errno::EINVAL);
        }
        let mut slots = Vec::new();
        slots
            .try_reserve(other.slots.len())
            .map_err(|_| Errno::ENOMEM)?;
        for slot in &other.slots {
            let value = match slot.value.as_ref() {
                Some(value) => Some(clone(value)?),
                None => None,
            };
            slots.push(Slot {
                value,
                rights: slot.rights,
                generation: slot.generation,
                retired: slot.retired,
                lineage: slot.lineage.clone(),
            });
        }
        self.slots = slots;
        Ok(())
    }

    /// Look up `handle`, requiring every right in `needed`, and return the entry with the exact
    /// rights still usable on the slot.
    ///
    /// The rights mask is read from the slot and the lineage node, not reconstructed by probing a
    /// list of known bits, so every defined (and future) right round-trips without a mapping
    /// table. A missing right returns `EPERM` when it was revoked on this handle or an ancestor,
    /// and `EACCES` when it was never granted.
    pub fn get_with_rights(&self, handle: Handle, needed: Rights) -> Result<(&T, Rights), Errno> {
        let index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
        let slot = self.slots.get(index).ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() {
            return Err(Errno::EBADF);
        }
        let value = slot.value.as_ref().ok_or(Errno::EBADF)?;
        let (revoked, effective) = match slot.lineage.as_ref() {
            // The error split uses the rights this handle actually held, masked by the whole
            // revocation chain: an ancestor revoke is EPERM, a bit this handle never had is
            // EACCES even when an ancestor revoked the same bit.
            Some(node) => (node.revoked_effective(), node.effective()),
            None => (Rights::NONE, slot.rights),
        };
        if !effective.contains(needed) {
            return Err(if (revoked.0 & needed.0) != 0 {
                Errno::EPERM
            } else {
                Errno::EACCES
            });
        }
        Ok((value, effective))
    }

    /// Look up `handle`, requiring every right in `needed`.
    pub fn get(&self, handle: Handle, needed: Rights) -> Result<&T, Errno> {
        self.get_with_rights(handle, needed)
            .map(|(value, _rights)| value)
    }

    /// Rights still usable for `handle`.
    pub fn rights(&self, handle: Handle) -> Result<Rights, Errno> {
        self.get_with_rights(handle, Rights::NONE)
            .map(|(_value, rights)| rights)
    }

    /// The lineage node for a live `handle`.
    pub fn lineage_node(&self, handle: Handle) -> Result<Arc<LineageNode>, Errno> {
        let index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
        let slot = self.slots.get(index).ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() || slot.value.is_none() {
            return Err(Errno::EBADF);
        }
        slot.lineage.clone().ok_or(Errno::EBADF)
    }

    /// Revoke `mask` on `handle`'s lineage node and return the node and the applied mask.
    ///
    /// `mask == 0` means every right currently usable on the handle. Bits not held are ignored.
    /// The caller propagates the returned mask to descendants (the kernel walks its registry;
    /// host tests can call [`LineageNode::is_descendant_of`] on their own node set). The handle
    /// stays open.
    pub fn revoke_target(
        &mut self,
        handle: Handle,
        mask: Rights,
    ) -> Result<(Arc<LineageNode>, Rights), Errno> {
        let index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
        let slot = self.slots.get_mut(index).ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() || slot.value.is_none() {
            return Err(Errno::EBADF);
        }
        let node = slot.lineage.clone().ok_or(Errno::EBADF)?;
        let effective = node.effective();
        let applied = if mask.0 == 0 {
            effective
        } else {
            effective.intersect(mask)
        };
        node.revoke(applied);
        Ok((node, applied))
    }

    /// Close `handle`, invalidating its generation.
    ///
    /// Closing a slot at generation 255 retires it instead of wrapping to generation 0. The slot
    /// stays empty and unusable, so every handle issued for it before the retirement remains
    /// stale forever. The slot's lineage node is released; descendants keep their strong parent
    /// links and current revoked state.
    pub fn close(&mut self, handle: Handle) -> Result<(), Errno> {
        let index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
        let slot = self.slots.get_mut(index).ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() || slot.value.is_none() {
            return Err(Errno::EBADF);
        }
        slot.value = None;
        slot.rights = Rights::NONE;
        slot.lineage = None;
        if slot.generation == u8::MAX {
            slot.retired = true;
        } else {
            slot.generation += 1;
        }
        Ok(())
    }

    /// Duplicate `handle` with `rights` intersected with the effective rights.
    pub fn dup(&mut self, handle: Handle, rights: Rights) -> Result<Handle, Errno> {
        let (value, old_rights, parent) = {
            let slot_index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
            let slot = self.slots.get(slot_index).ok_or(Errno::EBADF)?;
            if slot.generation != handle.generation() {
                return Err(Errno::EBADF);
            }
            let value = slot.value.as_ref().ok_or(Errno::EBADF)?;
            // The node's effective mask is authoritative. `slot.rights` is the effective mask
            // recorded at insert time and can be stale for a queued child whose granted bit was
            // revoked before the install; the node still knows it was granted (EPERM), so do not
            // short-circuit on the slot mask.
            let parent = slot.lineage.clone().ok_or(Errno::EBADF)?;
            let effective = parent.effective();
            if !effective.contains(Rights::DUP) {
                return Err(if (parent.revoked_effective().0 & Rights::DUP.0) != 0 {
                    Errno::EPERM
                } else {
                    Errno::EACCES
                });
            }
            (value.clone(), effective, parent)
        };
        self.insert_derived(value, old_rights.intersect(rights), &parent)
    }

    fn allocate_index(&mut self) -> Result<usize, Errno> {
        let mut index = 0usize;
        while index < self.slots.len() {
            if let Some(slot) = self.slots.get(index)
                && slot.value.is_none()
                && !slot.retired
            {
                return Ok(index);
            }
            index = index.saturating_add(1);
        }
        if self.slots.len() >= MAX_HANDLES {
            return Err(Errno::EMFILE);
        }
        let index = self.slots.len();
        self.slots.push(Slot::empty());
        Ok(index)
    }

    fn fill(
        &mut self,
        index: usize,
        value: T,
        rights: Rights,
        lineage: Arc<LineageNode>,
    ) -> Handle {
        let slot = match self.slots.get_mut(index) {
            Some(slot) => slot,
            None => return Handle::NONE,
        };
        slot.value = Some(value);
        slot.rights = rights;
        slot.lineage = Some(lineage);
        Handle::new(index as u32, slot.generation)
    }
}

impl<T> Default for HandleTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
