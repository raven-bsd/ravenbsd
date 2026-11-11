// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure read-only initfs tree model used by `initfsd`.
//!
//! The model synthesises directories from the sorted initfs paths, assigns stable inode numbers
//! (files keep the initfs entry index + 1, directories follow after the files), and packs K60
//! directory entries. It never mutates: all write operations are rejected by the server.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_proto_fs::{
    DT_DIR, DT_REG, DirEntry, DirEntryHeader, Errno, S_IFDIR, S_IFREG, Stat, StatFs,
};

/// One read-only initfs file. The data borrows the mapped archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileEntry<'a> {
    /// Absolute initfs path starting with `/`.
    pub path: &'a str,
    /// Permission bits from the initfs header.
    pub mode: u32,
    /// File contents in the archive.
    pub data: &'a [u8],
}

/// A node reference inside a [`Model`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeRef {
    /// Index into the model's file array.
    File(usize),
    /// Index into the model's directory array; index 0 is the root.
    Dir(usize),
}

struct DirNode {
    name: String,
    ino: u64,
    entries: BTreeMap<String, NodeRef>,
}

struct FileNode<'a> {
    name: String,
    ino: u64,
    mode: u32,
    data: &'a [u8],
}

/// A read-only tree built from initfs entries.
pub struct Model<'a> {
    dirs: Vec<DirNode>,
    files: Vec<FileNode<'a>>,
    file_count: usize,
}

impl<'a> Model<'a> {
    /// Build the tree from `entries` in initfs order.
    ///
    /// Every directory prefix is created; a path used both as a file and as a directory, a
    /// duplicate path or an invalid component is rejected. Files keep inode `index + 1`;
    /// directories start at `entries.len() + 1`.
    pub fn new(entries: &[FileEntry<'a>]) -> Result<Self, Errno> {
        let root_ino = u64::try_from(entries.len())
            .map_err(|_| Errno::ENOMEM)?
            .checked_add(1)
            .ok_or(Errno::ENOMEM)?;
        let mut dirs = Vec::new();
        dirs.push(DirNode {
            name: "/".to_string(),
            ino: root_ino,
            entries: BTreeMap::new(),
        });
        let mut files = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            if !entry.path.starts_with('/') {
                return Err(Errno::EINVAL);
            }
            let components = components(entry.path)?;
            if components.is_empty() {
                return Err(Errno::EINVAL);
            }
            let mut current = 0usize;
            for name in components.iter().take(components.len() - 1) {
                match dirs[current].entries.get(*name).copied() {
                    Some(NodeRef::Dir(dir)) => current = dir,
                    Some(NodeRef::File(_)) => return Err(Errno::ENOTDIR),
                    None => {
                        let ino = u64::try_from(entries.len() + dirs.len() + 1)
                            .map_err(|_| Errno::ENOMEM)?;
                        let dir = dirs.len();
                        dirs.push(DirNode {
                            name: (*name).to_string(),
                            ino,
                            entries: BTreeMap::new(),
                        });
                        dirs[current]
                            .entries
                            .insert((*name).to_string(), NodeRef::Dir(dir));
                        current = dir;
                    }
                }
            }
            let name = components.last().ok_or(Errno::EINVAL)?;
            if dirs[current].entries.contains_key(*name) {
                return Err(Errno::EEXIST);
            }
            let ino = u64::try_from(index + 1).map_err(|_| Errno::ENOMEM)?;
            files.push(FileNode {
                name: (*name).to_string(),
                ino,
                mode: entry.mode & 0o7777,
                data: entry.data,
            });
            dirs[current]
                .entries
                .insert((*name).to_string(), NodeRef::File(index));
        }
        let file_count = files.len();
        Ok(Self {
            dirs,
            files,
            file_count,
        })
    }

    /// Number of files.
    pub fn file_count(&self) -> usize {
        self.file_count
    }

    /// Number of directories, including the root.
    pub fn dir_count(&self) -> usize {
        self.dirs.len()
    }

    /// Look up an absolute or relative path.
    ///
    /// An empty path or `/` returns the root directory. A file used as an intermediate
    /// component is `ENOTDIR`; a missing component is `ENOENT`.
    pub fn lookup(&self, path: &str) -> Result<NodeRef, Errno> {
        let components = components(path)?;
        let mut current = NodeRef::Dir(0);
        for name in components {
            let NodeRef::Dir(dir) = current else {
                return Err(Errno::ENOTDIR);
            };
            current = *self.dirs[dir].entries.get(name).ok_or(Errno::ENOENT)?;
        }
        Ok(current)
    }

    /// Read a file at `offset`; returns the number of bytes copied (short at EOF).
    pub fn read_file(&self, file: usize, offset: u64, out: &mut [u8]) -> Result<usize, Errno> {
        let node = self.files.get(file).ok_or(Errno::EBADF)?;
        let start = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
        let available = node.data.get(start..).unwrap_or(&[]);
        let length = available.len().min(out.len());
        let target = out.get_mut(..length).ok_or(Errno::EINVAL)?;
        target.copy_from_slice(available.get(..length).unwrap_or(&[]));
        Ok(length)
    }

    /// Pack directory entries from byte `cookie`, as K60 `READ` expects.
    pub fn readdir(&self, dir: usize, cookie: u64, out: &mut [u8]) -> Result<usize, Errno> {
        let node = self.dirs.get(dir).ok_or(Errno::EBADF)?;
        let mut offset = 0u64;
        let mut written = 0usize;
        for (name, child) in &node.entries {
            let (ino, kind) = match *child {
                NodeRef::File(index) => (self.files.get(index).ok_or(Errno::EIO)?.ino, DT_REG),
                NodeRef::Dir(index) => (self.dirs.get(index).ok_or(Errno::EIO)?.ino, DT_DIR),
            };
            let entry = DirEntry {
                header: DirEntryHeader {
                    ino,
                    kind,
                    name_len: u8::try_from(name.len()).map_err(|_| Errno::ENAMETOOLONG)?,
                    pad: [0; 6],
                },
                name: name.as_bytes(),
            };
            let length = entry.encoded_len();
            let start = offset;
            offset = offset.checked_add(length as u64).ok_or(Errno::EIO)?;
            if offset <= cookie {
                continue;
            }
            if start < cookie {
                return Err(Errno::EINVAL);
            }
            if written + length > out.len() {
                if written == 0 {
                    return Err(Errno::EINVAL);
                }
                break;
            }
            let slot = out
                .get_mut(written..written + length)
                .ok_or(Errno::EINVAL)?;
            entry.encode(slot).map_err(|_| Errno::EIO)?;
            written += length;
        }
        Ok(written)
    }

    /// Stat one node.
    pub fn stat(&self, node: NodeRef) -> Result<Stat, Errno> {
        match node {
            NodeRef::File(index) => {
                let file = self.files.get(index).ok_or(Errno::EBADF)?;
                let size = file.data.len() as u64;
                Ok(Stat {
                    ino: file.ino,
                    size,
                    blocks: size.div_ceil(512),
                    mode: S_IFREG | (file.mode & 0o7777),
                    nlink: 1,
                    ..Stat::default()
                })
            }
            NodeRef::Dir(index) => {
                let dir = self.dirs.get(index).ok_or(Errno::EBADF)?;
                Ok(Stat {
                    ino: dir.ino,
                    mode: S_IFDIR | 0o755,
                    nlink: 2,
                    ..Stat::default()
                })
            }
        }
    }

    /// Filesystem statistics for the read-only image.
    pub fn statfs(&self) -> StatFs {
        let bytes: usize = self.files.iter().map(|file| file.data.len()).sum();
        let blocks = (bytes / 4096) as u64;
        StatFs {
            block_size: 4096,
            blocks,
            free_blocks: 0,
            files: (self.files.len() + self.dirs.len()) as u64,
            free_files: 0,
        }
    }

    /// File name for a node (used by tests).
    pub fn name(&self, node: NodeRef) -> &str {
        match node {
            NodeRef::File(index) => self.files.get(index).map_or("", |file| file.name.as_str()),
            NodeRef::Dir(index) => self.dirs.get(index).map_or("", |dir| dir.name.as_str()),
        }
    }
}

fn components(path: &str) -> Result<Vec<&str>, Errno> {
    if path.contains('\0') {
        return Err(Errno::EINVAL);
    }
    let mut result = Vec::new();
    for component in path.split('/').filter(|component| !component.is_empty()) {
        if component == "." || component == ".." {
            return Err(Errno::EINVAL);
        }
        if component.len() > 255 {
            return Err(Errno::ENAMETOOLONG);
        }
        result.push(component);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::String;
    use raven_proto_fs::DirEntryIter;

    fn entry(path: &'static str, data: &'static [u8]) -> FileEntry<'static> {
        FileEntry {
            path,
            mode: 0o644,
            data,
        }
    }

    fn model() -> Model<'static> {
        Model::new(&[
            entry("/a", b"alpha"),
            entry("/dir/b", b"beta"),
            entry("/dir/sub/c", b"gamma"),
        ])
        .unwrap()
    }

    fn names(model: &Model<'_>, dir: usize) -> Vec<String> {
        let mut output = [0u8; 1024];
        let length = model.readdir(dir, 0, &mut output).unwrap();
        DirEntryIter::new(&output[..length])
            .map(|entry| entry.unwrap().name.to_vec())
            .map(|name| String::from_utf8(name).unwrap())
            .collect()
    }

    #[test]
    fn empty_model_has_a_root() {
        let model = Model::new(&[]).unwrap();
        assert_eq!(model.dir_count(), 1);
        assert_eq!(model.file_count(), 0);
        assert_eq!(model.lookup("/").unwrap(), NodeRef::Dir(0));
    }

    #[test]
    fn lookup_empty_path_is_root() {
        let model = model();
        assert_eq!(model.lookup("").unwrap(), NodeRef::Dir(0));
        assert_eq!(model.lookup("/").unwrap(), NodeRef::Dir(0));
    }

    #[test]
    fn lookup_file() {
        let model = model();
        assert_eq!(model.lookup("/a").unwrap(), NodeRef::File(0));
    }

    #[test]
    fn lookup_nested_file() {
        let model = model();
        assert_eq!(model.lookup("/dir/sub/c").unwrap(), NodeRef::File(2));
    }

    #[test]
    fn lookup_missing_is_enoent() {
        let model = model();
        assert_eq!(model.lookup("/missing"), Err(Errno::ENOENT));
    }

    #[test]
    fn file_as_intermediate_is_enotdir() {
        let model = model();
        assert_eq!(model.lookup("/a/b"), Err(Errno::ENOTDIR));
    }

    #[test]
    fn file_as_final_is_a_file() {
        let model = model();
        let node = model.lookup("/a").unwrap();
        assert!(matches!(node, NodeRef::File(_)));
    }

    #[test]
    fn duplicate_path_is_eexist() {
        assert_eq!(
            Model::new(&[entry("/a", b"1"), entry("/a", b"2")]).err(),
            Some(Errno::EEXIST)
        );
    }

    #[test]
    fn file_directory_conflict_is_eexist() {
        assert_eq!(
            Model::new(&[entry("/a", b"1"), entry("/a/b", b"2")]).err(),
            Some(Errno::ENOTDIR)
        );
    }

    #[test]
    fn directory_file_conflict_is_eexist() {
        assert_eq!(
            Model::new(&[entry("/a/b", b"1"), entry("/a", b"2")]).err(),
            Some(Errno::EEXIST)
        );
    }

    #[test]
    fn relative_entry_path_is_einval() {
        assert_eq!(Model::new(&[entry("a", b"1")]).err(), Some(Errno::EINVAL));
    }

    #[test]
    fn dot_component_is_einval() {
        assert_eq!(
            Model::new(&[entry("/a/../b", b"1")]).err(),
            Some(Errno::EINVAL)
        );
        assert_eq!(
            Model::new(&[entry("/a/./b", b"1")]).err(),
            Some(Errno::EINVAL)
        );
    }

    #[test]
    fn nul_path_is_einval() {
        assert_eq!(
            Model::new(&[entry("/a\0b", b"1")]).err(),
            Some(Errno::EINVAL)
        );
    }

    #[test]
    fn long_name_is_enametoolong() {
        let name = "x".repeat(256);
        let path = alloc::format!("/{name}");
        let entry = FileEntry {
            path: &path,
            mode: 0o644,
            data: b"1",
        };
        assert_eq!(Model::new(&[entry]).err(), Some(Errno::ENAMETOOLONG));
    }

    #[test]
    fn empty_path_is_einval() {
        assert_eq!(Model::new(&[entry("/", b"1")]).err(), Some(Errno::EINVAL));
    }

    #[test]
    fn deep_path_is_synthesized() {
        let entry = entry("/a/b/c/d/e", b"x");
        let model = Model::new(&[entry]).unwrap();
        assert_eq!(model.lookup("/a/b/c/d/e").unwrap(), NodeRef::File(0));
        assert_eq!(model.dir_count(), 5);
    }

    #[test]
    fn read_file_from_start() {
        let model = model();
        let mut buffer = [0u8; 8];
        assert_eq!(model.read_file(0, 0, &mut buffer), Ok(5));
        assert_eq!(&buffer[..5], b"alpha");
    }

    #[test]
    fn read_file_offset_and_eof() {
        let model = model();
        let mut buffer = [0u8; 8];
        assert_eq!(model.read_file(0, 2, &mut buffer), Ok(3));
        assert_eq!(&buffer[..3], b"pha");
        assert_eq!(model.read_file(0, 5, &mut buffer), Ok(0));
        assert_eq!(model.read_file(0, 100, &mut buffer), Ok(0));
    }

    #[test]
    fn read_file_short_buffer() {
        let model = model();
        let mut buffer = [0u8; 2];
        assert_eq!(model.read_file(0, 0, &mut buffer), Ok(2));
        assert_eq!(&buffer, b"al");
    }

    #[test]
    fn read_file_invalid_index_is_ebadf() {
        let model = model();
        let mut buffer = [0u8; 4];
        assert_eq!(model.read_file(99, 0, &mut buffer), Err(Errno::EBADF));
    }

    #[test]
    fn stat_file() {
        let model = model();
        let stat = model.stat(NodeRef::File(0)).unwrap();
        assert_eq!(stat.ino, 1);
        assert_eq!(stat.size, 5);
        assert_eq!(stat.blocks, 1);
        assert_eq!(stat.mode, S_IFREG | 0o644);
        assert_eq!(stat.nlink, 1);
    }

    #[test]
    fn stat_directory() {
        let model = model();
        let stat = model.stat(NodeRef::Dir(0)).unwrap();
        assert_eq!(stat.mode, S_IFDIR | 0o755);
        assert_eq!(stat.nlink, 2);
        assert_eq!(stat.size, 0);
    }

    #[test]
    fn stat_invalid_node_is_ebadf() {
        let model = model();
        assert_eq!(model.stat(NodeRef::File(99)), Err(Errno::EBADF));
        assert_eq!(model.stat(NodeRef::Dir(99)), Err(Errno::EBADF));
    }

    #[test]
    fn statfs_is_read_only() {
        let model = model();
        let statfs = model.statfs();
        assert_eq!(statfs.block_size, 4096);
        assert_eq!(statfs.free_blocks, 0);
        assert_eq!(statfs.free_files, 0);
        assert_eq!(statfs.files, 6);
    }

    #[test]
    fn readdir_root_is_sorted() {
        let model = model();
        assert_eq!(names(&model, 0), ["a", "dir"]);
    }

    #[test]
    fn readdir_subdirectory() {
        let model = model();
        let dir = match model.lookup("/dir").unwrap() {
            NodeRef::Dir(index) => index,
            _ => panic!("dir is a directory"),
        };
        assert_eq!(names(&model, dir), ["b", "sub"]);
    }

    #[test]
    fn readdir_cookie_continues() {
        let model = model();
        let mut first = [0u8; 24];
        let mut second = [0u8; 24];
        assert_eq!(model.readdir(0, 0, &mut first), Ok(24));
        assert_eq!(model.readdir(0, 24, &mut second), Ok(24));
        let first_name = DirEntryIter::new(&first).next().unwrap().unwrap();
        let second_name = DirEntryIter::new(&second).next().unwrap().unwrap();
        assert_eq!(first_name.name, b"a");
        assert_eq!(second_name.name, b"dir");
    }

    #[test]
    fn readdir_end_cookie_returns_zero() {
        let model = model();
        let mut buffer = [0u8; 64];
        assert_eq!(model.readdir(0, 48, &mut buffer), Ok(0));
    }

    #[test]
    fn readdir_mid_cookie_is_einval() {
        let model = model();
        let mut buffer = [0u8; 64];
        assert_eq!(model.readdir(0, 5, &mut buffer), Err(Errno::EINVAL));
    }

    #[test]
    fn readdir_small_buffer_is_einval() {
        let model = model();
        let mut buffer = [0u8; 8];
        assert_eq!(model.readdir(0, 0, &mut buffer), Err(Errno::EINVAL));
    }

    #[test]
    fn readdir_invalid_directory_is_ebadf() {
        let model = model();
        let mut buffer = [0u8; 64];
        assert_eq!(model.readdir(99, 0, &mut buffer), Err(Errno::EBADF));
    }

    #[test]
    fn readdir_300_entries_across_reads() {
        let mut entries = Vec::new();
        let names: Vec<String> = (0..300).map(|index| format!("/f{index:03}")).collect();
        for name in &names {
            entries.push(FileEntry {
                path: name,
                mode: 0o644,
                data: b"x",
            });
        }
        let model = Model::new(&entries).unwrap();
        let mut buffer = [0u8; 512];
        let mut cookie = 0u64;
        let mut seen = Vec::new();
        loop {
            let length = model.readdir(0, cookie, &mut buffer).unwrap();
            if length == 0 {
                break;
            }
            for item in DirEntryIter::new(&buffer[..length]) {
                let item = item.unwrap();
                seen.push(String::from_utf8(item.name.to_vec()).unwrap());
            }
            cookie += length as u64;
            assert!(seen.len() <= 300);
        }
        assert_eq!(seen.len(), 300);
        assert_eq!(seen[0], "f000");
        assert_eq!(seen[299], "f299");
    }

    #[test]
    fn readdir_name_at_255_bytes() {
        let name = "z".repeat(255);
        let path = format!("/{name}");
        let entry = FileEntry {
            path: &path,
            mode: 0o644,
            data: b"x",
        };
        let model = Model::new(&[entry]).unwrap();
        let mut buffer = [0u8; 512];
        let length = model.readdir(0, 0, &mut buffer).unwrap();
        let item = DirEntryIter::new(&buffer[..length])
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(item.name.len(), 255);
    }

    #[test]
    fn file_inode_is_entry_index_plus_one() {
        let model = model();
        assert_eq!(model.stat(NodeRef::File(2)).unwrap().ino, 3);
    }

    #[test]
    fn directory_inodes_follow_file_inodes() {
        let model = model();
        let root = model.stat(NodeRef::Dir(0)).unwrap().ino;
        let dir = match model.lookup("/dir").unwrap() {
            NodeRef::Dir(index) => index,
            _ => panic!("dir is a directory"),
        };
        assert!(model.stat(NodeRef::Dir(dir)).unwrap().ino > root);
    }

    #[test]
    fn trailing_slash_lookup_works() {
        let model = model();
        assert_eq!(
            model.lookup("/dir/").unwrap(),
            model.lookup("/dir").unwrap()
        );
    }

    #[test]
    fn file_and_directory_counts() {
        let model = model();
        assert_eq!(model.file_count(), 3);
        assert_eq!(model.dir_count(), 3);
    }

    #[test]
    fn name_reports_the_final_component() {
        let model = model();
        assert_eq!(model.name(NodeRef::File(0)), "a");
        assert_eq!(model.name(NodeRef::Dir(1)), "dir");
    }

    #[test]
    fn modes_are_preserved_on_files() {
        let entry = FileEntry {
            path: "/x",
            mode: 0o600,
            data: b"x",
        };
        let model = Model::new(&[entry]).unwrap();
        assert_eq!(model.stat(NodeRef::File(0)).unwrap().mode, S_IFREG | 0o600);
    }

    #[test]
    fn empty_directory_reads_zero() {
        let model = Model::new(&[]).unwrap();
        let mut buffer = [0u8; 32];
        assert_eq!(model.readdir(0, 0, &mut buffer), Ok(0));
    }
}
