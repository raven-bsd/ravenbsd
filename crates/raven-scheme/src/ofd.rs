// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Server-side open-file descriptions.
//!
//! One [`OpenFile`] is one open-file description (OFD): flags and the sequential
//! cursor.  The kernel may own several handles for the same OFD after `DUP`; the
//! server sees one description with a reference count and releases it only when
//! the last handle closes.  A transfer or an attached reference follows the same
//! rule.  [`OpenTable`] is the small map a filesystem server uses for that state.

use alloc::collections::BTreeMap;

use raven_abi::errno::Errno;
use raven_proto_fs::OpenFlags;

/// One open-file description and its reference count.
#[derive(Clone, Debug)]
pub struct OpenFile<F> {
    /// Server-specific file state (an inode number, a node, a handle, ...).
    pub file: F,
    /// Validated open flags.
    pub flags: OpenFlags,
    cursor: u64,
    references: u32,
}

impl<F> OpenFile<F> {
    /// The current sequential cursor.
    pub const fn cursor(&self) -> u64 {
        self.cursor
    }

    /// Set the cursor, for example after a truncate or an append write.
    pub fn set_cursor(&mut self, cursor: u64) {
        self.cursor = cursor;
    }

    /// Advance the cursor by `bytes`, returning the new position.
    pub fn advance(&mut self, bytes: usize) -> Result<u64, Errno> {
        let bytes = u64::try_from(bytes).map_err(|_| Errno::EOVERFLOW)?;
        let next = self.cursor.checked_add(bytes).ok_or(Errno::EOVERFLOW)?;
        self.cursor = next;
        Ok(next)
    }

    /// Number of handles referring to this description.
    pub const fn references(&self) -> u32 {
        self.references
    }
}

/// The server's open-file-description table.
#[derive(Clone, Debug)]
pub struct OpenTable<F> {
    entries: BTreeMap<u64, OpenFile<F>>,
    next_id: u64,
}

impl<F> Default for OpenTable<F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F> OpenTable<F> {
    /// Create an empty table.
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Insert a description under a server-chosen id and return the id.
    ///
    /// The first id is 1; zero is never allocated.  `ENFILE` is returned when
    /// the id space is exhausted.
    pub fn insert(&mut self, file: F, flags: OpenFlags) -> Result<u64, Errno> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(Errno::ENFILE)?;
        self.insert_with_id(id, file, flags)?;
        Ok(id)
    }

    /// Insert a description under an id chosen by the caller (the kernel's
    /// `file_id` in the S11 scheme protocol).
    pub fn insert_with_id(&mut self, id: u64, file: F, flags: OpenFlags) -> Result<(), Errno> {
        if id == 0 || self.entries.contains_key(&id) {
            return Err(Errno::EINVAL);
        }
        self.entries.insert(
            id,
            OpenFile {
                file,
                flags,
                cursor: 0,
                references: 1,
            },
        );
        Ok(())
    }

    /// Look up a description.
    pub fn get(&self, id: u64) -> Option<&OpenFile<F>> {
        self.entries.get(&id)
    }

    /// Look up a description mutably.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut OpenFile<F>> {
        self.entries.get_mut(&id)
    }

    /// Account for `DUP` on the same open-file description.
    ///
    /// The caller returns `id` as the server file id; the kernel gives the
    /// duplicate its own client handle, while the server keeps one description.
    pub fn dup(&mut self, id: u64) -> Result<u64, Errno> {
        let entry = self.entries.get_mut(&id).ok_or(Errno::EBADF)?;
        entry.references = entry.references.checked_add(1).ok_or(Errno::EMFILE)?;
        Ok(id)
    }

    /// Account for `CLOSE`.
    ///
    /// Returns `Some(file)` only when this was the last reference, so the caller
    /// can release unlinked data at the right time.  The id is no longer valid
    /// afterwards.
    pub fn close(&mut self, id: u64) -> Result<Option<F>, Errno> {
        let entry = self.entries.get_mut(&id).ok_or(Errno::EBADF)?;
        if entry.references > 1 {
            entry.references -= 1;
            return Ok(None);
        }
        let entry = self.entries.remove(&id).ok_or(Errno::EBADF)?;
        Ok(Some(entry.file))
    }

    /// Reference count for `id`, or `None` when it is not open.
    pub fn references(&self, id: u64) -> Option<u32> {
        self.entries.get(&id).map(OpenFile::references)
    }

    /// Number of open descriptions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags() -> OpenFlags {
        OpenFlags::from_linux(raven_proto_fs::O_RDWR).expect("flags")
    }

    #[test]
    fn insert_and_get_start_at_one() {
        let mut table = OpenTable::new();
        assert_eq!(table.insert(7u32, flags()).unwrap(), 1);
        assert_eq!(table.insert(8u32, flags()).unwrap(), 2);
        assert_eq!(table.get(1).unwrap().file, 7);
        assert_eq!(table.get(2).unwrap().file, 8);
        assert!(table.get(3).is_none());
        assert_eq!(table.references(1), Some(1));
        assert_eq!(table.len(), 2);
        assert!(!table.is_empty());
    }

    #[test]
    fn insert_with_id_is_unique_and_nonzero() {
        let mut table = OpenTable::new();
        table.insert_with_id(42, 7u32, flags()).unwrap();
        assert_eq!(table.get(42).unwrap().file, 7);
        assert_eq!(table.insert_with_id(42, 8, flags()), Err(Errno::EINVAL));
        assert_eq!(table.insert_with_id(0, 9, flags()), Err(Errno::EINVAL));
    }

    #[test]
    fn dup_shares_the_description_and_cursor() {
        let mut table = OpenTable::new();
        table.insert_with_id(5, 7u32, flags()).unwrap();
        let id = 5;
        assert_eq!(table.dup(id).unwrap(), id);
        assert_eq!(table.references(id), Some(2));
        table.get_mut(id).unwrap().advance(9).unwrap();
        assert_eq!(table.get(id).unwrap().cursor(), 9);
    }

    #[test]
    fn last_close_returns_the_file_and_removes_the_entry() {
        let mut table = OpenTable::new();
        table.insert_with_id(5, 7u32, flags()).unwrap();
        table.dup(5).unwrap();
        assert_eq!(table.close(5).unwrap(), None);
        assert_eq!(table.references(5), Some(1));
        assert_eq!(table.close(5).unwrap(), Some(7));
        assert_eq!(table.references(5), None);
        assert!(table.is_empty());
        assert_eq!(table.close(5), Err(Errno::EBADF));
    }

    #[test]
    fn missing_ids_are_ebadf() {
        let mut table = OpenTable::<u32>::new();
        assert_eq!(table.dup(9), Err(Errno::EBADF));
        assert_eq!(table.close(9), Err(Errno::EBADF));
        assert!(table.get(9).is_none());
    }

    #[test]
    fn cursor_overflow_is_reported() {
        let mut table = OpenTable::new();
        table.insert_with_id(1, 7u32, flags()).unwrap();
        let entry = table.get_mut(1).unwrap();
        entry.set_cursor(u64::MAX);
        assert_eq!(entry.advance(1), Err(Errno::EOVERFLOW));
        assert_eq!(entry.cursor(), u64::MAX);
    }
}
