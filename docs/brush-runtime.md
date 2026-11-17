# brush static `-c` hang - diagnosis

Date: 2026-11-17.  Base: `origin/master` `0a3fab65`.

## Symptom

`cargo xtask qtest --case modern-shell --arch x86_64` reaches `rsh: ready` and then hangs on
the first spawned command:

```
raven# /bin/brush -c 'echo echo:ok; ...'
```

No output and no next prompt.  `shell-session` hangs on `uname -a`; `base-tools` hangs on
`mkdir /tmp/bt`.  The hang is therefore not brush-specific.

## Isolation

With all three standard descriptors redirected, the same brush command exits normally:

```
raven# /bin/brush -c 'echo brush-redir-ok' < /dev/null > /tmp/u 2>&1
process: child exited (0)
raven# cat /tmp/u
brush-redir-ok
```

The kernel reports `child exited (0)`, `rsh` returns to its prompt, and the file contains the
expected output.  A child with only console descriptors hangs.

## Cause

`userland/bin/rsh/src/main.rs:850-863` builds the child fd map but transfers only
`Stream::Handle` entries:

```rust
for fd in [0u8, 1, 2] {
    let stream = stream_at(io, fd).unwrap_or(Stream::Console);
    if let Stream::Handle(handle) = stream {
        ...
        fdmap.push((i32::from(fd), handle.0));
        env.push(format!("RSH_FD{fd}={child}"));
    }
}
```

The default `Stream::Console` is skipped.  The child starts with an empty `RFDM` map, and
`libc` step 10 (`start.rs::ensure_standard_fds`) then tries to open `tty:console` for fds 0-2.
That open does not complete while the parent `rsh` owns the console, so every spawned program
stalls before `main`.

## Owner fix (outside K26e Owns)

`userland/bin/rsh/src/main.rs` (shell lane; C60/C61), not `userland/brush`, `crates/raven-spawn`
or the libc runtime:

1. pass the session `tty` handle into `spawn_program` (the caller already has it; `Io` uses
   `Stream::handle(tty)`);
2. for every fd in 0,1,2, map `Stream::Console => tty` and `Stream::Handle(h) => h`;
3. push all three handles into `fdmap` and set the matching `RSH_FD<n>` environment entries;
4. spawn as before.

With that fd map, the child receives the console handles instead of reopening the console and
the three qtests can exercise brush, uname and mkdir normally.

## Acceptance on this base

- `modern-shell`, `shell-session`, `base-tools`: FAIL at the first spawned command (same hang).
- `make check-fast`: PASSED.
- `libc-pack`, kernel, loader and `build user`: exit 0.

This file records the diagnosis for the owning lane; no rsh source is changed from K26e.
