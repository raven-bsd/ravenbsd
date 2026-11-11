<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Filesystem protocol and open-file contract

Status: K66 contract for protocol version 1.  This document is authoritative for
`raven-proto-fs`, `raven-scheme` and `raven-rt`; S37 implements the kernel half
and K67 makes `initfsd`, `ramfsd` and later `fsd-ufs` conform.  Where this
document and a server disagree, the server is wrong until K67 fixes it.

## 1. Identity model

These are distinct objects.  Do not use one name for two of them.

| Name | Owner | Meaning | Survives DUP | Survives transfer/attachment | Survives client death |
|---|---|---|---|---|---|
| **kernel handle** | kernel process handle table | index into one process's handle table, with rights | no: DUP creates a second kernel handle | transfer creates a handle in the destination table; the source handle is unchanged | no: every handle the process owned is closed exactly once |
| **server file id** | server | server-side lookup key carried in the S11 `file_id`/`server_cookie` fields | no: it is the same id on every duplicate | yes, until the last reference closes | yes, until the last reference closes |
| **open-file description (OFD)** | server | one open: flags, sequential cursor, directory generation | **yes**: all duplicates share one OFD | yes, transferred references share it | released only when the last reference disappears |
| **cursor** | server, inside the OFD | sequential byte position for files; directory position for directories | **yes**, because it lives in the shared OFD | yes | discarded with the last reference |
| **credentials / client identity** | not implemented in this wave | no uid/gid switching, no per-client identity on the wire | n/a | n/a | n/a |

The kernel handle is not a file identity.  A server must key state by its own
server file id and must reference-count the OFD, not the kernel handle.  A server
must never treat the first `CLOSE` as "the file is closed" while another handle
still refers to the same OFD.

## 2. Open flags

The S11 `SchemePacket.flags` field is 16 bits.  It carries the compact wire bit
set below, **not** Linux `O_*` values.  `raven-proto-fs::OpenFlags` is the only
decoder/encoder; unknown bits are rejected (`EINVAL`), never ignored.

| Bit | Wire name | Linux origin | Rule |
|---|---|---|---|
| 0 | `WIRE_OPEN_READ` | `O_RDONLY` (0) / the read half of `O_RDWR` | at least one of READ/WRITE is required |
| 1 | `WIRE_OPEN_WRITE` | `O_WRONLY` / the write half of `O_RDWR` | at least one of READ/WRITE is required |
| 2 | `WIRE_OPEN_CREATE` | `O_CREAT` | creates a missing regular file; incompatible with DIRECTORY |
| 3 | `WIRE_OPEN_EXCL` | `O_EXCL` | requires CREATE; the existence test and creation are one atomic server operation |
| 4 | `WIRE_OPEN_TRUNC` | `O_TRUNC` | requires WRITE; incompatible with DIRECTORY; truncation happens before the open succeeds |
| 5 | `WIRE_OPEN_APPEND` | `O_APPEND` | requires WRITE; every write uses the current end of file, atomically at the server |
| 6 | `WIRE_OPEN_DIRECTORY` | `O_DIRECTORY` | the resolved path must be a directory; incompatible with CREATE/TRUNC |
| 7 | `WIRE_OPEN_NOFOLLOW` | `O_NOFOLLOW` | a final symbolic link fails with `ELOOP`; intermediate links are still followed |

`O_ACCMODE == 3` is invalid.  `raven-proto-fs::OpenFlags::from_linux` and
`from_wire` enforce all of the above and return `EINVAL`.

### Create mode and umask

Protocol version 1 has no per-client umask and no mode field on `OPEN`.
`O_CREAT` creates a regular file with:

```
CREATE_FILE_MODE = 0o666
DEFAULT_UMASK    = 0o022
mode             = 0o644
```

`raven-proto-fs::OpenFlags::create_mode` returns that value.  A client that needs
another mode calls `CHMOD` after the create.  `MKDIR` already carries an explicit
mode.  This is a defined rule, not the old accidental fixed 0644.

### Atomicity

`O_EXCL` and `O_APPEND` are server operations.  A client sequence of STAT then
CREATE, or SEEK then WRITE, is not equivalent and must not be presented as such:

* `O_CREAT|O_EXCL` on an existing path returns `EEXIST` without modifying it.
* Each `O_APPEND` write is placed at the file's length at the time of that write,
  even when two OFDs write concurrently.  The cursor of the writing OFD is left
  at the end of its write.

## 3. Offsets and the I/O model

Protocol version 1 is **sequential**, not positional.  There is no mixed model:

* `READ` and `WRITE` use the cursor in the open-file description.  The wire
  `offset` field must be `0`; a server rejects a nonzero offset with `EINVAL`.
  This applies to regular files and to directories.
* The cursor starts at 0 on `OPEN` and advances by the number of bytes read or
  written.  `O_APPEND` overrides the write position with the current file
  length, then advances the cursor to the new end.
* `DUP` shares the OFD, so a read through either duplicate advances the one
  cursor.  `TRY_CLONE` in `raven-rt` is the same operation at the client API.
* `TRUNCATE` sets the file length; the server sets the OFD cursor to the new
  length when the cursor was beyond it.
* `lseek` is not defined by this protocol version.  Do not invent a wire
  `offset` meaning for files; S37 rejects nonzero offsets explicitly.

Contract test: a sequence of sequential reads returns the same bytes as one
larger read; a nonzero offset `READ` or `WRITE` returns `EINVAL`.

## 4. DUP, transfer and last close

* `DUP` sends `Opcode::Dup` with the same server file id.  The server increments
  the OFD reference count and returns the same id.  The kernel creates a second
  handle; the server creates no second cursor.
* `CLOSE` decrements the reference count.  Only the last close removes the OFD
  and releases unlinked data.  `EBADF` follows an unknown or already released id.
* A transferred or attached reference is another reference to the same OFD; the
  source handle and the destination handle are independent kernel handles.
* Client death closes every handle the process owned exactly once.  The server
  releases the OFD on the last reference, even when the file was unlinked while
  open.  A server must not reclaim data while a transferred reference exists.
* `raven-scheme::OpenTable` implements the reference count and cursor; servers
  should use it instead of an ad-hoc counter.

## 5. Paths

**vfsd normalizes; servers do not each invent a policy.**  The client resolves a
relative path against `PWD` and passes an absolute path.  vfsd applies
`raven-proto-fs::normalize_request`:

| Input | Normalized path | Directory requirement |
|---|---|---|
| `/` | `/` | yes |
| `.` or empty component | the containing directory; a bare `.` is resolved against `PWD` first | yes when it is the final component |
| `..` | pops the previous component; `..` above `/` is `EINVAL` | yes when it is the final component |
| `//` and repeated separators | collapsed | yes only when the input ends in a separator |
| trailing slash (`/a/`) | `/a` | yes: the result must be a directory |
| trailing slash on a file | `/file` | open fails `ENOTDIR` |
| trailing slash on a directory | `/dir` | succeeds |
| trailing slash on a symlink | link target | followed, then the result must be a directory |
| missing component | unchanged | lookup fails `ENOENT` |
| NUL, component > 255, path > 255 | rejected | `EINVAL` / `ENAMETOOLONG` |

The normalized path is split at the longest whole-component mount prefix.  The
owning server receives the path **relative to its mount root** (`/` becomes the
empty path).  Servers reject `.` and `..` components in their relative input as a
defence in depth; they must not resolve `..` out of the mount.

### Symlinks

vfsd performs lexical normalization only; it does not dereference links.  The
owning server resolves symlinks inside its mount:

* A relative target is resolved against the link's directory; a target may not
  escape the mount with `..` (`EXDEV`).
* An absolute target is resolved against the global namespace.  Protocol version
  1 has no cross-mount forwarding in the server; an absolute target that leaves
  the mount fails `EXDEV`.  K16/K67 define the eventual cross-mount rule.
* At most 8 links are followed; more is `ELOOP`.
* `O_NOFOLLOW` makes a **final** symlink fail with `ELOOP`.  It does not stop
  traversal through earlier components.
* A trailing slash implies directory semantics before `O_NOFOLLOW` is applied;
  a final symlink with `O_NOFOLLOW` still fails `ELOOP`.

## 6. Directory reads

Directory reads are sequential, like file reads.  On the first read of a
directory OFD the server records the directory's generation (a counter
incremented on every create, unlink, rmdir, rename or symlink change in that
directory) and its position.  Each `READ` returns zero or more packed
`DirEntry` records and advances the OFD position.

* A zero-byte `READ` means end of directory.
* A buffer smaller than the next complete record returns `EINVAL`; a server must
  not return a partial record.
* If the directory generation changed since the first read, the next `READ`
  through that OFD returns `EINVAL` (documented invalidation).  There is no
  silent skip, duplicate or byte-offset reinterpretation.
* Reopening the directory starts a new OFD and a fresh stream.
* `raven-proto-fs::DirCookie` is the standard `(generation, index)` encoding for
  a server's internal cursor.  Protocol version 1 does not send it on the wire;
  it is not a live byte position.

Readdir entries do not include `.` or `..`; a shell must synthesize them.  Names
are UTF-8, do not contain `/` or NUL, and are at most 255 bytes.

## 7. Permissions

**Mode bits are stored, not enforced, in this wave.**  There are no credentials
on the wire and no uid/gid switching; `STAT` reports `uid = gid = 0`.  A file
with mode `0o000` can still be opened and read by any client, and a mode change
does not grant or deny access.  Do not describe this as POSIX permission
behaviour.

`CHMOD` changes the stored permission bits and `FSTAT`/`STAT` report them.  Access
mode (`READ`/`WRITE` in the open flags) is enforced by the server: writing a
read-only OFD is `EBADF`, reading a write-only OFD is `EBADF`.  That is an access
mode check on the OFD, not a permission check on the inode.  Credential-based
enforcement is deferred to the process/identity work; K66 records the decision so
no server claims more than it does.

## 8. Errors

| Error | Meaning in this contract |
|---|---|
| `EINVAL` | invalid open-flag bits/combinations, nonzero I/O offset, stale directory cursor, invalid path |
| `ENOENT` | a component does not exist |
| `ENOTDIR` | `O_DIRECTORY` or a trailing slash on a non-directory |
| `EISDIR` | write/truncate on a directory |
| `EEXIST` | `O_CREAT|O_EXCL` on an existing path |
| `ELOOP` | `O_NOFOLLOW` on a final symlink, or too many links |
| `EXDEV` | a symlink or rename crosses a mount |
| `EBADF` | unknown server file id, or access mode not granted by the OFD |
| `ENOSYS` | operation not implemented by this server |

## 9. Responsibilities

K66 (this change) defines the contract and implements the shared side:

* `raven-proto-fs`: `OpenFlags`, the wire table, create mode/umask, path
  normalization with trailing-slash intent, `DirCookie`, `OpenTable`-adjacent
  cursor helpers.
* `raven-scheme`: validation of `OPEN` flags before dispatch, `Handler::open_with`,
  `OpenTable`, client `open_flags`.
* `raven-rt`: sequential `File`, `try_clone`, `read_dir`, and Linux-to-wire
  flag translation.
* This document and the affected `docs/man2` pages.

S37 must:

* carry all eight wire bits through `OPEN` and reject unknown bits in the kernel;
* populate the wire offset for `READ`/`WRITE` only as `0`, and reject nonzero
  offsets before the request reaches a server (sequential model);
* make `DUP` share one OFD and close only at the last reference; deliver
  closes on client death;
* keep `docs/man2` in sync with the final kernel wording.

K67 must:

* make `initfsd`, `ramfsd` and `fsd-ufs` use `OpenFlags` and `OpenTable`;
* implement atomic `O_EXCL`/`O_APPEND`, `O_TRUNC`, `O_DIRECTORY`, `O_NOFOLLOW`;
* implement the directory generation invalidation and small-buffer rule;
* remove the fixed 0644 creation mode and the live byte-position readdir
  cursor;
* decide cross-mount symlink forwarding with K16.

## 10. Tests

* `crates/raven-proto-fs`: flag table, round-trip, unknown bits, invalid
  combinations, create mode, path/trailing-slash matrix, `DirCookie`.
* `crates/raven-scheme`: flag validation, `open_with`, `OpenTable` reference
  counting and cursor sharing.
* `tests/fixtures/k66-fs`: fake server and client implementing this document;
  the QEMU case `fs-contract` prints `k66-fs: contract ok`.
