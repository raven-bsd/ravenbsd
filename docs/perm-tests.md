<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Adversarial permission and setuid tests (U3d)

OBSERVED on `stuart/U3d` (base `origin/master` `dee52490`). Test-only changes to the `perm` and
`setuid` fixtures; no kernel/libc/server change. No board claim.

## Case matrix

| Case | Input / actor | Expected result | Observed |
|---|---|---|---|
| symlink to 0600 root file | uid 1000 opens `file:/tmp/perm/link600` | `EACCES` | `perm: symlink=denied` |
| `..` path escape | uid 1000 opens `dir755/../../perm/root600` | `EACCES` | `perm: dotdot=denied` |
| deleted cwd | uid 1000 opens a path below removed `/tmp/perm/gone` | `ENOENT` | `perm: deleted-cwd=enoent` |
| duplicate handle | clone a read handle, drop the original, read the clone | read succeeds | `perm: handles duplicate=ok stale=badf` |
| stale handle | read after `close` | `EBADF` | same line |
| extra group list | uid 1000 with supplementary gid 0 reads 0640 root:root file | read succeeds | `perm: group-list=ok` |
| root write execute-only | root opens mode 0111 file for write | allowed | `perm: root ... write=ok` |
| non-root write execute-only | uid 1000 opens mode 0111 file for write | `EACCES` | `perm: xonly-write=denied` |
| setuid directory | uid 1000 `spawn_file` on a 4755 directory handle | `BadElf`, no credential change | `perm: setuid-dir=bad-elf` |
| setgid directory inheritance | uid 1000 creates a file below a 02777 root:root directory | gid 0 expected | **KNOWN-DEFECT: gid 1000** |
| setuid without execute | uid 1000 spawns 4644 ELF | `EACCES`, no child | `setuid: setuid-0644=denied` |
| setuid script | uid 1000 spawns 4755 non-ELF script | `BadElf`, no credential change | `setuid: script=bad-elf` |
| nosuid | uid 1000 spawns 4755 ELF with `nosuid` | euid stays 1000 | `setuid-child: mode=nosuid ... euid=1000` |
| exited child handle | wait then `PROC_HANDLE_INFO`; close; info again | pid then `EBADF` | `setuid: exited-handle=ok` |

The `perm` fixture starts with `initfsd`/`ramfsd`/`vfsd`, creates the adversarial tree as root,
checks the root bypass, sets supplementary group 0, drops to uid/gid 1000, and runs the cases
through `file:` and direct `ramfs:`. The `setuid` fixture installs the same helper ELF at modes
0644/4644/0755/4755/2755 plus a 4755 script and checks the native exec path.

## Known defect (not fixed by this ticket)

**Setgid directory inheritance is missing.** The `ramfsd` handler creates a file with the caller's
`egid` (`create_file_at_owned(..., creds.euid, creds.egid, ...)`) instead of inheriting the
parent directory's group when the directory has `S_ISGID` and the caller is not a member of that
group. The fixture prints:

```
perm: setgid-dir=KNOWN-DEFECT gid=1000
```

Expected gid is 0 (the root:root 02777 directory's group). This is a server-code defect and U3d
must not change server code; it is recorded for a follow-up. The qtest does not assert the line,
so the known-defect case does not weaken the enforced cases above.

## Evidence

```
$ cargo xtask qtest --case perm --arch x86_64
perm: pass
exit 0

$ cargo xtask qtest --case setuid --arch x86_64
setuid: pass
exit 0

$ make check-fast
check-fast: PASSED
```

`build/qtest/perm.json` and `build/qtest/setuid.json`: result `pass`, no unmatched or forbidden
hits. The `setgid-dir=KNOWN-DEFECT` line is intentionally not an expectation.

## Limits

* Test-only fixture changes; no kernel/libc/server behavior was changed.
* The setgid-directory defect above is open.
* No board claim.
