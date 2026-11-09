// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host mock shared by the linux-shim tests.

#![allow(dead_code)]

use std::collections::BTreeMap;

use linux_shim::Host;
use raven_abi::errno::Errno;

#[derive(Default)]
pub struct MockHost {
    /// Native mappings, keyed by start: (length, protection).
    pub maps: BTreeMap<usize, (usize, u32)>,
    pub words: BTreeMap<usize, u32>,
    pub waiters: BTreeMap<usize, usize>,
    pub last_timeout: Option<Option<u64>>,
    pub last_clock: Option<u32>,
    pub clock: (i64, i64),
    pub now: u64,
    pub fail_map: bool,
}

impl MockHost {
    pub fn new() -> Self {
        Self {
            maps: BTreeMap::new(),
            words: BTreeMap::new(),
            waiters: BTreeMap::new(),
            last_timeout: None,
            last_clock: None,
            clock: (100, 0),
            now: 100_000_000_000,
            fail_map: false,
        }
    }

    pub fn word(&mut self, addr: usize, value: u32) {
        self.words.insert(addr, value);
    }

    pub fn mapped(&self, addr: usize, len: usize) -> bool {
        let end = addr + len;
        self.maps
            .iter()
            .any(|(&start, &(length, _))| start < end && addr < start + length)
    }
}

impl Host for MockHost {
    fn map(&mut self, addr: usize, len: usize, prot: u32, fixed: bool) -> Result<usize, Errno> {
        if self.fail_map {
            return Err(Errno::ENOMEM);
        }
        if fixed && self.mapped(addr, len) {
            return Err(Errno::EEXIST);
        }
        self.maps.insert(addr, (len, prot));
        Ok(addr)
    }

    fn unmap(&mut self, addr: usize, len: usize) -> Result<(), Errno> {
        let end = addr + len;
        let mut rebuilt = BTreeMap::new();
        for (&start, &(length, prot)) in &self.maps {
            let vma_end = start + length;
            if vma_end <= addr || end <= start {
                rebuilt.insert(start, (length, prot));
                continue;
            }
            if start < addr {
                rebuilt.insert(start, (addr - start, prot));
            }
            if end < vma_end {
                rebuilt.insert(end, (vma_end - end, prot));
            }
        }
        self.maps = rebuilt;
        Ok(())
    }

    fn protect(&mut self, addr: usize, len: usize, prot: u32) -> Result<(), Errno> {
        let end = addr + len;
        let mut rebuilt = BTreeMap::new();
        for (&start, &(length, old_prot)) in &self.maps {
            let vma_end = start + length;
            if vma_end <= addr || end <= start {
                rebuilt.insert(start, (length, old_prot));
                continue;
            }
            if start < addr {
                rebuilt.insert(start, (addr - start, old_prot));
            }
            let overlap_end = vma_end.min(end);
            rebuilt.insert(addr.max(start), (overlap_end - addr.max(start), prot));
            if end < vma_end {
                rebuilt.insert(end, (vma_end - end, old_prot));
            }
        }
        self.maps = rebuilt;
        Ok(())
    }

    fn futex_wait(
        &mut self,
        addr: usize,
        expected: u32,
        timeout_ns: Option<u64>,
    ) -> Result<(), Errno> {
        self.last_timeout = Some(timeout_ns);
        if self.words.get(&addr).copied().unwrap_or(0) != expected {
            return Err(Errno::EAGAIN);
        }
        *self.waiters.entry(addr).or_insert(0) += 1;
        Ok(())
    }

    fn futex_wake(&mut self, addr: usize, n: u32) -> usize {
        let waiters = self.waiters.entry(addr).or_insert(0);
        let woken = (*waiters).min(n as usize);
        *waiters -= woken;
        woken
    }

    fn clock_get(&mut self, native_clock: u32) -> Result<(i64, i64), Errno> {
        self.last_clock = Some(native_clock);
        if native_clock > 1 {
            return Err(Errno::EINVAL);
        }
        Ok(self.clock)
    }

    fn now_ns(&mut self) -> u64 {
        self.now
    }

    fn read_u32(&mut self, addr: usize) -> Result<u32, Errno> {
        Ok(self.words.get(&addr).copied().unwrap_or(0))
    }
}
