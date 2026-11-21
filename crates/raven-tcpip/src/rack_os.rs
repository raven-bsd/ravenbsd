// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Interface map for the FreeBSD RACK port at donor revision
// 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/sys/mbuf.h        m_get, m_gethdr, m_getcl, m_free, m_freem, M_TRAILINGROOM, M_EXTPG
//   sys/kern/uipc_mbuf.c  m_adj, m_copym, m_copydata, m_length
//   sys/sys/sockbuf.h     sbavail, sbused, sbspace
//   sys/kern/uipc_sockbuf.c sbappendstream_locked, sbcut_locked, sbsndmbuf,
//                         sbsndptr_noadv, sbsndptr_adv, sbreserve_locked
//   sys/vm/uma.h, uma_core.c uma_zcreate/zalloc/zfree/zdestroy/set_max/get_max/get_cur
//   sys/kern/kern_sysctl.c sysctl_ctx_init/free, sysctl_handle_32; sys/sys/sysctl.h SYSCTL_ADD_*
//   sys/netinet/tcp_log_buf.c tcp_log_event, tcp_log_stackspecific, tcp_log_end_status,
//                         tcp_log_socket_option, tcp_log_flowend, tcp_bblogging_on
//   sys/sys/counter.h     counter_u64_alloc/free/add/zero
//   sys/sys/ktls.h        ktls_enable, ktls_send, ktls_ready, ktls_output_eagain,
//                         ktls_release_snd_tag, ktls_get_maxlen
//   sys/netinet/tcp_subr.c tcp_estimate_tls_overhead
//   sys/netinet/tcp_hpts.c tcp_hpts_insert, tcp_hpts_remove
//   sys/netinet/tcp_stacks/rack.c rack_get_pacing_len, rack_set_cc_pacing,
//                         rack_undo_cc_pacing, rack_remove_pacing
//   sys/netinet/tcp_var.h tcpcb/tcp_stacks fields used by rack.c
// BSD-2/3. This is a sans-IO software model of those interfaces for the RACK port; no kernel
// pointers, locks, UMA/SMR internals, callouts or hardware tags are copied.

//! OS context for the FreeBSD RACK transport port.
//!
//! `RackOs` groups the FreeBSD interfaces `rack.c` needs: mbuf chains, socket-buffer accounting,
//! UMA zones, the sysctl tree, BTF/blackbox logging and counters, KTLS sessions, HPTS/pacing and
//! the `tcpcb` fields. Everything is software state driven by explicit calls; hardware KTLS tags
//! and the rate engine are simulated, never stubbed, and each donor interface has a test.

use alloc::borrow::ToOwned;
use alloc::collections::{BTreeSet, VecDeque};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

/// FreeBSD `MSIZE` in the model.
pub const MSIZE: usize = 256;
/// FreeBSD `MHLEN` in the model.
pub const MHLEN: usize = 224;
/// FreeBSD `MLEN` in the model.
pub const MLEN: usize = 240;
/// FreeBSD `MCLBYTES` in the model.
pub const MCLBYTES: usize = 2_048;
/// FreeBSD `M_PKTHDR`.
pub const M_PKTHDR: u32 = 0x0002;
/// FreeBSD `M_EXT`.
pub const M_EXT: u32 = 0x0001;
/// FreeBSD `M_EOR`; exposed for the RACK port, unused by the current model paths.
#[allow(dead_code)]
pub const M_EOR: u32 = 0x0004;
/// FreeBSD `M_EXTPG`.
pub const M_EXTPG: u32 = 0x0100;
/// FreeBSD `M_NOWAIT`.
pub const M_NOWAIT: u32 = 0x0001;
/// FreeBSD `M_WAITOK`; exposed for the RACK port, unused by the model allocator.
#[allow(dead_code)]
pub const M_WAITOK: u32 = 0x0002;

/// The maximum socket buffer the model accepts.
pub const SB_MAX: u32 = 2 * 1024 * 1024;

/// An mbuf handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MbufId(pub u32);

/// Why a `RackOs` operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackOsError {
    /// The mbuf/zone/TLS allocator has no capacity.
    NoMemory,
    /// The handle or node does not exist.
    NotFound,
    /// The handle is invalid or the buffer is too small.
    BadHandle,
    /// The socket buffer has no space.
    NoSpace,
    /// KTLS could not send because the tag/queue is busy.
    TlsEagain,
    /// A hardware KTLS session has no send tag.
    TlsNoTag,
    /// The KTLS session is not ready.
    TlsNotReady,
    /// The request exceeds the KTLS frame/limit.
    TlsTooLarge,
    /// A donor assertion/panic path was reached.
    Assert,
    /// A lock contract was violated.
    Locked,
}

impl fmt::Display for RackOsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RackOsError::NoMemory => f.write_str("out of memory"),
            RackOsError::NotFound => f.write_str("not found"),
            RackOsError::BadHandle => f.write_str("bad handle"),
            RackOsError::NoSpace => f.write_str("no socket-buffer space"),
            RackOsError::TlsEagain => f.write_str("ktls would block"),
            RackOsError::TlsNoTag => f.write_str("ktls hardware send tag missing"),
            RackOsError::TlsNotReady => f.write_str("ktls session is not ready"),
            RackOsError::TlsTooLarge => f.write_str("ktls request too large"),
            RackOsError::Assert => f.write_str("rack assertion"),
            RackOsError::Locked => f.write_str("lock contract violated"),
        }
    }
}

/// One mbuf in the software chain.
#[derive(Clone, Debug)]
pub struct Mbuf {
    /// Storage; `off..off + len` is the live region.
    pub data: Vec<u8>,
    /// Start of the live region.
    pub off: usize,
    /// Live bytes.
    pub len: usize,
    /// FreeBSD `m_flags`.
    pub flags: u32,
    /// Chain link.
    pub next: Option<MbufId>,
    /// `m_epg_tls`/`sb_tls_info` association for the model.
    pub tls: Option<u32>,
}

fn mbuf_mem(mbuf: &Mbuf) -> u32 {
    mbuf.donor_mem()
}

impl Mbuf {
    fn capacity(&self) -> usize {
        self.data.len()
    }

    fn donor_mem(&self) -> u32 {
        if self.flags & (M_EXT | M_EXTPG) != 0 {
            u32::try_from(MCLBYTES).unwrap_or(u32::MAX)
        } else {
            u32::try_from(MSIZE).unwrap_or(u32::MAX)
        }
    }

    fn trailing_room(&self) -> usize {
        self.capacity().saturating_sub(self.off + self.len)
    }

    /// Live bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.data[self.off..self.off + self.len]
    }
}

/// A software mbuf allocator and chain.
pub struct MbufPool {
    slots: Vec<Option<Mbuf>>,
    free: Vec<MbufId>,
    memcnt: u64,
}

impl Default for MbufPool {
    fn default() -> Self {
        Self::new()
    }
}

impl MbufPool {
    /// Create an empty pool.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            memcnt: 0,
        }
    }

    fn allocate(&mut self, capacity: usize, flags: u32) -> Option<MbufId> {
        let mbuf = Mbuf {
            data: vec![0u8; capacity],
            off: 0,
            len: 0,
            flags,
            next: None,
            tls: None,
        };
        self.memcnt = self.memcnt.saturating_add(u64::from(mbuf_mem(&mbuf)));
        let id = if let Some(id) = self.free.pop() {
            self.slots[id.0 as usize] = Some(mbuf);
            id
        } else {
            let id = MbufId(u32::try_from(self.slots.len()).ok()?);
            self.slots.push(Some(mbuf));
            id
        };
        Some(id)
    }

    /// FreeBSD `m_get`: one mbuf with `MLEN` storage.
    pub fn m_get(&mut self, _how: u32) -> Option<MbufId> {
        self.allocate(MLEN, 0)
    }

    /// FreeBSD `m_gethdr`: one packet-header mbuf with `MHLEN` storage.
    pub fn m_gethdr(&mut self, _how: u32) -> Option<MbufId> {
        self.allocate(MHLEN, M_PKTHDR)
    }

    /// FreeBSD `m_getcl`: one cluster-backed mbuf with `MCLBYTES` storage.
    pub fn m_getcl(&mut self, _how: u32, flags: u32) -> Option<MbufId> {
        self.allocate(MCLBYTES, flags | M_EXT)
    }

    /// FreeBSD `m_free`: free one mbuf and return its old chain link.
    pub fn m_free(&mut self, id: MbufId) -> Option<MbufId> {
        let mbuf = self.slots.get_mut(id.0 as usize)?.take()?;
        self.memcnt = self.memcnt.saturating_sub(u64::from(mbuf_mem(&mbuf)));
        self.free.push(id);
        mbuf.next
    }

    /// The donor mbuf memory charge: `MSIZE`, or `MCLBYTES` for an external/cluster mbuf.
    #[must_use]
    pub fn mbuf_mem(&self, id: MbufId) -> u32 {
        self.mbuf(id).map_or(0, mbuf_mem)
    }

    /// FreeBSD `m_freem`: free the whole chain.
    pub fn m_freem(&mut self, mut chain: Option<MbufId>) {
        while let Some(id) = chain {
            chain = self.m_free(id);
        }
    }

    /// FreeBSD `m_length`: sum the live bytes.
    #[must_use]
    pub fn m_length(&self, mut chain: Option<MbufId>) -> usize {
        let mut length = 0;
        while let Some(id) = chain {
            let Some(mbuf) = self.mbuf(id) else {
                break;
            };
            length += mbuf.len;
            chain = mbuf.next;
        }
        length
    }

    /// FreeBSD `m_memcnt`: bytes held by allocated mbufs.
    #[must_use]
    pub const fn m_memcnt(&self) -> u64 {
        self.memcnt
    }

    /// FreeBSD `M_TRAILINGROOM` for one mbuf.
    #[must_use]
    pub fn m_trailingroom(&self, id: MbufId) -> usize {
        self.mbuf(id).map_or(0, Mbuf::trailing_room)
    }

    /// FreeBSD `m_adj`: consume from the front for a positive length, trim from the tail for a
    /// negative length. Zero-length mbufs are unlinked by the model. Returns the new chain head
    /// (FreeBSD updates the caller's `m` pointer).
    pub fn m_adj(&mut self, chain: Option<MbufId>, req_len: i32) -> Option<MbufId> {
        let head = chain?;
        if req_len >= 0 {
            self.adj_front(head, req_len as usize)
        } else {
            self.adj_tail(head, req_len.unsigned_abs() as usize)
        }
    }

    fn adj_front(&mut self, head: MbufId, mut req: usize) -> Option<MbufId> {
        let mut current = Some(head);
        let mut new_head = Some(head);
        while let Some(id) = current {
            let (next, len) = match self.mbuf(id) {
                Some(mbuf) => (mbuf.next, mbuf.len),
                None => break,
            };
            if req == 0 {
                break;
            }
            if req >= len {
                req -= len;
                current = next;
                new_head = next;
                self.m_free(id);
            } else {
                if let Some(mbuf) = self.mbuf_mut(id) {
                    mbuf.off += req;
                    mbuf.len -= req;
                }
                break;
            }
        }
        new_head
    }

    fn adj_tail(&mut self, head: MbufId, mut req: usize) -> Option<MbufId> {
        let mut ids = Vec::new();
        let mut current = Some(head);
        while let Some(id) = current {
            let Some(mbuf) = self.mbuf(id) else {
                break;
            };
            ids.push(id);
            current = mbuf.next;
        }
        for id in ids.into_iter().rev() {
            if req == 0 {
                break;
            }
            let Some(mbuf) = self.mbuf_mut(id) else {
                break;
            };
            if req >= mbuf.len {
                req -= mbuf.len;
                mbuf.len = 0;
            } else {
                mbuf.len -= req;
                req = 0;
            }
        }
        // Unlink and free the zero-length tail mbufs; track a new head if the whole chain
        // became empty.
        let mut current = Some(head);
        let mut new_head = Some(head);
        let mut previous: Option<MbufId> = None;
        while let Some(id) = current {
            let Some(mbuf) = self.mbuf(id) else {
                break;
            };
            let next = mbuf.next;
            let empty = mbuf.len == 0;
            if empty {
                if let Some(previous) = previous {
                    if let Some(parent) = self.mbuf_mut(previous) {
                        parent.next = next;
                    }
                } else {
                    new_head = next;
                }
                self.m_free(id);
            } else {
                previous = Some(id);
            }
            current = next;
        }
        new_head
    }

    /// FreeBSD `m_copydata`: copy `len` bytes from `off` into `out`.
    pub fn m_copydata(
        &self,
        chain: Option<MbufId>,
        off: usize,
        len: usize,
        out: &mut [u8],
    ) -> Result<usize, RackOsError> {
        if out.len() < len {
            return Err(RackOsError::BadHandle);
        }
        let mut skip = off;
        let mut copied = 0;
        let mut current = chain;
        while let Some(id) = current {
            let mbuf = self.mbuf(id).ok_or(RackOsError::BadHandle)?;
            if skip >= mbuf.len {
                skip -= mbuf.len;
                current = mbuf.next;
                continue;
            }
            let start = skip;
            let available = mbuf.len - start;
            let take = (len - copied).min(available);
            out[copied..copied + take]
                .copy_from_slice(&mbuf.data[mbuf.off + start..mbuf.off + start + take]);
            copied += take;
            skip = 0;
            if copied == len {
                break;
            }
            current = mbuf.next;
        }
        if copied != len {
            return Err(RackOsError::BadHandle);
        }
        Ok(copied)
    }

    /// FreeBSD `m_copym`: clone `len` bytes from `off` into a new chain.
    pub fn m_copym(&mut self, chain: Option<MbufId>, off: usize, len: usize) -> Option<MbufId> {
        if len == 0 {
            return None;
        }
        let mut skip = off;
        let mut remaining = len;
        let mut head: Option<MbufId> = None;
        let mut tail: Option<MbufId> = None;
        let mut current = chain;
        while let Some(id) = current {
            let (start, take, next) = {
                let mbuf = self.mbuf(id)?;
                if skip >= mbuf.len {
                    skip -= mbuf.len;
                    current = mbuf.next;
                    continue;
                }
                let start = skip;
                let available = mbuf.len - start;
                (start, remaining.min(available), mbuf.next)
            };
            let mut chunk = vec![0u8; take];
            self.m_copydata(Some(id), start, take, &mut chunk).ok()?;
            // Donor `m_copym` shares an external cluster for a large chunk; the software
            // model copies it into an external mbuf so the live length fits.
            let new_id = if take > MLEN {
                self.allocate(take, M_EXT)?
            } else {
                self.m_get(M_NOWAIT)?
            };
            if let Some(new) = self.mbuf_mut(new_id) {
                new.data[..take].copy_from_slice(&chunk);
                new.len = take;
            }
            if let Some(tail) = tail {
                if let Some(parent) = self.mbuf_mut(tail) {
                    parent.next = Some(new_id);
                }
            } else {
                head = Some(new_id);
            }
            tail = Some(new_id);
            remaining -= take;
            skip = 0;
            if remaining == 0 {
                break;
            }
            current = next;
        }
        if remaining != 0 {
            self.m_freem(head);
            None
        } else {
            head
        }
    }

    /// Read one mbuf.
    #[must_use]
    pub fn mbuf(&self, id: MbufId) -> Option<&Mbuf> {
        self.slots.get(id.0 as usize)?.as_ref()
    }

    /// Mutate one mbuf.
    pub fn mbuf_mut(&mut self, id: MbufId) -> Option<&mut Mbuf> {
        self.slots.get_mut(id.0 as usize)?.as_mut()
    }

    /// Set the live length, capped by the storage and the current offset.
    pub fn set_len(&mut self, id: MbufId, len: usize) -> bool {
        let Some(mbuf) = self.mbuf_mut(id) else {
            return false;
        };
        if mbuf.off + len > mbuf.data.len() {
            return false;
        }
        mbuf.len = len;
        true
    }

    /// Write bytes into the mbuf (model of `m_copyback`), extending the live length.
    pub fn write(&mut self, id: MbufId, offset: usize, bytes: &[u8]) -> bool {
        let Some(mbuf) = self.mbuf_mut(id) else {
            return false;
        };
        if offset + bytes.len() > mbuf.data.len() {
            return false;
        }
        mbuf.data[mbuf.off + offset..mbuf.off + offset + bytes.len()].copy_from_slice(bytes);
        if offset + bytes.len() > mbuf.len {
            mbuf.len = offset + bytes.len();
        }
        true
    }

    /// Associate an mbuf with a KTLS session (`m_epg_tls`).
    pub fn set_tls(&mut self, id: MbufId, tls: Option<u32>) -> bool {
        self.mbuf_mut(id).map(|mbuf| mbuf.tls = tls).is_some()
    }
}

/// A software socket buffer (`struct sockbuf`).
#[derive(Clone, Debug)]
pub struct SockBuf {
    /// `sb_hiwat`.
    pub hiwat: u32,
    /// `sb_mbmax`.
    pub mbmax: u32,
    /// `sb_ccc`: bytes in the buffer.
    pub ccc: u32,
    /// `sb_acc`: bytes available to read.
    pub acc: u32,
    /// `sb_mbcnt`: mbuf memory charged to the buffer.
    pub mbcnt: u32,
    /// First mbuf.
    pub head: Option<MbufId>,
    /// Last mbuf.
    pub tail: Option<MbufId>,
    /// `sb_sndptr`.
    pub sndptr: Option<MbufId>,
    /// `sb_sndptroff`.
    pub sndptroff: u32,
    /// `sb_tls_info`.
    pub tls_info: Option<u32>,
}

impl Default for SockBuf {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl SockBuf {
    /// Create a buffer with the FreeBSD `sb_hiwat`/`sb_mbmax` bounds.
    #[must_use]
    pub const fn new(hiwat: u32, mbmax: u32) -> Self {
        Self {
            hiwat,
            mbmax,
            ccc: 0,
            acc: 0,
            mbcnt: 0,
            head: None,
            tail: None,
            sndptr: None,
            sndptroff: 0,
            tls_info: None,
        }
    }

    /// FreeBSD `sbavail`: bytes available right now.
    #[must_use]
    pub const fn sbavail(&self) -> u32 {
        self.acc
    }

    /// FreeBSD `sbused`: bytes in the buffer.
    #[must_use]
    pub const fn sbused(&self) -> u32 {
        self.ccc
    }

    /// FreeBSD `sbspace`: the smaller of the byte and mbuf headroom.
    #[must_use]
    pub const fn sbspace(&self) -> i64 {
        let bleft = self.hiwat as i64 - self.ccc as i64;
        let mleft = self.mbmax as i64 - self.mbcnt as i64;
        if bleft < mleft { bleft } else { mleft }
    }

    /// FreeBSD `sbappendstream_locked`: append a chain.
    pub fn sbappendstream_locked(
        &mut self,
        pool: &mut MbufPool,
        chain: Option<MbufId>,
    ) -> Result<(), RackOsError> {
        let Some(chain) = chain else {
            return Ok(());
        };
        let mut last = None;
        let mut current = Some(chain);
        let mut bytes = 0u32;
        let mut mem = 0u32;
        while let Some(id) = current {
            let mbuf = pool.mbuf(id).ok_or(RackOsError::BadHandle)?;
            bytes = bytes.saturating_add(u32::try_from(mbuf.len).unwrap_or(u32::MAX));
            mem = mem.saturating_add(mbuf.donor_mem());
            last = Some(id);
            current = mbuf.next;
        }
        if let Some(tail) = self.tail {
            let parent = pool.mbuf_mut(tail).ok_or(RackOsError::BadHandle)?;
            parent.next = Some(chain);
        } else {
            self.head = Some(chain);
        }
        self.tail = last;
        self.ccc = self.ccc.saturating_add(bytes);
        self.acc = self.acc.saturating_add(bytes);
        self.mbcnt = self.mbcnt.saturating_add(mem);
        Ok(())
    }

    /// FreeBSD `sbcut_locked`: remove up to `len` bytes from the front and return the removed
    /// chain. A partially consumed mbuf is split into a new front mbuf.
    pub fn sbcut_locked(&mut self, pool: &mut MbufPool, len: usize) -> Option<MbufId> {
        if len == 0 || self.head.is_none() {
            return None;
        }
        let mut remaining = len;
        let mut removed_head: Option<MbufId> = None;
        let mut removed_tail: Option<MbufId> = None;
        let mut bytes = 0usize;
        let mut mem = 0u32;
        while let Some(head) = self.head {
            let (next, mbuf_len, mbuf_mem) = {
                let Some(mbuf) = pool.mbuf(head) else {
                    break;
                };
                (mbuf.next, mbuf.len, mbuf.donor_mem())
            };
            if remaining == 0 {
                break;
            }
            if mbuf_len <= remaining {
                // Detach the whole mbuf.
                self.head = next;
                if self.head.is_none() {
                    self.tail = None;
                }
                if let Some(mbuf) = pool.mbuf_mut(head) {
                    mbuf.next = None;
                }
                if let Some(tail) = removed_tail {
                    if let Some(parent) = pool.mbuf_mut(tail) {
                        parent.next = Some(head);
                    }
                } else {
                    removed_head = Some(head);
                }
                removed_tail = Some(head);
                bytes += mbuf_len;
                mem = mem.saturating_add(mbuf_mem);
                remaining -= mbuf_len;
            } else {
                // Split the front: copy the removed prefix into a new chain (FreeBSD
                // `m_split`/`m_copym`), keep the rest in place.
                let take = remaining;
                let Some(copy) = pool.m_copym(Some(head), 0, take) else {
                    break;
                };
                let mut copy_tail = copy;
                while let Some(next) = pool.mbuf(copy_tail).and_then(|mbuf| mbuf.next) {
                    copy_tail = next;
                }
                if let Some(mbuf) = pool.mbuf_mut(head) {
                    mbuf.off += take;
                    mbuf.len -= take;
                }
                if let Some(tail) = removed_tail {
                    if let Some(parent) = pool.mbuf_mut(tail) {
                        parent.next = Some(copy);
                    }
                } else {
                    removed_head = Some(copy);
                }
                removed_tail = Some(copy_tail);
                bytes += take;
                // The original mbuf stays in the buffer with a shorter live region; the
                // removed prefix is caller-owned and not charged to sb_mbcnt.
                remaining = 0;
            }
        }
        self.ccc = self
            .ccc
            .saturating_sub(u32::try_from(bytes).unwrap_or(u32::MAX));
        self.acc = self
            .acc
            .saturating_sub(u32::try_from(bytes).unwrap_or(u32::MAX));
        self.mbcnt = self.mbcnt.saturating_sub(mem);
        // The send pointer is a cache into the old chain; invalidate it on any cut.
        self.sndptr = None;
        self.sndptroff = 0;
        removed_head
    }

    /// FreeBSD `sbsndptr_noadv`: find the send mbuf/mbuf offset at byte `off`.
    pub fn sbsndptr_noadv(&mut self, pool: &MbufPool, off: u32) -> Option<(MbufId, u32)> {
        let (id, moff, start) = self.find_off(pool, off)?;
        self.sndptr = Some(id);
        self.sndptroff = start;
        Some((id, moff))
    }

    /// FreeBSD `sbsndmbuf`: find the send mbuf/mbuf offset at byte `off` without moving the cache.
    #[must_use]
    pub fn sbsndmbuf(&self, pool: &MbufPool, off: u32) -> Option<(MbufId, u32)> {
        self.find_off_readonly(pool, off)
    }

    /// FreeBSD `sbsndptr_adv`: advance the cached pointer by `len` bytes from `mb`.
    pub fn sbsndptr_adv(&mut self, pool: &MbufPool, mb: MbufId, len: u32) -> bool {
        if self.sndptr != Some(mb) {
            return false;
        }
        let mut current = Some(mb);
        let mut remaining = len;
        let mut consumed = 0u32;
        while let Some(id) = current {
            let Some(mbuf) = pool.mbuf(id) else {
                return false;
            };
            if remaining == 0 {
                self.sndptr = Some(id);
                self.sndptroff = self.sndptroff.saturating_add(consumed);
                return true;
            }
            let available = u32::try_from(mbuf.len).unwrap_or(u32::MAX);
            if remaining < available {
                self.sndptr = Some(id);
                self.sndptroff = self
                    .sndptroff
                    .saturating_add(consumed)
                    .saturating_add(remaining);
                return true;
            }
            remaining -= available;
            consumed = consumed.saturating_add(available);
            current = mbuf.next;
        }
        false
    }

    /// FreeBSD `sbreserve_locked`: reserve `cc` bytes for the buffer.
    pub fn sbreserve_locked(&mut self, cc: u32) -> bool {
        if cc > SB_MAX {
            return false;
        }
        self.hiwat = cc;
        self.mbmax = cc.saturating_mul(2);
        true
    }

    fn find_off(&mut self, pool: &MbufPool, off: u32) -> Option<(MbufId, u32, u32)> {
        let (start_id, start_off) = if let Some(ptr) = self.sndptr {
            if self.sndptroff <= off {
                (ptr, self.sndptroff)
            } else {
                (self.head?, 0)
            }
        } else {
            (self.head?, 0)
        };
        let mut current = Some(start_id);
        let mut base = start_off;
        while let Some(id) = current {
            let mbuf = pool.mbuf(id)?;
            let len = u32::try_from(mbuf.len).unwrap_or(u32::MAX);
            if off < base.saturating_add(len) {
                return Some((id, off - base, start_off));
            }
            base = base.saturating_add(len);
            current = mbuf.next;
        }
        None
    }

    fn find_off_readonly(&self, pool: &MbufPool, off: u32) -> Option<(MbufId, u32)> {
        let mut current = self.head;
        let mut base = 0u32;
        while let Some(id) = current {
            let mbuf = pool.mbuf(id)?;
            let len = u32::try_from(mbuf.len).unwrap_or(u32::MAX);
            if off < base.saturating_add(len) {
                return Some((id, off - base));
            }
            base = base.saturating_add(len);
            current = mbuf.next;
        }
        None
    }
}

/// One software UMA zone.
#[derive(Clone, Debug)]
pub struct UmaZone {
    /// Zone name.
    pub name: String,
    /// Item size.
    pub size: usize,
    /// Maximum items (`0` = unlimited).
    pub max: usize,
    /// Current items.
    pub cur: usize,
    /// Total allocations.
    pub allocations: u64,
    /// Failed allocations.
    pub failures: u64,
    items: BTreeSet<u64>,
}

/// The software UMA zone table.
#[derive(Default)]
pub struct Uma {
    zones: Vec<Option<UmaZone>>,
    next_item: u64,
}

impl Uma {
    /// Create an empty zone table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            zones: Vec::new(),
            next_item: 0,
        }
    }

    /// FreeBSD `uma_zcreate`.
    pub fn uma_zcreate(&mut self, name: &str, size: usize) -> u32 {
        let id = u32::try_from(self.zones.len()).unwrap_or(u32::MAX);
        self.zones.push(Some(UmaZone {
            name: name.to_owned(),
            size,
            max: 0,
            cur: 0,
            allocations: 0,
            failures: 0,
            items: BTreeSet::new(),
        }));
        id
    }

    /// FreeBSD `uma_zalloc`: one item, or `None` at the zone limit.
    pub fn uma_zalloc(&mut self, zone: u32) -> Option<u64> {
        let item = self.next_item;
        let zone = self.zones.get_mut(zone as usize)?.as_mut()?;
        if zone.max != 0 && zone.cur >= zone.max {
            zone.failures = zone.failures.saturating_add(1);
            return None;
        }
        zone.cur += 1;
        zone.allocations = zone.allocations.saturating_add(1);
        zone.items.insert(item);
        self.next_item = self.next_item.wrapping_add(1);
        Some(item)
    }

    /// FreeBSD `uma_zfree`: return one item.
    pub fn uma_zfree(&mut self, zone: u32, item: u64) -> bool {
        let Some(zone) = self.zones.get_mut(zone as usize).and_then(Option::as_mut) else {
            return false;
        };
        if !zone.items.remove(&item) {
            return false;
        }
        zone.cur = zone.cur.saturating_sub(1);
        true
    }

    /// FreeBSD `uma_zdestroy`.
    pub fn uma_zdestroy(&mut self, zone: u32) -> bool {
        let Some(slot) = self.zones.get_mut(zone as usize) else {
            return false;
        };
        slot.take().is_some()
    }

    /// FreeBSD `uma_zone_set_max`.
    pub fn uma_zone_set_max(&mut self, zone: u32, max: usize) -> bool {
        let Some(zone) = self.zones.get_mut(zone as usize).and_then(Option::as_mut) else {
            return false;
        };
        zone.max = max;
        true
    }

    /// FreeBSD `uma_zone_get_max`.
    #[must_use]
    pub fn uma_zone_get_max(&self, zone: u32) -> Option<usize> {
        self.zones.get(zone as usize)?.as_ref().map(|zone| zone.max)
    }

    /// FreeBSD `uma_zone_get_cur`.
    #[must_use]
    pub fn uma_zone_get_cur(&self, zone: u32) -> Option<usize> {
        self.zones.get(zone as usize)?.as_ref().map(|zone| zone.cur)
    }

    /// Zone metadata.
    #[must_use]
    pub fn zone(&self, zone: u32) -> Option<&UmaZone> {
        self.zones.get(zone as usize)?.as_ref()
    }
}

/// One sysctl value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SysctlValue {
    /// `uint8_t`.
    U8(u8),
    /// `uint16_t`.
    U16(u16),
    /// `uint32_t`.
    U32(u32),
    /// `uint64_t`.
    U64(u64),
    /// `int32_t`.
    S32(i32),
    /// A counter snapshot.
    Counter(u64),
    /// A string/proc node.
    Proc(String),
    /// A node with no value.
    Node,
}

/// One sysctl node.
#[derive(Clone, Debug)]
pub struct SysctlNode {
    /// Parent node, `None` for the root.
    pub parent: Option<u32>,
    /// Node name.
    pub name: String,
    /// Current value.
    pub value: SysctlValue,
    /// Creating context.
    pub ctx: u32,
}

/// The software sysctl tree and contexts.
#[derive(Default)]
pub struct SysctlTree {
    nodes: Vec<Option<SysctlNode>>,
    next_ctx: u32,
}

impl SysctlTree {
    /// Create an empty tree.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next_ctx: 1,
        }
    }

    /// FreeBSD `sysctl_ctx_init`.
    pub fn sysctl_ctx_init(&mut self) -> u32 {
        let ctx = self.next_ctx;
        self.next_ctx = self.next_ctx.saturating_add(1);
        ctx
    }

    fn add(&mut self, parent: Option<u32>, name: &str, value: SysctlValue, ctx: u32) -> u32 {
        let id = u32::try_from(self.nodes.len()).unwrap_or(u32::MAX);
        self.nodes.push(Some(SysctlNode {
            parent,
            name: name.to_owned(),
            value,
            ctx,
        }));
        id
    }

    /// FreeBSD `SYSCTL_ADD_NODE`.
    pub fn sysctl_add_node(&mut self, parent: Option<u32>, name: &str, ctx: u32) -> u32 {
        self.add(parent, name, SysctlValue::Node, ctx)
    }

    /// FreeBSD `SYSCTL_ADD_U8`.
    pub fn sysctl_add_u8(&mut self, parent: Option<u32>, name: &str, value: u8, ctx: u32) -> u32 {
        self.add(parent, name, SysctlValue::U8(value), ctx)
    }

    /// FreeBSD `SYSCTL_ADD_U16`.
    pub fn sysctl_add_u16(&mut self, parent: Option<u32>, name: &str, value: u16, ctx: u32) -> u32 {
        self.add(parent, name, SysctlValue::U16(value), ctx)
    }

    /// FreeBSD `SYSCTL_ADD_U32`.
    pub fn sysctl_add_u32(&mut self, parent: Option<u32>, name: &str, value: u32, ctx: u32) -> u32 {
        self.add(parent, name, SysctlValue::U32(value), ctx)
    }

    /// FreeBSD `SYSCTL_ADD_U64`.
    pub fn sysctl_add_u64(&mut self, parent: Option<u32>, name: &str, value: u64, ctx: u32) -> u32 {
        self.add(parent, name, SysctlValue::U64(value), ctx)
    }

    /// FreeBSD `SYSCTL_ADD_S32`.
    pub fn sysctl_add_s32(&mut self, parent: Option<u32>, name: &str, value: i32, ctx: u32) -> u32 {
        self.add(parent, name, SysctlValue::S32(value), ctx)
    }

    /// FreeBSD `SYSCTL_ADD_COUNTER_U64`.
    pub fn sysctl_add_counter(
        &mut self,
        parent: Option<u32>,
        name: &str,
        value: u64,
        ctx: u32,
    ) -> u32 {
        self.add(parent, name, SysctlValue::Counter(value), ctx)
    }

    /// FreeBSD `SYSCTL_ADD_PROC`.
    pub fn sysctl_add_proc(
        &mut self,
        parent: Option<u32>,
        name: &str,
        text: &str,
        ctx: u32,
    ) -> u32 {
        self.add(parent, name, SysctlValue::Proc(text.to_owned()), ctx)
    }

    /// FreeBSD `sysctl_ctx_free`: remove every node created by `ctx`.
    pub fn sysctl_ctx_free(&mut self, ctx: u32) -> usize {
        let mut removed = 0;
        for slot in &mut self.nodes {
            if slot.as_ref().is_some_and(|node| node.ctx == ctx) {
                *slot = None;
                removed += 1;
            }
        }
        removed
    }

    /// FreeBSD `sysctl_handle_32`: read a 32-bit value.
    #[must_use]
    pub fn sysctl_handle_32(&self, node: u32) -> Option<i64> {
        match self.nodes.get(node as usize)?.as_ref()?.value {
            SysctlValue::U8(value) => Some(i64::from(value)),
            SysctlValue::U16(value) => Some(i64::from(value)),
            SysctlValue::U32(value) => Some(i64::from(value)),
            SysctlValue::S32(value) => Some(i64::from(value)),
            SysctlValue::Counter(value) => i64::try_from(value).ok(),
            _ => None,
        }
    }

    /// Set a scalar node.
    pub fn sysctl_set_u32(&mut self, node: u32, value: u32) -> bool {
        let Some(node) = self.nodes.get_mut(node as usize).and_then(Option::as_mut) else {
            return false;
        };
        match &mut node.value {
            SysctlValue::U8(slot) => {
                *slot = u8::try_from(value).unwrap_or(u8::MAX);
            }
            SysctlValue::U16(slot) => {
                *slot = u16::try_from(value).unwrap_or(u16::MAX);
            }
            SysctlValue::U32(slot) => *slot = value,
            SysctlValue::U64(slot) => *slot = u64::from(value),
            SysctlValue::S32(slot) => *slot = i32::try_from(value).unwrap_or(i32::MAX),
            _ => return false,
        }
        true
    }

    /// Add to a counter node.
    pub fn sysctl_counter_add(&mut self, node: u32, delta: u64) -> bool {
        let Some(node) = self.nodes.get_mut(node as usize).and_then(Option::as_mut) else {
            return false;
        };
        if let SysctlValue::Counter(value) = &mut node.value {
            *value = value.saturating_add(delta);
            true
        } else {
            false
        }
    }

    /// `SYSCTL_OUT`: copy a proc string or scalar text into `out`.
    pub fn sysctl_out(&self, node: u32, out: &mut [u8]) -> Result<usize, RackOsError> {
        let node = self
            .nodes
            .get(node as usize)
            .and_then(Option::as_ref)
            .ok_or(RackOsError::NotFound)?;
        let text = match &node.value {
            SysctlValue::Proc(text) => text.clone(),
            SysctlValue::U8(value) => format!("{value}"),
            SysctlValue::U16(value) => format!("{value}"),
            SysctlValue::U32(value) => format!("{value}"),
            SysctlValue::U64(value) => format!("{value}"),
            SysctlValue::S32(value) => format!("{value}"),
            SysctlValue::Counter(value) => format!("{value}"),
            SysctlValue::Node => String::new(),
        };
        let bytes = text.as_bytes();
        let take = bytes.len().min(out.len());
        out[..take].copy_from_slice(&bytes[..take]);
        Ok(take)
    }

    /// The node name.
    #[must_use]
    pub fn sysctl_name(&self, node: u32) -> Option<&str> {
        self.nodes
            .get(node as usize)?
            .as_ref()
            .map(|node| node.name.as_str())
    }

    /// The children of a node (`SYSCTL_CHILDREN`).
    #[must_use]
    pub fn sysctl_children(&self, parent: Option<u32>) -> Vec<u32> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(id, slot)| {
                slot.as_ref()
                    .filter(|node| node.parent == parent)
                    .map(|_| u32::try_from(id).unwrap_or(u32::MAX))
            })
            .collect()
    }
}

/// One bounded BTF/blackbox event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcpLogEvent {
    /// Event type id.
    pub kind: u32,
    /// Four scalar arguments.
    pub args: [u32; 4],
    /// Status/error.
    pub status: i32,
    /// Whether this is a stack-specific event.
    pub stack: bool,
    /// Payload bytes represented by the event.
    pub bytes: u32,
}

/// A bounded software TCP blackbox log.
#[derive(Clone, Debug)]
pub struct TcpLog {
    enabled: bool,
    capacity: usize,
    events: VecDeque<TcpLogEvent>,
    dropped: u64,
}

impl TcpLog {
    /// Create a log with a maximum event count.
    #[must_use]
    pub fn new(enabled: bool, capacity: usize) -> Self {
        Self {
            enabled,
            capacity: capacity.max(1),
            events: VecDeque::new(),
            dropped: 0,
        }
    }

    /// FreeBSD `tcp_bblogging_on`.
    #[must_use]
    pub const fn tcp_bblogging_on(&self) -> bool {
        self.enabled
    }

    /// Enable or disable logging.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// FreeBSD `tcp_log_event`/`TCP_LOG_EVENTP`.
    pub fn tcp_log_event(&mut self, kind: u32, args: [u32; 4], status: i32) -> bool {
        self.push(TcpLogEvent {
            kind,
            args,
            status,
            stack: false,
            bytes: 0,
        })
    }

    /// FreeBSD `tcp_log_stackspecific`.
    pub fn tcp_log_stackspecific(&mut self, kind: u32, status: i32, bytes: u32) -> bool {
        self.push(TcpLogEvent {
            kind,
            args: [0; 4],
            status,
            stack: true,
            bytes,
        })
    }

    /// FreeBSD `tcp_log_end_status`.
    pub fn tcp_log_end_status(&mut self, status: i32) -> bool {
        self.tcp_log_event(0, [0; 4], status)
    }

    /// FreeBSD `tcp_log_socket_option`.
    pub fn tcp_log_socket_option(&mut self, option: u32, value: u32) -> bool {
        self.tcp_log_event(0, [option, value, 0, 0], 0)
    }

    /// FreeBSD `tcp_log_flowend`.
    pub fn tcp_log_flowend(&mut self) -> bool {
        self.tcp_log_event(0, [0; 4], 0)
    }

    /// FreeBSD `tcp_log_buffer`: number of retained events.
    #[must_use]
    pub fn tcp_log_buffer(&self) -> usize {
        self.events.len()
    }

    /// The retained events.
    #[must_use]
    pub fn events(&self) -> &VecDeque<TcpLogEvent> {
        &self.events
    }

    /// Events dropped because the ring was full.
    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    fn push(&mut self, event: TcpLogEvent) -> bool {
        if !self.enabled {
            return false;
        }
        if self.events.len() >= self.capacity {
            self.events.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.events.push_back(event);
        true
    }
}

/// A software `counter_u64_t`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CounterU64(u64);

impl CounterU64 {
    /// FreeBSD `counter_u64_add`.
    pub fn add(&mut self, delta: u64) {
        self.0 = self.0.wrapping_add(delta);
    }

    /// FreeBSD `counter_u64_zero`.
    pub fn zero(&mut self) {
        self.0 = 0;
    }

    /// The current value.
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.0
    }
}

/// One software KTLS session.
#[derive(Clone, Debug)]
pub struct TlsSession {
    /// Session id.
    pub id: u32,
    /// Hardware offload session (`t_nic_ktls_xmit`/`M_EXTPG` path).
    pub hardware: bool,
    /// `ktls_ready`.
    pub ready: bool,
    /// Per-record header + trailer overhead.
    pub overhead: u32,
    /// TLS record payload limit.
    pub max_frame: u32,
    /// Hardware send tag.
    pub snd_tag: Option<u32>,
    /// A simulated `EAGAIN` for the next send.
    pub output_eagain: bool,
    /// Records emitted.
    pub records: u64,
    /// Payload bytes emitted.
    pub payload: u64,
    /// Overhead bytes emitted.
    pub overhead_bytes: u64,
}

/// The software KTLS session table.
#[derive(Default)]
pub struct TlsContext {
    sessions: Vec<Option<TlsSession>>,
    next_id: u32,
}

impl TlsContext {
    /// Create an empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sessions: Vec::new(),
            next_id: 0,
        }
    }

    /// FreeBSD `ktls_enable_tx` (software or hardware).
    pub fn ktls_enable(&mut self, hardware: bool, overhead: u32, max_frame: u32) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.sessions.push(Some(TlsSession {
            id,
            hardware,
            ready: true,
            overhead,
            max_frame: max_frame.max(1),
            snd_tag: if hardware { None } else { Some(0) },
            output_eagain: false,
            records: 0,
            payload: 0,
            overhead_bytes: 0,
        }));
        id
    }

    /// FreeBSD `ktls_ready`.
    #[must_use]
    pub fn ktls_ready(&self, id: u32) -> bool {
        self.session(id).is_some_and(|session| session.ready)
    }

    /// FreeBSD `ktls_get_maxlen`.
    #[must_use]
    pub fn ktls_get_maxlen(&self, id: u32) -> u32 {
        self.session(id).map_or(0, |session| session.max_frame)
    }

    /// Attach a simulated hardware send tag.
    pub fn ktls_set_snd_tag(&mut self, id: u32, tag: u32) -> bool {
        let Some(session) = self.session_mut(id) else {
            return false;
        };
        session.snd_tag = Some(tag);
        true
    }

    /// FreeBSD `ktls_release_snd_tag`.
    pub fn ktls_release_snd_tag(&mut self, id: u32) -> bool {
        let Some(session) = self.session_mut(id) else {
            return false;
        };
        session.snd_tag = None;
        true
    }

    /// FreeBSD `ktls_output_eagain`: clear and report a simulated EAGAIN.
    pub fn ktls_output_eagain(&mut self, id: u32) -> bool {
        let Some(session) = self.session_mut(id) else {
            return false;
        };
        let eagain = session.output_eagain;
        session.output_eagain = false;
        eagain
    }

    /// FreeBSD `ktls_send`: software send, or a simulated hardware tag send.
    ///
    /// Returns the bytes added to the wire (payload + record overhead). A hardware session
    /// without a send tag returns [`RackOsError::TlsNoTag`]; a pending EAGAIN returns
    /// [`RackOsError::TlsEagain`]; a closed session returns [`RackOsError::TlsNotReady`].
    pub fn ktls_send(&mut self, id: u32, len: u32) -> Result<u32, RackOsError> {
        let Some(session) = self.session_mut(id) else {
            return Err(RackOsError::NotFound);
        };
        if !session.ready {
            return Err(RackOsError::TlsNotReady);
        }
        if session.hardware && session.snd_tag.is_none() {
            return Err(RackOsError::TlsNoTag);
        }
        if session.output_eagain {
            return Err(RackOsError::TlsEagain);
        }
        if len == 0 {
            return Ok(0);
        }
        let records = len.div_ceil(session.max_frame);
        let overhead = records.saturating_mul(session.overhead);
        session.records = session.records.saturating_add(u64::from(records));
        session.payload = session.payload.saturating_add(u64::from(len));
        session.overhead_bytes = session.overhead_bytes.saturating_add(u64::from(overhead));
        Ok(len.saturating_add(overhead))
    }

    /// FreeBSD `tcp_estimate_tls_overhead`: `ceil(bytes / max_frame) * overhead`.
    #[must_use]
    pub fn tcp_estimate_tls_overhead(&self, id: u32, bytes: u64) -> u32 {
        let Some(session) = self.session(id) else {
            return 0;
        };
        let records = bytes.div_ceil(u64::from(session.max_frame));
        u32::try_from(records.saturating_mul(u64::from(session.overhead))).unwrap_or(u32::MAX)
    }

    /// A session by id.
    #[must_use]
    pub fn session(&self, id: u32) -> Option<&TlsSession> {
        self.sessions.get(id as usize)?.as_ref()
    }

    fn session_mut(&mut self, id: u32) -> Option<&mut TlsSession> {
        self.sessions.get_mut(id as usize)?.as_mut()
    }
}

/// The HPTS/pacing method.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PacingMethod {
    /// No pacing.
    #[default]
    None,
    /// Regular pacing.
    Reg,
    /// Dynamic goodput pacing.
    Dgp,
}

/// Software HPTS state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hpts {
    /// A callout is armed.
    pub active: bool,
    /// Absolute deadline in microseconds.
    pub deadline_us: i64,
    /// Insert calls.
    pub inserts: u64,
    /// Remove calls.
    pub removes: u64,
}

/// Software HPTS/pacing context.
#[derive(Clone, Debug)]
pub struct Pacing {
    /// `rack_pace_every_seg`/pacing enabled.
    pub enabled: bool,
    /// `rack_default_pacing_divisor`.
    pub divisor: u32,
    /// `rack_pacing_min_seg`.
    pub min_seg: u16,
    /// `rack_pacing_delay_reduction`.
    pub delay_reduction: u32,
    /// `rack_hptsi_segments`/maximum burst segments.
    pub max_segs: u16,
    /// Current method.
    pub method: PacingMethod,
    /// CC beta-swap state (`rc_pacing_cc_set`).
    pub cc_pacing: bool,
    /// HPTS state.
    pub hpts: Hpts,
}

impl Default for Pacing {
    fn default() -> Self {
        Self {
            enabled: false,
            divisor: 250,
            min_seg: 0,
            delay_reduction: 4,
            max_segs: 40,
            method: PacingMethod::None,
            cc_pacing: false,
            hpts: Hpts::default(),
        }
    }
}

impl Pacing {
    /// FreeBSD `rack_get_pacing_len`: burst bytes for `bw` bytes/s and `mss`.
    #[must_use]
    pub fn rack_get_pacing_len(&self, bw: u64, mss: u32) -> u32 {
        let user_max = u32::from(self.max_segs).saturating_mul(mss);
        if bw == 0 || mss == 0 {
            return 0;
        }
        // Software model of tcp_get_pacing_burst_size_w_divisor: the divisor is a milli-second
        // pacing window, converted to MSS-sized segments, then clamped by the user minimum and
        // maximum.
        let window_bytes = bw.saturating_mul(u64::from(self.divisor.max(1))) / 1_000;
        let segments = window_bytes.div_ceil(u64::from(mss));
        let segments = segments
            .max(u64::from(self.min_seg))
            .min(u64::from(self.max_segs));
        u32::try_from(segments)
            .unwrap_or(u32::MAX)
            .saturating_mul(mss)
            .min(user_max)
    }

    /// FreeBSD `rack_set_cc_pacing`: mark the CC beta values swapped.
    pub fn rack_set_cc_pacing(&mut self) -> bool {
        if self.cc_pacing {
            return false;
        }
        self.cc_pacing = true;
        true
    }

    /// FreeBSD `rack_undo_cc_pacing`: restore the CC beta values.
    pub fn rack_undo_cc_pacing(&mut self) -> bool {
        if !self.cc_pacing {
            return false;
        }
        self.cc_pacing = false;
        true
    }

    /// FreeBSD `rack_remove_pacing`: undo CC pacing and clear the method.
    pub fn rack_remove_pacing(&mut self) -> bool {
        let changed = self.cc_pacing || self.method != PacingMethod::None || self.enabled;
        self.rack_undo_cc_pacing();
        self.method = PacingMethod::None;
        self.enabled = false;
        self.hpts.active = false;
        changed
    }

    /// FreeBSD `tcp_hpts_insert`: arm the callout at `now_us + usecs`.
    pub fn tcp_hpts_insert(&mut self, now_us: i64, usecs: u32) -> i64 {
        let deadline = now_us.saturating_add(i64::from(usecs));
        self.hpts.active = true;
        self.hpts.deadline_us = deadline;
        self.hpts.inserts = self.hpts.inserts.saturating_add(1);
        deadline
    }

    /// FreeBSD `tcp_hpts_remove`: disarm the callout.
    pub fn tcp_hpts_remove(&mut self) -> bool {
        let was = self.hpts.active;
        self.hpts.active = false;
        self.hpts.removes = self.hpts.removes.saturating_add(1);
        was
    }

    /// Whether the callout is due.
    #[must_use]
    pub const fn tcp_hpts_due(&self, now_us: i64) -> bool {
        self.hpts.active && now_us >= self.hpts.deadline_us
    }

    /// Set the maximum burst segments.
    pub fn rack_set_pace_segments(&mut self, segments: u16) -> bool {
        self.max_segs = segments;
        true
    }
}

/// The `tcpcb` fields used by the RACK port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RackTcpcb {
    /// `snd_una`.
    pub snd_una: u32,
    /// `snd_nxt`.
    pub snd_nxt: u32,
    /// `snd_max`.
    pub snd_max: u32,
    /// `snd_cwnd`.
    pub snd_cwnd: u32,
    /// `snd_ssthresh`.
    pub snd_ssthresh: u32,
    /// `t_flags`.
    pub t_flags: u32,
    /// `t_state`.
    pub t_state: u8,
    /// `t_rxtshift`.
    pub t_rxtshift: u8,
    /// `t_srtt` (q8).
    pub t_srtt: u32,
    /// `t_rttvar` (q8).
    pub t_rttvar: u32,
    /// `t_rttmin` in slow ticks.
    pub t_rttmin: u32,
    /// `t_rxtcur` in slow ticks.
    pub t_rxtcur: u32,
    /// `t_maxseg`.
    pub t_maxseg: u16,
    /// `t_peer_mss`.
    pub t_peer_mss: u16,
    /// `t_ourmss`.
    pub t_ourmss: u16,
    /// `t_rcvtime`.
    pub t_rcvtime: u32,
    /// `t_sndtime`.
    pub t_sndtime: u32,
    /// `t_softerror`.
    pub t_softerror: i32,
    /// `t_acktime`.
    pub t_acktime: u32,
    /// `t_ecn`.
    pub t_ecn: u8,
    /// `t_keepidle`.
    pub t_keepidle: u32,
    /// `t_keepintvl`.
    pub t_keepintvl: u32,
    /// `t_maxidle`.
    pub t_maxidle: u32,
    /// `t_rtseq`.
    pub t_rtseq: u32,
    /// `t_bw` (bytes/s).
    pub t_bw: u64,
    /// `t_pacing_rate` (bytes/s).
    pub t_pacing_rate: u64,
    /// The pacing method.
    pub pacing_method: PacingMethod,
}

impl Default for RackTcpcb {
    fn default() -> Self {
        Self {
            snd_una: 0,
            snd_nxt: 0,
            snd_max: 0,
            snd_cwnd: 10,
            snd_ssthresh: u32::MAX,
            t_flags: 0,
            t_state: 0,
            t_rxtshift: 0,
            t_srtt: 0,
            t_rttvar: 0,
            t_rttmin: 2,
            t_rxtcur: 2,
            t_maxseg: 1_460,
            t_peer_mss: 1_460,
            t_ourmss: 1_460,
            t_rcvtime: 0,
            t_sndtime: 0,
            t_softerror: 0,
            t_acktime: 0,
            t_ecn: 0,
            t_keepidle: 14_400,
            t_keepintvl: 150,
            t_maxidle: 1_200,
            t_rtseq: 0,
            t_bw: 0,
            t_pacing_rate: 0,
            pacing_method: PacingMethod::None,
        }
    }
}

impl RackTcpcb {
    /// `RACK_REXMTVAL`: `max(t_rttmin, (t_srtt >> 3) + t_rttvar)` in the donor units.
    #[must_use]
    pub const fn rack_rexmtval(&self) -> u32 {
        let value = (self.t_srtt >> 3).saturating_add(self.t_rttvar);
        if value < self.t_rttmin {
            self.t_rttmin
        } else {
            value
        }
    }

    /// True when the connection has advanced past the SYN.
    #[must_use]
    pub const fn have_established(&self) -> bool {
        self.t_state >= 4
    }
}

/// Software model of the `tcp_stacks` transport callbacks used by `rack.c`.
#[derive(Clone, Debug)]
pub struct RackStacks {
    /// Stack name.
    pub name: &'static str,
    /// `tfb_tcp_output` calls.
    pub output_calls: u64,
    /// `tfb_tcp_rexmit_tmr` calls.
    pub rexmit_timer_calls: u64,
    /// `tfb_hwtls_change` calls.
    pub hwtls_changes: u64,
}

impl Default for RackStacks {
    fn default() -> Self {
        Self::new()
    }
}

impl RackStacks {
    /// The RACK transport object.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            name: "rack",
            output_calls: 0,
            rexmit_timer_calls: 0,
            hwtls_changes: 0,
        }
    }

    /// `tfb_tcp_output`.
    pub fn tfb_tcp_output(&mut self) -> i32 {
        self.output_calls = self.output_calls.saturating_add(1);
        0
    }

    /// `tfb_tcp_rexmit_tmr`.
    pub fn tfb_tcp_rexmit_tmr(&mut self) {
        self.rexmit_timer_calls = self.rexmit_timer_calls.saturating_add(1);
    }

    /// `tfb_hwtls_change`.
    pub fn tfb_hwtls_change(&mut self, _enabled: bool) {
        self.hwtls_changes = self.hwtls_changes.saturating_add(1);
    }
}

/// The aggregate sans-IO OS context for the RACK port.
pub struct RackOs {
    /// mbuf allocator/chain.
    pub mbufs: MbufPool,
    /// send socket buffer.
    pub snd: SockBuf,
    /// UMA zones.
    pub uma: Uma,
    /// sysctl tree.
    pub sysctl: SysctlTree,
    /// BTF/blackbox log.
    pub log: TcpLog,
    /// KTLS sessions.
    pub tls: TlsContext,
    /// HPTS/pacing.
    pub pacing: Pacing,
    /// tcpcb mirror.
    pub tcpcb: RackTcpcb,
    /// `tcp_stacks` transport callbacks.
    pub stacks: RackStacks,
    /// Microsecond clock.
    pub now_us: i64,
    /// `INP_WLOCK` state.
    inp_locked: bool,
    /// `SOCK_SENDBUF_LOCK` state.
    sndbuf_locked: bool,
    /// `sched_pin` nesting.
    pins: u32,
    /// Last donor assertion message.
    pub last_fault: Option<String>,
}

impl Default for RackOs {
    fn default() -> Self {
        Self::new()
    }
}

impl RackOs {
    /// Create a context with a 16 KiB send buffer and a 4-event log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mbufs: MbufPool::new(),
            snd: SockBuf::new(16 * 1024, 32 * 1024),
            uma: Uma::new(),
            sysctl: SysctlTree::new(),
            log: TcpLog::new(false, 4),
            tls: TlsContext::new(),
            pacing: Pacing::default(),
            tcpcb: RackTcpcb::default(),
            stacks: RackStacks::new(),
            now_us: 0,
            inp_locked: false,
            sndbuf_locked: false,
            pins: 0,
            last_fault: None,
        }
    }

    /// Advance the software clock.
    pub fn advance_us(&mut self, usecs: u32) {
        self.now_us = self.now_us.saturating_add(i64::from(usecs));
    }

    /// FreeBSD `tcp_get_usecs`.
    #[must_use]
    pub fn tcp_get_usecs(&self) -> u64 {
        u64::try_from(self.now_us).unwrap_or(0)
    }

    /// FreeBSD `get_cyclecount` (the software clock in this model).
    #[must_use]
    pub fn get_cyclecount(&self) -> u64 {
        self.tcp_get_usecs()
    }

    /// FreeBSD `tcp_ts_getticks` (milliseconds from the microsecond clock).
    #[must_use]
    pub fn tcp_ts_getticks(&self) -> u32 {
        u32::try_from(self.now_us / 1_000).unwrap_or(u32::MAX)
    }

    /// FreeBSD `tcp_tv_to_usec`.
    #[must_use]
    pub const fn tcp_tv_to_usec(tv_sec: i64, tv_usec: i64) -> i64 {
        tv_sec.saturating_mul(1_000_000).saturating_add(tv_usec)
    }

    /// FreeBSD `tfb_tcp_output`.
    pub fn tfb_tcp_output(&mut self) -> i32 {
        self.stacks.tfb_tcp_output()
    }

    /// FreeBSD `tfb_tcp_rexmit_tmr`.
    pub fn tfb_tcp_rexmit_tmr(&mut self) {
        self.stacks.tfb_tcp_rexmit_tmr();
    }

    /// FreeBSD `tfb_hwtls_change`.
    pub fn tfb_hwtls_change(&mut self, enabled: bool) {
        self.stacks.tfb_hwtls_change(enabled);
    }

    /// FreeBSD `m_get`.
    pub fn m_get(&mut self) -> Option<MbufId> {
        self.mbufs.m_get(M_NOWAIT)
    }

    /// FreeBSD `m_gethdr`.
    pub fn m_gethdr(&mut self) -> Option<MbufId> {
        self.mbufs.m_gethdr(M_NOWAIT)
    }

    /// FreeBSD `m_getcl`.
    pub fn m_getcl(&mut self, flags: u32) -> Option<MbufId> {
        self.mbufs.m_getcl(M_NOWAIT, flags)
    }

    /// FreeBSD `m_free`.
    pub fn m_free(&mut self, id: MbufId) -> Option<MbufId> {
        self.mbufs.m_free(id)
    }

    /// FreeBSD `m_freem`.
    pub fn m_freem(&mut self, chain: Option<MbufId>) {
        self.mbufs.m_freem(chain);
    }

    /// FreeBSD `m_adj`; returns the new chain head.
    pub fn m_adj(&mut self, chain: Option<MbufId>, req_len: i32) -> Option<MbufId> {
        self.mbufs.m_adj(chain, req_len)
    }

    /// FreeBSD `m_copym`.
    pub fn m_copym(&mut self, chain: Option<MbufId>, off: usize, len: usize) -> Option<MbufId> {
        self.mbufs.m_copym(chain, off, len)
    }

    /// FreeBSD `m_copydata`.
    pub fn m_copydata(
        &self,
        chain: Option<MbufId>,
        off: usize,
        len: usize,
        out: &mut [u8],
    ) -> Result<usize, RackOsError> {
        self.mbufs.m_copydata(chain, off, len, out)
    }

    /// FreeBSD `m_length`.
    #[must_use]
    pub fn m_length(&self, chain: Option<MbufId>) -> usize {
        self.mbufs.m_length(chain)
    }

    /// FreeBSD `m_memcnt`.
    #[must_use]
    pub const fn m_memcnt(&self) -> u64 {
        self.mbufs.m_memcnt()
    }

    /// FreeBSD `M_TRAILINGROOM`.
    #[must_use]
    pub fn m_trailingroom(&self, id: MbufId) -> usize {
        self.mbufs.m_trailingroom(id)
    }

    /// FreeBSD `sbavail`.
    #[must_use]
    pub const fn sbavail(&self) -> u32 {
        self.snd.sbavail()
    }

    /// FreeBSD `sbused`.
    #[must_use]
    pub const fn sbused(&self) -> u32 {
        self.snd.sbused()
    }

    /// FreeBSD `sbspace`.
    #[must_use]
    pub const fn sbspace(&self) -> i64 {
        self.snd.sbspace()
    }

    /// FreeBSD `sbappendstream_locked`.
    pub fn sbappendstream_locked(&mut self, chain: Option<MbufId>) -> Result<(), RackOsError> {
        self.snd.sbappendstream_locked(&mut self.mbufs, chain)
    }

    /// FreeBSD `sbcut_locked`.
    pub fn sbcut_locked(&mut self, len: usize) -> Option<MbufId> {
        self.snd.sbcut_locked(&mut self.mbufs, len)
    }

    /// FreeBSD `sbsndmbuf`.
    #[must_use]
    pub fn sbsndmbuf(&self, off: u32) -> Option<(MbufId, u32)> {
        self.snd.sbsndmbuf(&self.mbufs, off)
    }

    /// FreeBSD `sbsndptr_noadv`.
    pub fn sbsndptr_noadv(&mut self, off: u32) -> Option<(MbufId, u32)> {
        self.snd.sbsndptr_noadv(&self.mbufs, off)
    }

    /// FreeBSD `sbsndptr_adv`.
    pub fn sbsndptr_adv(&mut self, mb: MbufId, len: u32) -> bool {
        self.snd.sbsndptr_adv(&self.mbufs, mb, len)
    }

    /// FreeBSD `sbreserve_locked`.
    pub fn sbreserve_locked(&mut self, cc: u32) -> bool {
        self.snd.sbreserve_locked(cc)
    }

    /// FreeBSD `uma_zcreate`.
    pub fn uma_zcreate(&mut self, name: &str, size: usize) -> u32 {
        self.uma.uma_zcreate(name, size)
    }

    /// FreeBSD `uma_zalloc`.
    pub fn uma_zalloc(&mut self, zone: u32) -> Option<u64> {
        self.uma.uma_zalloc(zone)
    }

    /// FreeBSD `uma_zfree`.
    pub fn uma_zfree(&mut self, zone: u32, item: u64) -> bool {
        self.uma.uma_zfree(zone, item)
    }

    /// FreeBSD `uma_zdestroy`.
    pub fn uma_zdestroy(&mut self, zone: u32) -> bool {
        self.uma.uma_zdestroy(zone)
    }

    /// FreeBSD `uma_zone_set_max`.
    pub fn uma_zone_set_max(&mut self, zone: u32, max: usize) -> bool {
        self.uma.uma_zone_set_max(zone, max)
    }

    /// FreeBSD `uma_zone_get_max`.
    #[must_use]
    pub fn uma_zone_get_max(&self, zone: u32) -> Option<usize> {
        self.uma.uma_zone_get_max(zone)
    }

    /// FreeBSD `uma_zone_get_cur`.
    #[must_use]
    pub fn uma_zone_get_cur(&self, zone: u32) -> Option<usize> {
        self.uma.uma_zone_get_cur(zone)
    }

    /// FreeBSD `sysctl_ctx_init`.
    pub fn sysctl_ctx_init(&mut self) -> u32 {
        self.sysctl.sysctl_ctx_init()
    }

    /// FreeBSD `sysctl_ctx_free`.
    pub fn sysctl_ctx_free(&mut self, ctx: u32) -> usize {
        self.sysctl.sysctl_ctx_free(ctx)
    }

    /// FreeBSD `SYSCTL_ADD_NODE`.
    pub fn sysctl_add_node(&mut self, parent: Option<u32>, name: &str, ctx: u32) -> u32 {
        self.sysctl.sysctl_add_node(parent, name, ctx)
    }

    /// FreeBSD `SYSCTL_ADD_U32`.
    pub fn sysctl_add_u32(&mut self, parent: Option<u32>, name: &str, value: u32, ctx: u32) -> u32 {
        self.sysctl.sysctl_add_u32(parent, name, value, ctx)
    }

    /// FreeBSD `SYSCTL_ADD_COUNTER_U64`.
    pub fn sysctl_add_counter(
        &mut self,
        parent: Option<u32>,
        name: &str,
        value: u64,
        ctx: u32,
    ) -> u32 {
        self.sysctl.sysctl_add_counter(parent, name, value, ctx)
    }

    /// FreeBSD `SYSCTL_ADD_PROC`.
    pub fn sysctl_add_proc(
        &mut self,
        parent: Option<u32>,
        name: &str,
        text: &str,
        ctx: u32,
    ) -> u32 {
        self.sysctl.sysctl_add_proc(parent, name, text, ctx)
    }

    /// FreeBSD `SYSCTL_CHILDREN`.
    #[must_use]
    pub fn sysctl_children(&self, parent: Option<u32>) -> Vec<u32> {
        self.sysctl.sysctl_children(parent)
    }

    /// FreeBSD `sysctl_handle_32`.
    #[must_use]
    pub fn sysctl_handle_32(&self, node: u32) -> Option<i64> {
        self.sysctl.sysctl_handle_32(node)
    }

    /// Set a sysctl scalar.
    pub fn sysctl_set_u32(&mut self, node: u32, value: u32) -> bool {
        self.sysctl.sysctl_set_u32(node, value)
    }

    /// Add to a sysctl counter.
    pub fn sysctl_counter_add(&mut self, node: u32, delta: u64) -> bool {
        self.sysctl.sysctl_counter_add(node, delta)
    }

    /// FreeBSD `tcp_bblogging_on`.
    #[must_use]
    pub const fn tcp_bblogging_on(&self) -> bool {
        self.log.tcp_bblogging_on()
    }

    /// FreeBSD `tcp_log_event`.
    pub fn tcp_log_event(&mut self, kind: u32, args: [u32; 4], status: i32) -> bool {
        self.log.tcp_log_event(kind, args, status)
    }

    /// FreeBSD `tcp_log_stackspecific`.
    pub fn tcp_log_stackspecific(&mut self, kind: u32, status: i32, bytes: u32) -> bool {
        self.log.tcp_log_stackspecific(kind, status, bytes)
    }

    /// FreeBSD `ktls_enable_tx`.
    pub fn ktls_enable(&mut self, hardware: bool, overhead: u32, max_frame: u32) -> u32 {
        self.tls.ktls_enable(hardware, overhead, max_frame)
    }

    /// FreeBSD `ktls_ready`.
    #[must_use]
    pub fn ktls_ready(&self, id: u32) -> bool {
        self.tls.ktls_ready(id)
    }

    /// FreeBSD `ktls_get_maxlen`.
    #[must_use]
    pub fn ktls_get_maxlen(&self, id: u32) -> u32 {
        self.tls.ktls_get_maxlen(id)
    }

    /// FreeBSD `ktls_output_eagain`.
    pub fn ktls_output_eagain(&mut self, id: u32) -> bool {
        self.tls.ktls_output_eagain(id)
    }

    /// FreeBSD `ktls_send`.
    pub fn ktls_send(&mut self, id: u32, len: u32) -> Result<u32, RackOsError> {
        self.tls.ktls_send(id, len)
    }

    /// FreeBSD `tcp_estimate_tls_overhead`.
    #[must_use]
    pub fn tcp_estimate_tls_overhead(&self, id: u32, bytes: u64) -> u32 {
        self.tls.tcp_estimate_tls_overhead(id, bytes)
    }

    /// FreeBSD `ktls_release_snd_tag`.
    pub fn ktls_release_snd_tag(&mut self, id: u32) -> bool {
        self.tls.ktls_release_snd_tag(id)
    }

    /// Attach a simulated hardware send tag.
    pub fn ktls_set_snd_tag(&mut self, id: u32, tag: u32) -> bool {
        self.tls.ktls_set_snd_tag(id, tag)
    }

    /// FreeBSD `tcp_hpts_insert`.
    pub fn tcp_hpts_insert(&mut self, usecs: u32) -> i64 {
        self.pacing.tcp_hpts_insert(self.now_us, usecs)
    }

    /// FreeBSD `tcp_hpts_remove`.
    pub fn tcp_hpts_remove(&mut self) -> bool {
        self.pacing.tcp_hpts_remove()
    }

    /// Whether the HPTS callout is due at the current time.
    #[must_use]
    pub const fn tcp_hpts_due(&self) -> bool {
        self.pacing.tcp_hpts_due(self.now_us)
    }

    /// FreeBSD `rack_set_cc_pacing`.
    pub fn rack_set_cc_pacing(&mut self) -> bool {
        self.pacing.rack_set_cc_pacing()
    }

    /// FreeBSD `rack_undo_cc_pacing`.
    pub fn rack_undo_cc_pacing(&mut self) -> bool {
        self.pacing.rack_undo_cc_pacing()
    }

    /// FreeBSD `rack_remove_pacing`.
    pub fn rack_remove_pacing(&mut self) -> bool {
        self.pacing.rack_remove_pacing()
    }

    /// FreeBSD `rack_get_pacing_len`.
    #[must_use]
    pub fn rack_get_pacing_len(&self, bw: u64, mss: u32) -> u32 {
        self.pacing.rack_get_pacing_len(bw, mss)
    }

    /// FreeBSD `rack_set_pace_segments`.
    pub fn rack_set_pace_segments(&mut self, segments: u16) -> bool {
        self.pacing.rack_set_pace_segments(segments)
    }

    /// FreeBSD `INP_WLOCK`.
    pub fn inp_wlock(&mut self) -> Result<(), RackOsError> {
        if self.inp_locked {
            return Err(RackOsError::Locked);
        }
        self.inp_locked = true;
        Ok(())
    }

    /// FreeBSD `INP_WUNLOCK`.
    pub fn inp_wunlock(&mut self) -> bool {
        let was = self.inp_locked;
        self.inp_locked = false;
        was
    }

    /// FreeBSD `INP_WLOCK_ASSERT`.
    pub fn inp_wlock_assert(&self) -> Result<(), RackOsError> {
        if self.inp_locked {
            Ok(())
        } else {
            Err(RackOsError::Locked)
        }
    }

    /// FreeBSD `SOCK_SENDBUF_LOCK`.
    pub fn sock_sendbuf_lock(&mut self) -> Result<(), RackOsError> {
        if self.sndbuf_locked {
            return Err(RackOsError::Locked);
        }
        self.sndbuf_locked = true;
        Ok(())
    }

    /// FreeBSD `SOCK_SENDBUF_UNLOCK`.
    pub fn sock_sendbuf_unlock(&mut self) -> bool {
        let was = self.sndbuf_locked;
        self.sndbuf_locked = false;
        was
    }

    /// FreeBSD `sched_pin`.
    pub fn sched_pin(&mut self) {
        self.pins = self.pins.saturating_add(1);
    }

    /// FreeBSD `sched_unpin`.
    pub fn sched_unpin(&mut self) {
        self.pins = self.pins.saturating_sub(1);
    }

    /// The current pin nesting.
    #[must_use]
    pub const fn pins(&self) -> u32 {
        self.pins
    }

    /// FreeBSD `KASSERT`.
    pub fn kassert(&mut self, condition: bool, message: &str) -> Result<(), RackOsError> {
        if condition {
            Ok(())
        } else {
            self.last_fault = Some(message.to_owned());
            Err(RackOsError::Assert)
        }
    }
}

#[cfg(test)]
#[path = "rack_os_ut.rs"]
mod tests;
