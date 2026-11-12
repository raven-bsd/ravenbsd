// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure pager state and packet model.
//!
//! The kernel owns the frames and the page tables; this crate owns the page state machine and the
//! bounded `PageIn`/`PageOut` payload that the kernel and the pager server exchange over the
//! existing scheme `CALL` opcode. It has no kernel or allocator dependencies beyond `alloc`.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;

/// Page size used by the model.
pub const PAGE_SIZE: usize = 4096;
/// `PagerPacket` wire size.
pub const PACKET_SIZE: usize = 32;
/// Pager packet magic (`P17\0`).
pub const MAGIC: u32 = 0x5031_3700;
/// `PagerPacket` kind: kernel asks for one page.
pub const PAGE_IN: u16 = 1;
/// `PagerPacket` kind: kernel asks the pager to write one dirty page back.
pub const PAGE_OUT: u16 = 2;

/// State of one page in a pager-backed object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageState {
    /// No frame; the first fault must ask the pager.
    Absent,
    /// A PageIn request is in flight; further faults wait on the page queue.
    InFlight,
    /// The frame is present and clean.
    Present,
    /// The frame is present and needs writeback.
    Dirty,
    /// A PageOut request is in flight.
    Writing,
    /// The pager failed the page; waiters are woken with EIO.
    Failed,
}

/// What a fault should do after [`PagerObject::begin_page_in`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageIn {
    /// The page is (or became) usable; map it.
    Present,
    /// Another fault already sent the request; wait on the page queue.
    Wait,
    /// This fault owns the request; send one PageIn and wait.
    Send,
    /// The page failed; report `EIO`.
    Failed,
}

/// What an unmap/writeback path should do after [`PagerObject::begin_page_out`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageOut {
    /// Nothing to write back.
    Clean,
    /// A writeback is already in flight; wait for it.
    Wait,
    /// This caller owns the writeback request.
    Send,
}

/// One pager-backed object's page table.
pub struct PagerObject {
    pages: Vec<PageState>,
}

impl PagerObject {
    /// Create an object with `pages` absent pages.
    pub fn new(pages: usize) -> Option<Self> {
        if pages == 0 {
            return None;
        }
        let mut states = Vec::new();
        states.try_reserve(pages).ok()?;
        states.resize(pages, PageState::Absent);
        Some(Self { pages: states })
    }

    /// Create an eager object whose pages are all present (anonymous `MEMOBJ_CREATE`).
    pub fn present(pages: usize) -> Option<Self> {
        let mut object = Self::new(pages)?;
        for state in &mut object.pages {
            *state = PageState::Present;
        }
        Some(object)
    }

    /// Number of pages.
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// True when the object has no pages (never constructed by the kernel).
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// State of `page`, or `None` for an out-of-range index.
    pub fn state(&self, page: usize) -> Option<PageState> {
        self.pages.get(page).copied()
    }

    /// True when the page can be mapped now (present/dirty/writing).
    pub fn ready(&self, page: usize) -> bool {
        matches!(
            self.state(page),
            Some(PageState::Present | PageState::Dirty | PageState::Writing)
        )
    }

    /// Mark a fault's page. The first fault becomes the PageIn owner; later faults wait.
    pub fn begin_page_in(&mut self, page: usize) -> PageIn {
        let Some(state) = self.pages.get_mut(page) else {
            return PageIn::Failed;
        };
        match *state {
            PageState::Present | PageState::Dirty | PageState::Writing => PageIn::Present,
            PageState::InFlight => PageIn::Wait,
            PageState::Failed => PageIn::Failed,
            PageState::Absent => {
                *state = PageState::InFlight;
                PageIn::Send
            }
        }
    }

    /// Complete a supply. Only an absent/in-flight page can be supplied; a late supply after a
    /// failure is ignored so a failed fault cannot be revived by a racing pager.
    pub fn supply(&mut self, page: usize) -> bool {
        let Some(state) = self.pages.get_mut(page) else {
            return false;
        };
        if matches!(*state, PageState::Absent | PageState::InFlight) {
            *state = PageState::Present;
            true
        } else {
            false
        }
    }

    /// Fail a page; every waiter will observe `Failed` and report `EIO`.
    pub fn fail(&mut self, page: usize) -> bool {
        let Some(state) = self.pages.get_mut(page) else {
            return false;
        };
        if matches!(*state, PageState::Absent | PageState::InFlight) {
            *state = PageState::Failed;
            true
        } else {
            false
        }
    }

    /// Mark a present page dirty. A write during a PageOut keeps the page dirty: the current
    /// writeback may have captured older bytes.
    pub fn mark_dirty(&mut self, page: usize) -> bool {
        let Some(state) = self.pages.get_mut(page) else {
            return false;
        };
        match *state {
            PageState::Present | PageState::Writing => {
                *state = PageState::Dirty;
                true
            }
            PageState::Dirty => true,
            _ => false,
        }
    }

    /// Start writeback of a dirty page.
    pub fn begin_page_out(&mut self, page: usize) -> PageOut {
        let Some(state) = self.pages.get_mut(page) else {
            return PageOut::Clean;
        };
        match *state {
            PageState::Present | PageState::Absent | PageState::Failed => PageOut::Clean,
            PageState::Writing => PageOut::Wait,
            PageState::Dirty => {
                *state = PageState::Writing;
                PageOut::Send
            }
            PageState::InFlight => PageOut::Wait,
        }
    }

    /// Complete a writeback. A write that raced the writeback leaves the page dirty.
    pub fn page_out_done(&mut self, page: usize) -> bool {
        let Some(state) = self.pages.get_mut(page) else {
            return false;
        };
        if *state == PageState::Writing {
            *state = PageState::Present;
            true
        } else {
            false
        }
    }

    /// A failed writeback leaves the page dirty for a later retry.
    pub fn page_out_fail(&mut self, page: usize) -> bool {
        let Some(state) = self.pages.get_mut(page) else {
            return false;
        };
        if *state == PageState::Writing {
            *state = PageState::Dirty;
            true
        } else {
            false
        }
    }

    /// Number of dirty pages.
    pub fn dirty_pages(&self) -> usize {
        self.pages
            .iter()
            .filter(|state| **state == PageState::Dirty)
            .count()
    }

    /// Number of pages with a PageIn or PageOut request in flight.
    pub fn in_flight(&self) -> usize {
        self.pages
            .iter()
            .filter(|state| matches!(state, PageState::InFlight | PageState::Writing))
            .count()
    }
}

/// Why a `PagerPacket` was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketError {
    /// The buffer is shorter than [`PACKET_SIZE`].
    TooShort,
    /// The magic does not match [`MAGIC`].
    BadMagic,
    /// The kind is not `PAGE_IN` or `PAGE_OUT`.
    BadKind,
    /// A reserved field is non-zero.
    BadReserved,
    /// The offset is not page-aligned, or the length is not one page.
    BadRange,
}

/// The bounded PageIn/PageOut payload carried in a scheme `CALL` packet's inline data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagerPacket {
    /// [`MAGIC`].
    pub magic: u32,
    /// [`PAGE_IN`] or [`PAGE_OUT`].
    pub kind: u16,
    /// Must be zero.
    pub reserved: u16,
    /// The pager's object handle value (its key from `MEMOBJ_CREATE`).
    pub object: u32,
    /// Must be zero.
    pub reserved2: u32,
    /// Byte offset inside the object (page-aligned).
    pub offset: u64,
    /// Length in bytes (one page).
    pub len: u64,
}

impl PagerPacket {
    /// Build a PageIn request for `page * PAGE_SIZE`.
    pub fn page_in(object: u32, offset: u64) -> Self {
        Self {
            magic: MAGIC,
            kind: PAGE_IN,
            reserved: 0,
            object,
            reserved2: 0,
            offset,
            len: PAGE_SIZE as u64,
        }
    }

    /// Build a PageOut request for `page * PAGE_SIZE`.
    pub fn page_out(object: u32, offset: u64) -> Self {
        Self {
            magic: MAGIC,
            kind: PAGE_OUT,
            reserved: 0,
            object,
            reserved2: 0,
            offset,
            len: PAGE_SIZE as u64,
        }
    }

    /// Encode into `output`; the first [`PACKET_SIZE`] bytes are written.
    pub fn encode(&self, output: &mut [u8]) -> Result<(), PacketError> {
        if output.len() < PACKET_SIZE {
            return Err(PacketError::TooShort);
        }
        output[0..4].copy_from_slice(&self.magic.to_le_bytes());
        output[4..6].copy_from_slice(&self.kind.to_le_bytes());
        output[6..8].copy_from_slice(&self.reserved.to_le_bytes());
        output[8..12].copy_from_slice(&self.object.to_le_bytes());
        output[12..16].copy_from_slice(&self.reserved2.to_le_bytes());
        output[16..24].copy_from_slice(&self.offset.to_le_bytes());
        output[24..32].copy_from_slice(&self.len.to_le_bytes());
        Ok(())
    }

    /// Decode and validate a packet.
    pub fn decode(input: &[u8]) -> Result<Self, PacketError> {
        if input.len() < PACKET_SIZE {
            return Err(PacketError::TooShort);
        }
        let magic = u32::from_le_bytes(input[0..4].try_into().map_err(|_| PacketError::TooShort)?);
        if magic != MAGIC {
            return Err(PacketError::BadMagic);
        }
        let kind = u16::from_le_bytes(input[4..6].try_into().map_err(|_| PacketError::TooShort)?);
        if kind != PAGE_IN && kind != PAGE_OUT {
            return Err(PacketError::BadKind);
        }
        let reserved =
            u16::from_le_bytes(input[6..8].try_into().map_err(|_| PacketError::TooShort)?);
        let object =
            u32::from_le_bytes(input[8..12].try_into().map_err(|_| PacketError::TooShort)?);
        let reserved2 = u32::from_le_bytes(
            input[12..16]
                .try_into()
                .map_err(|_| PacketError::TooShort)?,
        );
        let offset = u64::from_le_bytes(
            input[16..24]
                .try_into()
                .map_err(|_| PacketError::TooShort)?,
        );
        let len = u64::from_le_bytes(
            input[24..32]
                .try_into()
                .map_err(|_| PacketError::TooShort)?,
        );
        if reserved != 0 || reserved2 != 0 {
            return Err(PacketError::BadReserved);
        }
        if !offset.is_multiple_of(PAGE_SIZE as u64) || len != PAGE_SIZE as u64 {
            return Err(PacketError::BadRange);
        }
        Ok(Self {
            magic,
            kind,
            reserved,
            object,
            reserved2,
            offset,
            len,
        })
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
