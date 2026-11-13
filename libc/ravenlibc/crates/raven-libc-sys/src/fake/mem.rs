// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Anonymous memory and memory objects of the fake kernel.
//!
//! Mirrors `kernel/src/syscall/memory_ctl.rs:117-215` (`MAP`/`UNMAP`/`PROTECT`),
//! `memory_ctl.rs:38-75` (`MEMOBJ_CREATE`) and the fixed `raven-abi` layout ranges.

use raven_abi::errno::Errno;
use raven_abi::types::{map, prot};

use super::PAGE;

/// Fake user address space base; arena offset plus this constant.
pub const FAKE_BASE: usize = 0x0000_7000_0000_0000;
/// Number of pages in the fake address space (16 MiB).
pub const MAX_PAGES: usize = 4096;
/// Largest memory object the fake models (16 MiB); larger objects return `ENOSYS`.
pub const MAX_MEMOBJ: usize = MAX_PAGES * PAGE;

#[derive(Clone, Debug)]
struct MemObj {
    data: host::vec::Vec<u8>,
    pager: bool,
}

/// The memory model.
#[derive(Clone, Debug)]
pub struct Memory {
    /// Page -> owning mapping id.
    pages: host::vec::Vec<Option<usize>>,
    /// Page protection bits.
    protection: host::vec::Vec<u8>,
    /// Backing bytes (zero-filled).
    arena: host::vec::Vec<u8>,
    /// Memory objects by id.
    objects: host::vec::Vec<MemObj>,
    /// Live page count per mapping id.
    lives: host::vec::Vec<usize>,
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

impl Memory {
    /// An empty address space.
    pub fn new() -> Self {
        Self {
            pages: host::vec![None; MAX_PAGES],
            protection: host::vec![0; MAX_PAGES],
            arena: host::vec![0; MAX_PAGES * PAGE],
            objects: host::vec::Vec::new(),
            lives: host::vec::Vec::new(),
        }
    }

    /// Allocate a zero-filled memory object.
    pub fn memobj_create(&mut self, len: usize, flags: usize) -> Result<usize, Errno> {
        if flags & !raven_abi::types::memobj::PAGER != 0 || len == 0 || !len.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        if len > MAX_MEMOBJ {
            return Err(Errno::ENOSYS);
        }
        let id = self.objects.len();
        self.objects.push(MemObj {
            data: host::vec![0; len],
            pager: flags & raven_abi::types::memobj::PAGER != 0,
        });
        Ok(id)
    }

    /// Object length, if the id is valid.
    pub fn memobj_len(&self, id: usize) -> Option<usize> {
        self.objects.get(id).map(|object| object.data.len())
    }

    /// Map anonymous memory or a memory object; returns the fake address.
    pub fn map(
        &mut self,
        memobj: Option<usize>,
        offset: usize,
        addr: usize,
        len: usize,
        protection: u8,
        flags: usize,
    ) -> Result<usize, Errno> {
        if len == 0 || !len.is_multiple_of(PAGE) || !addr.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        if protection & !(prot::MASK as u8) != 0
            || (protection & (prot::WRITE as u8) != 0 && protection & (prot::EXEC as u8) != 0)
        {
            return Err(Errno::EINVAL);
        }
        let fixed = flags & map::FIXED != 0;
        let pages = len / PAGE;
        if pages > MAX_PAGES {
            return Err(Errno::ENOMEM);
        }
        if let Some(id) = memobj {
            let object = self.objects.get(id).ok_or(Errno::EBADF)?;
            let end = offset.checked_add(len).ok_or(Errno::EINVAL)?;
            if !offset.is_multiple_of(PAGE) || end > object.data.len() {
                return Err(Errno::EINVAL);
            }
        }

        let start = if fixed {
            let page = page_of(addr)?;
            let end = page.checked_add(pages).ok_or(Errno::ENOMEM)?;
            if end > MAX_PAGES {
                return Err(Errno::EINVAL);
            }
            if self
                .pages
                .get(page..end)
                .is_none_or(|slice| slice.iter().any(Option::is_some))
            {
                return Err(Errno::EEXIST);
            }
            page
        } else {
            let hint = if addr == 0 {
                1
            } else {
                page_of(addr).unwrap_or(1)
            };
            match self.find_free(start_from(hint, pages), pages) {
                Some(page) => page,
                None => match self.find_free(1, pages) {
                    Some(page) => page,
                    None => return Err(Errno::ENOMEM),
                },
            }
        };

        let id = self.lives.len();
        self.lives.push(pages);
        for page in start..start + pages {
            if let Some(slot) = self.pages.get_mut(page) {
                *slot = Some(id);
            }
            if let Some(slot) = self.protection.get_mut(page) {
                *slot = protection;
            }
        }

        if let Some(object_id) = memobj
            && let Some(object) = self.objects.get(object_id)
            && let Some(destination) = self.arena.get_mut(start * PAGE..(start + pages) * PAGE)
        {
            let source = object
                .data
                .get(offset..offset + pages * PAGE)
                .ok_or(Errno::EINVAL)?;
            destination.copy_from_slice(source);
        }
        Ok(FAKE_BASE + start * PAGE)
    }

    fn find_free(&self, start: usize, pages: usize) -> Option<usize> {
        if pages == 0 || pages > MAX_PAGES {
            return None;
        }
        let mut page = start;
        while page + pages <= MAX_PAGES {
            let slice = self.pages.get(page..page + pages)?;
            if slice.iter().all(Option::is_none) {
                return Some(page);
            }
            page += 1;
        }
        None
    }

    /// Unmap a range; partial ranges are supported by the page map.
    pub fn unmap(&mut self, addr: usize, len: usize) -> Result<(), Errno> {
        if len == 0 || !len.is_multiple_of(PAGE) || !addr.is_multiple_of(PAGE) {
            return Err(Errno::EINVAL);
        }
        let start = page_of(addr)?;
        let pages = len / PAGE;
        if start + pages > MAX_PAGES {
            return Err(Errno::EINVAL);
        }
        for page in start..start + pages {
            if let Some(slot) = self.pages.get_mut(page)
                && let Some(id) = slot.take()
                && let Some(life) = self.lives.get_mut(id)
            {
                *life = life.saturating_sub(1);
            }
            if let Some(slot) = self.protection.get_mut(page) {
                *slot = 0;
            }
            if let Some(bytes) = self.arena.get_mut(page * PAGE..(page + 1) * PAGE) {
                bytes.fill(0);
            }
        }
        Ok(())
    }

    /// Change protection of a mapped range.
    pub fn protect(&mut self, addr: usize, len: usize, protection: u8) -> Result<(), Errno> {
        if len == 0
            || !len.is_multiple_of(PAGE)
            || !addr.is_multiple_of(PAGE)
            || protection & !(prot::MASK as u8) != 0
            || (protection & (prot::WRITE as u8) != 0 && protection & (prot::EXEC as u8) != 0)
        {
            return Err(Errno::EINVAL);
        }
        let start = page_of(addr)?;
        let pages = len / PAGE;
        let slice = self.pages.get(start..start + pages).ok_or(Errno::EINVAL)?;
        if slice.iter().any(Option::is_none) {
            return Err(Errno::ENOMEM);
        }
        for page in start..start + pages {
            if let Some(slot) = self.protection.get_mut(page) {
                *slot = protection;
            }
        }
        Ok(())
    }

    /// Supply one page to a pager-backed object (C18).
    pub fn pager_supply(&mut self, id: usize, offset: usize, data: &[u8]) -> Result<(), Errno> {
        let object = self.objects.get_mut(id).ok_or(Errno::EBADF)?;
        if !object.pager
            || data.is_empty()
            || data.len() > PAGE
            || !offset.is_multiple_of(PAGE)
            || offset + PAGE > object.data.len()
        {
            return Err(Errno::EINVAL);
        }
        if let Some(destination) = object.data.get_mut(offset..offset + data.len()) {
            destination.copy_from_slice(data);
        }
        Ok(())
    }

    /// Number of mapped pages.
    pub fn mapped_pages(&self) -> usize {
        self.pages.iter().filter(|page| page.is_some()).count()
    }

    /// Copy out of the fake arena.
    pub fn peek(&self, addr: usize, len: usize) -> Option<host::vec::Vec<u8>> {
        let start = addr.checked_sub(FAKE_BASE)?;
        if !start.is_multiple_of(PAGE) || len == 0 {
            return None;
        }
        let end = start.checked_add(len)?;
        let slice = self.arena.get(start..end)?;
        let first = start / PAGE;
        let pages = len.div_ceil(PAGE);
        let owners = self.pages.get(first..first + pages)?;
        if owners.iter().any(Option::is_none) {
            return None;
        }
        Some(slice.to_vec())
    }

    /// Write into the fake arena.
    pub fn poke(&mut self, addr: usize, data: &[u8]) -> bool {
        let Some(start) = addr.checked_sub(FAKE_BASE) else {
            return false;
        };
        if !start.is_multiple_of(PAGE) {
            return false;
        }
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        let first = start / PAGE;
        let pages = data.len().div_ceil(PAGE).max(1);
        if self
            .pages
            .get(first..first + pages)
            .is_none_or(|owners| owners.iter().any(Option::is_none))
        {
            return false;
        }
        let Some(destination) = self.arena.get_mut(start..end) else {
            return false;
        };
        destination.copy_from_slice(data);
        true
    }

    /// Protection bits of the page containing `addr`.
    pub fn protection_at(&self, addr: usize) -> Option<u8> {
        let page = page_of(addr).ok()?;
        self.pages.get(page)?.as_ref()?;
        self.protection.get(page).copied()
    }
}

fn page_of(addr: usize) -> Result<usize, Errno> {
    let offset = addr.checked_sub(FAKE_BASE).ok_or(Errno::EINVAL)?;
    if !offset.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let page = offset / PAGE;
    if page >= MAX_PAGES {
        return Err(Errno::EINVAL);
    }
    Ok(page)
}

fn start_from(hint: usize, pages: usize) -> usize {
    if hint == 0 || hint + pages > MAX_PAGES {
        1
    } else {
        hint
    }
}
