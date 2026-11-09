// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Anonymous private address-space management.
//!
//! VMAs live in a `BTreeMap<start, Vma>`; adjacent areas with equal protections are merged.
//! Placement is first-fit top-down from `mmap_end`, matching the default Linux layout. File
//! mappings and shared memory are outside this shim layer.

use alloc::collections::BTreeMap;

use raven_abi::errno::Errno;
use raven_linux_abi::flags::mmap::{
    MAP_ANONYMOUS, MAP_FIXED, MAP_FIXED_NOREPLACE, MAP_SHARED, PROT_EXEC, PROT_NONE, PROT_READ,
    PROT_WRITE,
};

use crate::Host;

const PAGE: usize = 4096;
const MREMAP_MAYMOVE: u32 = 1;
const MREMAP_FIXED: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Vma {
    end: usize,
    prot: u32,
}

/// Address-space state for one Linux process.
pub struct Mm {
    mmap_base: usize,
    mmap_end: usize,
    brk_start: usize,
    brk_current: usize,
    vmas: BTreeMap<usize, Vma>,
}

impl Mm {
    /// Create an empty address space.
    ///
    /// `brk_start` is the initial program break; mappings are placed in
    /// `[mmap_base, mmap_end)`. Callers should pass page-aligned bounds.
    pub fn new(mmap_base: usize, mmap_end: usize, brk_start: usize) -> Self {
        Self {
            mmap_base,
            mmap_end,
            brk_start,
            brk_current: brk_start,
            vmas: BTreeMap::new(),
        }
    }

    /// Linux `mmap` for anonymous private memory.
    #[allow(clippy::too_many_arguments)] // Mirrors the Linux mmap(2) argument list.
    pub fn mmap(
        &mut self,
        host: &mut dyn Host,
        addr: usize,
        len: usize,
        prot: u32,
        flags: u32,
        fd: i32,
        off: usize,
    ) -> Result<usize, Errno> {
        if len == 0 {
            return Err(Errno::EINVAL);
        }
        let len = align_up(len).ok_or(Errno::EINVAL)?;
        if addr != 0 && !addr.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        if prot & (PROT_WRITE | PROT_EXEC) == PROT_WRITE | PROT_EXEC {
            return Err(Errno::EACCES);
        }
        if flags & MAP_ANONYMOUS == 0 {
            return Err(Errno::ENODEV);
        }
        let _ = (fd, off);
        if flags & MAP_SHARED != 0 {
            return Err(Errno::ENOSYS);
        }
        if flags & MAP_FIXED != 0 && flags & MAP_FIXED_NOREPLACE != 0 {
            return Err(Errno::EINVAL);
        }

        let fixed = flags & MAP_FIXED != 0;
        let noreplace = flags & MAP_FIXED_NOREPLACE != 0;
        let address = if fixed || noreplace {
            if addr == 0 {
                return Err(Errno::EINVAL);
            }
            let end = addr.checked_add(len).ok_or(Errno::EINVAL)?;
            if noreplace && !self.free(addr, end) {
                return Err(Errno::EEXIST);
            }
            if fixed {
                host.unmap(addr, len)?;
                self.remove_range(addr, end);
            }
            addr
        } else if addr != 0 {
            let end = addr.checked_add(len).ok_or(Errno::EINVAL)?;
            if self.free(addr, end) {
                addr
            } else {
                self.top_down(len).ok_or(Errno::ENOMEM)?
            }
        } else {
            self.top_down(len).ok_or(Errno::ENOMEM)?
        };

        let actual = host.map(address, len, prot, fixed || noreplace)?;
        if fixed && actual != address {
            return Err(Errno::EINVAL);
        }
        if !actual.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        let end = actual.checked_add(len).ok_or(Errno::EINVAL)?;
        if !self.free(actual, end) {
            return Err(Errno::EEXIST);
        }
        self.insert_vma(actual, end, prot);
        Ok(actual)
    }

    /// Linux `munmap`.
    pub fn munmap(&mut self, host: &mut dyn Host, addr: usize, len: usize) -> Result<(), Errno> {
        if len == 0 || !addr.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        let len = align_up(len).ok_or(Errno::EINVAL)?;
        let end = addr.checked_add(len).ok_or(Errno::EINVAL)?;
        host.unmap(addr, len)?;
        self.remove_range(addr, end);
        Ok(())
    }

    /// Linux `mprotect`.
    pub fn mprotect(
        &mut self,
        host: &mut dyn Host,
        addr: usize,
        len: usize,
        prot: u32,
    ) -> Result<(), Errno> {
        if len == 0 || !addr.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        let len = align_up(len).ok_or(Errno::EINVAL)?;
        let end = addr.checked_add(len).ok_or(Errno::EINVAL)?;
        if prot & (PROT_WRITE | PROT_EXEC) == PROT_WRITE | PROT_EXEC {
            return Err(Errno::EACCES);
        }
        if !self.covered(addr, end) {
            return Err(Errno::ENOMEM);
        }
        host.protect(addr, len, prot)?;
        self.set_prot_range(addr, end, prot);
        Ok(())
    }

    /// Linux `mremap` for the anonymous mappings owned by this table.
    ///
    /// `MREMAP_MAYMOVE` and `MREMAP_FIXED` are accepted. The `Host` contract has no page-copy
    /// primitive, so a move updates the VMA and native mapping but does not preserve contents;
    /// this limitation is documented in the crate-level deviations.
    pub fn mremap(
        &mut self,
        host: &mut dyn Host,
        old_addr: usize,
        old_len: usize,
        new_len: usize,
        flags: u32,
        new_addr: usize,
    ) -> Result<usize, Errno> {
        if !old_addr.is_multiple_of(PAGE)
            || old_len == 0
            || new_len == 0
            || flags & !(MREMAP_MAYMOVE | MREMAP_FIXED) != 0
        {
            return Err(Errno::EINVAL);
        }
        if flags & MREMAP_FIXED != 0 && flags & MREMAP_MAYMOVE == 0 {
            return Err(Errno::EINVAL);
        }
        let old_len = align_up(old_len).ok_or(Errno::EINVAL)?;
        let new_len = align_up(new_len).ok_or(Errno::EINVAL)?;
        let old_end = old_addr.checked_add(old_len).ok_or(Errno::EINVAL)?;
        let Some(prot) = self.protection_covering(old_addr, old_end) else {
            return Err(Errno::ENOMEM);
        };

        if flags & MREMAP_FIXED != 0 {
            if new_addr == 0 || !new_addr.is_multiple_of(PAGE) {
                return Err(Errno::EINVAL);
            }
            let target_end = new_addr.checked_add(new_len).ok_or(Errno::EINVAL)?;
            if !self.free(new_addr, target_end)
                || ranges_overlap(new_addr, target_end, old_addr, old_end)
            {
                return Err(Errno::ENOMEM);
            }
            host.map(new_addr, new_len, prot, true)?;
            host.unmap(old_addr, old_len)?;
            self.remove_range(old_addr, old_end);
            self.insert_vma(new_addr, target_end, prot);
            return Ok(new_addr);
        }

        if new_len <= old_len {
            if new_len == old_len {
                return Ok(old_addr);
            }
            let tail = old_addr.checked_add(new_len).ok_or(Errno::EINVAL)?;
            host.unmap(tail, old_len - new_len)?;
            self.remove_range(tail, old_end);
            return Ok(old_addr);
        }

        let new_end = old_end
            .checked_add(new_len - old_len)
            .ok_or(Errno::EINVAL)?;
        if new_end <= self.mmap_end && self.free(old_end, new_end) {
            host.map(old_end, new_len - old_len, prot, true)?;
            self.insert_vma(old_addr, new_end, prot);
            return Ok(old_addr);
        }

        if flags & MREMAP_MAYMOVE == 0 {
            return Err(Errno::ENOMEM);
        }
        let target = if flags & MREMAP_FIXED != 0 {
            if new_addr == 0 || !new_addr.is_multiple_of(PAGE) {
                return Err(Errno::EINVAL);
            }
            let target_end = new_addr.checked_add(new_len).ok_or(Errno::EINVAL)?;
            if !self.free(new_addr, target_end)
                || ranges_overlap(new_addr, target_end, old_addr, old_end)
            {
                return Err(Errno::ENOMEM);
            }
            new_addr
        } else {
            self.top_down(new_len).ok_or(Errno::ENOMEM)?
        };
        let target_end = target.checked_add(new_len).ok_or(Errno::EINVAL)?;
        host.map(target, new_len, prot, true)?;
        host.unmap(old_addr, old_len)?;
        self.remove_range(old_addr, old_end);
        self.insert_vma(target, target_end, prot);
        Ok(target)
    }

    /// Linux `brk`. Returns the current break on failure.
    pub fn brk(&mut self, host: &mut dyn Host, new: usize) -> usize {
        if new == 0 {
            return self.brk_current;
        }
        if new < self.brk_start || new > self.mmap_base {
            return self.brk_current;
        }
        if new == self.brk_current {
            return self.brk_current;
        }
        if new > self.brk_current {
            let old_end = align_up(self.brk_current).unwrap_or(self.brk_current);
            let new_end = align_up(new).unwrap_or(new);
            if new_end > self.mmap_end || !self.free(old_end, new_end) {
                return self.brk_current;
            }
            if old_end < new_end {
                if host
                    .map(old_end, new_end - old_end, PROT_READ | PROT_WRITE, true)
                    .is_err()
                {
                    return self.brk_current;
                }
                self.insert_vma(old_end, new_end, PROT_READ | PROT_WRITE);
            }
            self.brk_current = new;
            new
        } else {
            let old_end = align_up(self.brk_current).unwrap_or(self.brk_current);
            let new_end = align_up(new).unwrap_or(new);
            if new_end < old_end {
                if host.unmap(new_end, old_end - new_end).is_err() {
                    return self.brk_current;
                }
                self.remove_range(new_end, old_end);
            }
            self.brk_current = new;
            new
        }
    }

    fn top_down(&self, len: usize) -> Option<usize> {
        let mut candidate = align_down(self.mmap_end.checked_sub(len)?)?;
        while candidate >= self.mmap_base {
            if self.free(candidate, candidate.checked_add(len)?) {
                return Some(candidate);
            }
            candidate = candidate.checked_sub(PAGE)?;
        }
        None
    }

    fn free(&self, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }
        self.vmas
            .iter()
            .all(|(&vma_start, vma)| vma.end <= start || end <= vma_start)
    }

    fn covered(&self, start: usize, end: usize) -> bool {
        let mut cursor = start;
        for (&vma_start, vma) in &self.vmas {
            if vma.end <= cursor {
                continue;
            }
            if vma_start > cursor {
                break;
            }
            cursor = cursor.max(vma.end);
            if cursor >= end {
                return true;
            }
        }
        false
    }

    fn protection_covering(&self, start: usize, end: usize) -> Option<u32> {
        let mut cursor = start;
        let mut protection = None;
        for (&vma_start, vma) in &self.vmas {
            if vma.end <= cursor {
                continue;
            }
            if vma_start > cursor {
                return None;
            }
            if protection.is_none() {
                protection = Some(vma.prot);
            } else if protection != Some(vma.prot) {
                return None;
            }
            cursor = cursor.max(vma.end);
            if cursor >= end {
                return protection;
            }
        }
        None
    }

    fn insert_vma(&mut self, start: usize, end: usize, prot: u32) {
        if start >= end {
            return;
        }
        self.vmas.insert(start, Vma { end, prot });
        self.merge();
    }

    fn merge(&mut self) {
        let mut merged = BTreeMap::new();
        let mut current: Option<(usize, Vma)> = None;
        for (&start, &vma) in &self.vmas {
            if let Some((current_start, current_vma)) = current {
                if current_vma.end == start && current_vma.prot == vma.prot {
                    current = Some((
                        current_start,
                        Vma {
                            end: vma.end,
                            prot: vma.prot,
                        },
                    ));
                    continue;
                }
                merged.insert(current_start, current_vma);
            }
            current = Some((start, vma));
        }
        if let Some((start, vma)) = current {
            merged.insert(start, vma);
        }
        self.vmas = merged;
    }

    fn remove_range(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        let mut rebuilt = BTreeMap::new();
        for (&vma_start, &vma) in &self.vmas {
            if vma.end <= start || end <= vma_start {
                rebuilt.insert(vma_start, vma);
                continue;
            }
            if vma_start < start {
                rebuilt.insert(
                    vma_start,
                    Vma {
                        end: start,
                        prot: vma.prot,
                    },
                );
            }
            if end < vma.end {
                rebuilt.insert(
                    end,
                    Vma {
                        end: vma.end,
                        prot: vma.prot,
                    },
                );
            }
        }
        self.vmas = rebuilt;
    }

    fn set_prot_range(&mut self, start: usize, end: usize, prot: u32) {
        let mut rebuilt = BTreeMap::new();
        for (&vma_start, &vma) in &self.vmas {
            if vma.end <= start || end <= vma_start {
                rebuilt.insert(vma_start, vma);
                continue;
            }
            let overlap_start = vma_start.max(start);
            let overlap_end = vma.end.min(end);
            if vma_start < overlap_start {
                rebuilt.insert(
                    vma_start,
                    Vma {
                        end: overlap_start,
                        prot: vma.prot,
                    },
                );
            }
            rebuilt.insert(
                overlap_start,
                Vma {
                    end: overlap_end,
                    prot,
                },
            );
            if overlap_end < vma.end {
                rebuilt.insert(
                    overlap_end,
                    Vma {
                        end: vma.end,
                        prot: vma.prot,
                    },
                );
            }
        }
        self.vmas = rebuilt;
        self.merge();
    }
}

fn align_down(value: usize) -> Option<usize> {
    value.checked_sub(value % PAGE)
}

fn align_up(value: usize) -> Option<usize> {
    value.checked_add(PAGE - 1).map(|value| value & !(PAGE - 1))
}

fn ranges_overlap(start_a: usize, end_a: usize, start_b: usize, end_b: usize) -> bool {
    start_a < end_b && start_b < end_a
}

// Keep the named protection constant used in documentation/tests honest.
const _: () = assert!(PROT_NONE == 0);
