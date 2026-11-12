// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded IPv4 reassembly.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::{Instant, Ipv4Addr};

pub(crate) const MAX_FRAGMENTS_PER_DATAGRAM: usize = 64;

/// Reassembly key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub(crate) src: Ipv4Addr,
    pub(crate) dst: Ipv4Addr,
    pub(crate) id: u16,
    pub(crate) protocol: u8,
}

#[derive(Debug)]
struct Fragment {
    offset: usize,
    data: Vec<u8>,
}

#[derive(Debug)]
struct Entry {
    fragments: Vec<Fragment>,
    bytes: usize,
    deadline: Instant,
    saw_first: bool,
    saw_last: bool,
    last_end: usize,
}

/// Why a fragment was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReassemblyError {
    Overlap,
    Inconsistent,
    TooManyFragments,
    Exhausted,
}

/// A bounded, timed reassembly set.
#[derive(Debug)]
pub(crate) struct Reassembly {
    entries: BTreeMap<Key, Entry>,
    used_bytes: usize,
    cap_bytes: usize,
    timeout_ms: u64,
    dropped: usize,
    completed: usize,
}

impl Reassembly {
    pub(crate) fn new(cap_bytes: usize, timeout_ms: u64) -> Self {
        Self {
            entries: BTreeMap::new(),
            used_bytes: 0,
            cap_bytes,
            timeout_ms,
            dropped: 0,
            completed: 0,
        }
    }

    pub(crate) fn insert(
        &mut self,
        key: Key,
        offset: usize,
        more: bool,
        data: &[u8],
        now: Instant,
    ) -> Result<Option<Vec<u8>>, ReassemblyError> {
        if !offset.is_multiple_of(8) || data.is_empty() {
            return Err(ReassemblyError::Inconsistent);
        }
        if !self.entries.contains_key(&key)
            && self.used_bytes.saturating_add(data.len()) > self.cap_bytes
        {
            self.dropped = self.dropped.saturating_add(1);
            return Err(ReassemblyError::Exhausted);
        }
        let deadline = Instant::from_millis(
            now.as_millis()
                .saturating_add(i64::try_from(self.timeout_ms).unwrap_or(i64::MAX)),
        );
        let end = offset.saturating_add(data.len());
        let mut conflict = None;
        {
            let entry = self.entries.entry(key).or_insert_with(|| Entry {
                fragments: Vec::new(),
                bytes: 0,
                deadline,
                saw_first: false,
                saw_last: false,
                last_end: 0,
            });
            entry.deadline = deadline;
            for fragment in &entry.fragments {
                let fragment_end = fragment.offset.saturating_add(fragment.data.len());
                if offset < fragment_end && fragment.offset < end {
                    conflict = Some(ReassemblyError::Overlap);
                    break;
                }
            }
            if conflict.is_none() && entry.fragments.len() >= MAX_FRAGMENTS_PER_DATAGRAM {
                conflict = Some(ReassemblyError::TooManyFragments);
            }
            if conflict.is_none() && !more {
                if entry.saw_last && entry.last_end != end {
                    conflict = Some(ReassemblyError::Inconsistent);
                } else {
                    entry.saw_last = true;
                    entry.last_end = end;
                }
            }
            if conflict.is_none() {
                if offset == 0 {
                    entry.saw_first = true;
                }
                entry.bytes = entry.bytes.saturating_add(data.len());
                entry.fragments.push(Fragment {
                    offset,
                    data: data.to_vec(),
                });
            }
        }
        if let Some(error) = conflict {
            self.remove(&key);
            self.dropped = self.dropped.saturating_add(1);
            return Err(error);
        }
        self.used_bytes = self.used_bytes.saturating_add(data.len());

        let complete = match self.entries.get(&key) {
            Some(entry) if entry.saw_first && entry.saw_last => {
                let mut sorted: Vec<&Fragment> = entry.fragments.iter().collect();
                sorted.sort_by_key(|fragment| fragment.offset);
                let mut cursor = 0usize;
                let mut contiguous = true;
                for fragment in &sorted {
                    if fragment.offset != cursor {
                        contiguous = false;
                        break;
                    }
                    cursor = cursor.saturating_add(fragment.data.len());
                }
                contiguous && cursor == entry.last_end
            }
            _ => false,
        };
        if complete {
            let Some(entry) = self.remove(&key) else {
                return Ok(None);
            };
            self.completed = self.completed.saturating_add(1);
            let mut datagram = vec![0u8; entry.last_end];
            for fragment in entry.fragments {
                datagram[fragment.offset..fragment.offset + fragment.data.len()]
                    .copy_from_slice(&fragment.data);
            }
            return Ok(Some(datagram));
        }
        Ok(None)
    }

    pub(crate) fn expire(&mut self, now: Instant) -> usize {
        let before = self.entries.len();
        let expired: Vec<Key> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.deadline <= now)
            .map(|(key, _)| *key)
            .collect();
        for key in &expired {
            self.remove(key);
        }
        let removed = before - self.entries.len();
        self.dropped = self.dropped.saturating_add(removed);
        removed
    }

    #[allow(dead_code)] // Read by the testutil statistics hooks.
    pub(crate) fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    #[allow(dead_code)] // Read by the testutil statistics hooks.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)] // Read by the testutil statistics hooks.
    pub(crate) fn dropped(&self) -> usize {
        self.dropped
    }

    #[allow(dead_code)] // Read by the testutil statistics hooks.
    pub(crate) fn completed(&self) -> usize {
        self.completed
    }

    fn remove(&mut self, key: &Key) -> Option<Entry> {
        let entry = self.entries.remove(key)?;
        self.used_bytes = self.used_bytes.saturating_sub(entry.bytes);
        Some(entry)
    }
}
