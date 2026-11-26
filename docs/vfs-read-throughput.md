<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# vfsd large-read throughput (K141)

Status: **OBSERVED at base `0b6fc0d4`.** The `file:` proxy staged every 4 KiB through a
stack buffer, so a 3 MiB read was 738 nested scheme reads. The proxy chunk is now 64 KiB on the
heap; a 3 MiB read is 47 calls. No protocol, cache or file-semantics change.

## 1. Before and after

`copy_read` used `BUFFER = 4096` and a `[0u8; 4096]` stack scratch. A 64 KiB client read became
16 nested `PREAD` calls to the backing server. The 3,019,776-byte `/lib/libc.so.1` therefore
needed `ceil(3,019,776 / 4,096) = 738` proxy calls.

The new `PROXY_CHUNK = 64 * 1024` uses a heap `vec![0u8; PROXY_CHUNK]`; the directory staging
buffer remains a 4 KiB stack array (`DIR_BUFFER`). `copy_write` uses the same heap chunk. A 3 MiB
read is `ceil(3,019,776 / 65,536) = 47` calls, a **15.7x** reduction.

The host unit tests in `userland/servers/vfsd/src/chunk_ut.rs` encode the count:

```text
64 KiB chunk: 47 calls for 3,019,776 bytes
 4 KiB chunk: 738 calls for 3,019,776 bytes
short read:  ends the copy at the source length
overrun:     rejected as EPROTO
empty copy:  zero calls
```

## 2. Server and stack evidence

- `initfsd`/`ramfsd` read models copy `min(out.len(), available)`, so they return the full 64 KiB
  chunk; no server-side cap change was needed.
- `MAX_TRANSFER` is 4 MiB and the kernel capture window is 4 MiB, so a 64 KiB capture is within
  the existing ABI.
- vfsd has a 64 KiB user stack; the 64 KiB staging buffer is allocated on the heap, not on the
  stack. The directory page stays a 4 KiB stack array.

Measured read through `file:` at the shell-test image:

```text
$ wc -c /lib/libc.so.1
3019776 /lib/libc.so.1
before: 0.693 s (one run, 4 KiB proxy)
after:  0.635 s (one run, 64 KiB proxy)
```

The wall-clock difference is small because the kernel capture/copy cost still dominates; the
call-count reduction is the guaranteed outcome. `cat` of the same file to a ramfs file is much
slower and is dominated by the write path, so it is not used as the read bound.

## 3. Regression

- `cargo test -p raven-vfsd` (host): the `chunk_ut.rs` call-count, short-read, overrun and empty
  tests above.
- `tests/qtest/cases/fs-read-throughput.toml` (x86_64, shell-test): four 3 MiB reads through
  `file:` must return `3019776` four times and reach the prompt within the 4 s step bound
  (measured one read ~0.64 s, four ~2.5 s). This also proves the data path returns the full file.
- `base-tools` must stay green; it does.

## 4. Donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Chunked file read | Illumos `pread`/`read` copy through a caller buffer; no 4 KiB proxy tier. Behaviour only. | FreeBSD/NetBSD `read`/`pread` copy the caller length through `uiomove`; permissive BSD, not copied. | Page-cache/readahead outcome only; no code. | First-party chunk loop in vfsd, 64 KiB heap scratch; no donor code. |
| Short read | POSIX read returns the short count at EOF; user loops. | Same POSIX contract. | Same. | A short chunk ends the copy and is returned as the short count, preserving semantics. |
| Large-capture bound | Not applicable. | Not applicable. | Not applicable. | The existing 4 MiB `MAX_TRANSFER` and capture window bound the chunk; 64 KiB is within them. |
