// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<fts.h>`, `<ftw.h>` and `<dirent.h>` exports over C46's `raven-libc-fts` (C60c).
//!
//! The shell owns the C `FTS`, `FTSENT`, `DIR` and `struct dirent` layouts, the descriptor seam
//! [`crate::platform::ShellDirSys`] and the C callbacks. C46 owns the walk, the dirent parser,
//! the stream state and the sort. Every materialised `FTSENT` is kept alive until `fts_close`,
//! so the pointers a caller holds stay valid across `fts_read` calls.
//!
//! Deviation (reported): the engine has no setter for `fts_number`/`fts_pointer`, so writes a
//! caller makes to those two fields are not persisted inside the walk. The fields are copied out
//! on every `fts_read`; `fts_set` applies only the documented instruction.

pub(crate) mod alphasort;
pub(crate) mod closedir;
pub(crate) mod dirfd;
pub(crate) mod fdopendir;
pub(crate) mod fts_children;
pub(crate) mod fts_close;
pub(crate) mod fts_open;
pub(crate) mod fts_openat;
pub(crate) mod fts_read;
pub(crate) mod fts_set;
pub(crate) mod ftw;
pub(crate) mod nftw;
pub(crate) mod opendir;
pub(crate) mod posix_getdents;
pub(crate) mod readdir;
pub(crate) mod readdir_r;
pub(crate) mod rewinddir;
pub(crate) mod scandir;
pub(crate) mod seekdir;
pub(crate) mod telldir;
pub(crate) mod versionsort;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::{Dirent, Stat};
use raven_libc_fts::{AT_FDCWD, Compar, DirEntry, DirStream, Fts, FtsEnt};

/// The C `FTS` object.
#[repr(C)]
pub(crate) struct Cfts {
    /// `fts_cur`.
    pub fts_cur: *mut Cftsent,
    /// `fts_child`.
    pub fts_child: *mut Cftsent,
    /// `fts_array`.
    pub fts_array: *mut *mut Cftsent,
    /// `fts_dev`.
    pub fts_dev: u64,
    /// `fts_path`.
    pub fts_path: *mut c_char,
    /// `fts_rfd`.
    pub fts_rfd: c_int,
    /// `fts_pathlen`.
    pub fts_pathlen: usize,
    /// `fts_nitems`.
    pub fts_nitems: usize,
    /// The `fts_compar` union.
    pub fts_compar: FtsCompar,
    /// `fts_options`.
    pub fts_options: c_int,
    /// `fts_clientptr`.
    pub fts_clientptr: *mut c_void,
}

impl Cfts {
    /// A fresh `FTS` object with no current node and the caller's comparator.
    pub(crate) fn new(
        options: c_int,
        compar: Option<extern "C" fn(*const *const Cftsent, *const *const Cftsent) -> c_int>,
    ) -> Self {
        Self {
            fts_cur: core::ptr::null_mut(),
            fts_child: core::ptr::null_mut(),
            fts_array: core::ptr::null_mut(),
            fts_dev: 0,
            fts_path: core::ptr::null_mut(),
            fts_rfd: -1,
            fts_pathlen: 0,
            fts_nitems: 0,
            fts_compar: FtsCompar { compar },
            fts_options: options,
            fts_clientptr: core::ptr::null_mut(),
        }
    }
}

/// The `fts_compar` union of `FTS`.
#[repr(C)]
pub(crate) union FtsCompar {
    /// The caller's comparator.
    pub compar: Option<extern "C" fn(*const *const Cftsent, *const *const Cftsent) -> c_int>,
    /// The same word as an opaque pointer.
    pub client: *mut c_void,
}

/// The C `FTSENT` object.
#[repr(C)]
pub(crate) struct Cftsent {
    /// `fts_cycle`.
    pub fts_cycle: *mut Cftsent,
    /// `fts_parent`.
    pub fts_parent: *mut Cftsent,
    /// `fts_link`.
    pub fts_link: *mut Cftsent,
    /// `fts_number`.
    pub fts_number: i64,
    /// `fts_pointer`.
    pub fts_pointer: *mut c_void,
    /// `fts_accpath`.
    pub fts_accpath: *mut c_char,
    /// `fts_path`.
    pub fts_path: *mut c_char,
    /// `fts_errno`.
    pub fts_errno: c_int,
    /// `fts_symfd`.
    pub fts_symfd: c_int,
    /// `fts_dirfd`.
    pub fts_dirfd: c_int,
    /// `__fts_reserved[0]` holds the engine node id.
    pub reserved: [c_int; 3],
    /// `fts_pathlen`.
    pub fts_pathlen: usize,
    /// `fts_namelen`.
    pub fts_namelen: usize,
    /// `fts_ino`.
    pub fts_ino: u64,
    /// `fts_dev`.
    pub fts_dev: u64,
    /// `fts_nlink`.
    pub fts_nlink: u64,
    /// `fts_level`.
    pub fts_level: i64,
    /// `fts_info`.
    pub fts_info: c_int,
    /// `fts_flags`.
    pub fts_flags: u32,
    /// `fts_instr`.
    pub fts_instr: c_int,
    /// `fts_statp`.
    pub fts_statp: *mut Stat,
    /// `fts_name`.
    pub fts_name: *mut c_char,
    /// `fts_fts`.
    pub fts_fts: *mut Cfts,
}

impl Cftsent {
    /// The all-zero value with every pointer NULL.
    pub(crate) fn zero() -> Self {
        Self {
            fts_cycle: core::ptr::null_mut(),
            fts_parent: core::ptr::null_mut(),
            fts_link: core::ptr::null_mut(),
            fts_number: 0,
            fts_pointer: core::ptr::null_mut(),
            fts_accpath: core::ptr::null_mut(),
            fts_path: core::ptr::null_mut(),
            fts_errno: 0,
            fts_symfd: -1,
            fts_dirfd: -1,
            reserved: [0; 3],
            fts_pathlen: 0,
            fts_namelen: 0,
            fts_ino: 0,
            fts_dev: 0,
            fts_nlink: 0,
            fts_level: 0,
            fts_info: 0,
            fts_flags: 0,
            fts_instr: 0,
            fts_statp: core::ptr::null_mut(),
            fts_name: core::ptr::null_mut(),
            fts_fts: core::ptr::null_mut(),
        }
    }
}

/// One materialised C node: the struct plus its string and stat storage.
pub(crate) struct FtsNode {
    /// The C struct.
    pub ent: Cftsent,
    /// The NUL-terminated path.
    pub path: Vec<u8>,
    /// The NUL-terminated name.
    pub name: Vec<u8>,
    /// The stat copy.
    pub stat: Stat,
}

/// The shell state behind one `FTS` object.
pub(crate) struct ShellFts {
    /// The engine walk.
    pub fts: Fts,
    /// The descriptor seam.
    pub sys: crate::platform::ShellDirSys,
    /// The C `FTS` object.
    pub cfts: *mut Cfts,
    /// Every materialised node, kept alive until close.
    pub nodes: Vec<Box<FtsNode>>,
    /// Node-id to C-pointer lookup for `fts_parent`/`fts_cycle`.
    pub index: Vec<(usize, *mut Cftsent)>,
}

impl ShellFts {
    /// A node already materialised for `id`.
    pub(crate) fn lookup(&self, id: usize) -> Option<*mut Cftsent> {
        let mut index = self.index.len();
        while index > 0 {
            index = index.wrapping_sub(1);
            if let Some((node, pointer)) = self.index.get(index)
                && *node == id
            {
                return Some(*pointer);
            }
        }
        None
    }

    /// Materialise one engine node and return its C pointer.
    pub(crate) fn materialize(&mut self, ent: &FtsEnt) -> *mut Cftsent {
        let mut node = Box::new(build_node(ent, self.cfts));
        let pointer = (&mut node.ent) as *mut Cftsent;
        if let Some(parent) = ent.parent.and_then(|id| self.lookup(id)) {
            node.ent.fts_parent = parent;
        }
        if let Some(cycle) = ent.cycle.and_then(|id| self.lookup(id)) {
            node.ent.fts_cycle = cycle;
        }
        let id = ent.id & 0x7fff_ffff;
        node.ent.reserved[0] = id as c_int;
        self.index.push((ent.id, pointer));
        self.nodes.push(node);
        pointer
    }

    /// `fts_read`.
    ///
    /// # Errors
    /// The engine's error value.
    pub(crate) fn read(&mut self) -> Result<*mut Cftsent, Errno> {
        let snapshot = match self.fts.read(&mut self.sys)? {
            Some(entry) => entry.clone(),
            None => {
                if !self.cfts.is_null() {
                    // SAFETY: the FTS object is alive for the whole walk.
                    unsafe { (*self.cfts).fts_cur = core::ptr::null_mut() };
                }
                return Ok(core::ptr::null_mut());
            }
        };
        let node = self.materialize(&snapshot);
        if !self.cfts.is_null() {
            // SAFETY: the FTS object is alive for the whole walk.
            unsafe { (*self.cfts).fts_cur = node };
        }
        Ok(node)
    }

    /// `fts_children`.
    ///
    /// # Errors
    /// The engine's error value.
    pub(crate) fn children(&mut self, instr: c_int) -> Result<*mut Cftsent, Errno> {
        let snapshots: Vec<FtsEnt> = self
            .fts
            .children(&mut self.sys, instr)?
            .into_iter()
            .cloned()
            .collect();
        let mut first: *mut Cftsent = core::ptr::null_mut();
        let mut previous: *mut Cftsent = core::ptr::null_mut();
        for entry in &snapshots {
            let node = self.materialize(entry);
            if previous.is_null() {
                first = node;
            } else {
                // SAFETY: both nodes are alive in `self.nodes`.
                unsafe { (*previous).fts_link = node };
            }
            previous = node;
        }
        if !self.cfts.is_null() {
            // SAFETY: the FTS object is alive for the whole walk.
            unsafe { (*self.cfts).fts_child = first };
        }
        Ok(first)
    }

    /// `fts_set`.
    ///
    /// # Errors
    /// The engine's error value.
    pub(crate) fn set(&mut self, id: usize, instr: c_int) -> Result<(), Errno> {
        self.fts.set(id, instr)
    }

    /// `fts_close`.
    ///
    /// # Errors
    /// The engine's error value.
    pub(crate) fn close(mut self) -> Result<(), Errno> {
        self.fts.close(&mut self.sys)
    }
}

/// Build one C node from an engine node.
pub(crate) fn build_node(ent: &FtsEnt, cfts: *mut Cfts) -> FtsNode {
    let path = cstr_vec(&ent.path);
    let name = cstr_vec(&ent.name);
    let stat = ent.statp.unwrap_or(Stat::ZERO);
    let mut node = FtsNode {
        ent: Cftsent::zero(),
        path,
        name,
        stat,
    };
    node.ent.fts_number = ent.number;
    node.ent.fts_pointer = ent.pointer as usize as *mut c_void;
    node.ent.fts_path = node.path.as_ptr() as *mut c_char;
    node.ent.fts_accpath = node.ent.fts_path;
    node.ent.fts_name = node.name.as_ptr() as *mut c_char;
    node.ent.fts_statp = (&mut node.stat) as *mut Stat;
    node.ent.fts_errno = ent.errno;
    node.ent.fts_dirfd = ent.dirfd;
    node.ent.fts_pathlen = node.path.len().saturating_sub(1);
    node.ent.fts_namelen = node.name.len().saturating_sub(1);
    node.ent.fts_ino = ent.ino;
    node.ent.fts_dev = ent.dev;
    node.ent.fts_nlink = ent.nlink;
    node.ent.fts_level = ent.level;
    node.ent.fts_info = ent.info;
    node.ent.fts_flags = ent.flags;
    node.ent.fts_instr = ent.instr;
    node.ent.fts_fts = cfts;
    node
}

/// A NUL-terminated copy of `bytes`.
pub(crate) fn cstr_vec(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len().saturating_add(1));
    out.extend_from_slice(bytes);
    out.push(0);
    out
}

/// Build the comparator adapter of `fts_open`.
pub(crate) fn compar_adapter(
    func: extern "C" fn(*const *const Cftsent, *const *const Cftsent) -> c_int,
) -> Compar {
    Box::new(move |a: &FtsEnt, b: &FtsEnt| -> c_int {
        let stat_a = a.statp.unwrap_or(Stat::ZERO);
        let stat_b = b.statp.unwrap_or(Stat::ZERO);
        let path_a = cstr_vec(&a.path);
        let path_b = cstr_vec(&b.path);
        let name_a = cstr_vec(&a.name);
        let name_b = cstr_vec(&b.name);
        let left = plain_node(a, &stat_a, &path_a, &name_a);
        let right = plain_node(b, &stat_b, &path_b, &name_b);
        let left_ref: *const Cftsent = &left;
        let right_ref: *const Cftsent = &right;
        func(&left_ref, &right_ref)
    })
}

/// A stack-only C node whose strings and stat live in the caller's locals.
fn plain_node(ent: &FtsEnt, stat: &Stat, path: &[u8], name: &[u8]) -> Cftsent {
    let mut node = Cftsent::zero();
    node.fts_number = ent.number;
    node.fts_pointer = ent.pointer as usize as *mut c_void;
    node.fts_path = path.as_ptr() as *mut c_char;
    node.fts_accpath = node.fts_path;
    node.fts_name = name.as_ptr() as *mut c_char;
    node.fts_statp = stat as *const Stat as *mut Stat;
    node.fts_errno = ent.errno;
    node.fts_dirfd = ent.dirfd;
    node.fts_pathlen = path.len().saturating_sub(1);
    node.fts_namelen = name.len().saturating_sub(1);
    node.fts_ino = ent.ino;
    node.fts_dev = ent.dev;
    node.fts_nlink = ent.nlink;
    node.fts_level = ent.level;
    node.fts_info = ent.info;
    node.fts_flags = ent.flags;
    node.fts_instr = ent.instr;
    node
}

/// The C `DIR` object.
pub(crate) struct ShellDir {
    /// The C46 stream.
    pub stream: DirStream,
    /// The slot `readdir` returns.
    pub entry: Dirent,
}

impl ShellDir {
    /// Wrap a stream in a C `DIR`.
    pub(crate) fn new(stream: DirStream) -> Box<Self> {
        Box::new(Self {
            stream,
            entry: Dirent::ZERO,
        })
    }
}

/// Copy one parsed entry into a C `struct dirent`.
pub(crate) fn fill_dirent(entry: &DirEntry, out: &mut Dirent) {
    out.d_ino = entry.ino;
    out.d_off = entry.off;
    out.d_reclen = entry.reclen;
    out.d_type = entry.typ;
    let capacity = out.d_name.len().saturating_sub(1);
    let count = entry.name.len().min(capacity);
    let mut index = 0usize;
    while index < count {
        if let (Some(byte), Some(slot)) = (entry.name.get(index), out.d_name.get_mut(index)) {
            *slot = *byte as c_char;
        }
        index = index.wrapping_add(1);
    }
    if let Some(slot) = out.d_name.get_mut(count) {
        *slot = 0;
    }
}

/// Rebuild a `DirEntry` from a C `struct dirent` for the sort comparators.
///
/// # Safety
/// `pointer` is a readable `struct dirent`.
pub(crate) unsafe fn dirent_entry(pointer: *const Dirent) -> Option<DirEntry> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: the caller's struct dirent.
    let dirent = unsafe { &*pointer };
    // SAFETY: `d_name` is a NUL-terminated array inside the struct.
    let name = unsafe { crate::ffi::cstr::slice(dirent.d_name.as_ptr()) }.to_vec();
    Some(DirEntry {
        ino: dirent.d_ino,
        off: dirent.d_off,
        typ: dirent.d_type,
        name,
        reclen: dirent.d_reclen,
    })
}

/// The `AT_FDCWD` constant for the `fts_open` spelling.
pub(crate) const CURRENT_DIR: i32 = AT_FDCWD;

/// The shared `fts_open`/`fts_openat` body.
///
/// # Safety
/// `argv` is a NULL-terminated array of C path strings.
pub(crate) unsafe fn open_common(
    dirfd: i32,
    argv: *const *mut c_char,
    options: c_int,
    compar: Option<extern "C" fn(*const *const Cftsent, *const *const Cftsent) -> c_int>,
) -> Result<*mut Cfts, Errno> {
    if argv.is_null() {
        return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
    }
    let mut refs: Vec<&[u8]> = Vec::new();
    let mut index = 0usize;
    loop {
        // SAFETY: the caller's NULL-terminated argv array.
        let pointer = unsafe { *argv.add(index) };
        if pointer.is_null() {
            break;
        }
        // SAFETY: the slot is a NUL-terminated path.
        refs.push(unsafe { crate::ffi::cstr::slice(pointer) });
        index = index.wrapping_add(1);
        if index > 65_536 {
            return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
        }
    }
    let adapter = compar.map(compar_adapter);
    let mut sys = crate::platform::ShellDirSys;
    let fts = Fts::open(&mut sys, dirfd, &refs, options as u32, adapter)?;
    let cfts = Box::into_raw(Box::new(Cfts::new(options, compar)));
    let shell = ShellFts {
        fts,
        sys,
        cfts,
        nodes: Vec::new(),
        index: Vec::new(),
    };
    let shell = Box::into_raw(Box::new(shell));
    // SAFETY: both boxes were just allocated and are owned by the FTS object.
    unsafe { (*cfts).fts_clientptr = shell as *mut c_void };
    Ok(cfts)
}

/// The shell state behind an `FTS` object.
///
/// # Safety
/// `fts` is a live `FTS` returned by `fts_open`/`fts_openat`.
pub(crate) unsafe fn shell_of<'a>(fts: *mut Cfts) -> Option<&'a mut ShellFts> {
    if fts.is_null() {
        return None;
    }
    // SAFETY: the caller's FTS object.
    let raw = unsafe { (*fts).fts_clientptr } as *mut ShellFts;
    if raw.is_null() {
        None
    } else {
        // SAFETY: `fts_clientptr` is the Box allocated in `open_common`.
        Some(unsafe { &mut *raw })
    }
}

#[allow(dead_code)]
const _: () = assert!(core::mem::size_of::<Cfts>() == 88);
#[allow(dead_code)]
const _: () = assert!(core::mem::size_of::<Cftsent>() == 168);
