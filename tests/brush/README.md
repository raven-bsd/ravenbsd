<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# brush tests (K26a)

`bash-compat.sh` is the focused bash-compatibility smoke for the brush
adoption. Run it on the host with a brush binary:

```sh
userland/brush/target/debug/brush tests/brush/bash-compat.sh
```

It covers variables, quoting, redirection, a function, `if`/`for`, command
substitution, exit-status propagation, `set -e` and a bash-style `PS1`
placeholder. The script uses only builtins and `/bin/cat`, so the same file is
suitable for the on-target `shell-brush` qtest.

Pipelines are a known platform gap, not hidden: `pipe2(2)` is `ENOSYS` (PG-35)
and a spawned child does not inherit arbitrary descriptors yet (PG-49/PG-50).
A pipeline currently reports `error: i/o error: Function not implemented` and
returns 1; the smoke uses a builtin `echo` and a file round-trip instead.
Enable a pipeline assertion when those platform tickets land.

The on-target qtest is `tests/brush/shell-brush.toml`. It builds the target
binary, boots the `shell-test` image, checks `/bin/brush --version`, runs a
pipeline and then `/etc/brush-compat.sh` through brush. The profile installs
`/bin/brush`; `/bin/rsh` remains the rescue shell and the console session
owner until K26b switches the shell profile to brush.

```
cargo xtask qtest --case shell-brush --arch x86_64
```

`/bin/brush` is built by `cargo xtask build brush --arch x86_64`; that requires
`cargo xtask libc-pack --arch x86_64` first, like the other target userland.

## Host suite (K26f)

brush is a nested Cargo workspace. Run its Rust suite from `userland/brush`, not the repository
root:

```sh
cd userland/brush
cargo nextest run -p brush-shell
```

The K26f base result is **3707 passed, 109 skipped, 0 failed**. The host oracle is GNU bash
5.1.16; compat cases that assert bash 5.2+ behaviour are gated with
`min_oracle_version: "5.2"` and run on a 5.2/5.3 oracle. Known failures remain in the suite as
`known_failure` cases; `Signals::Ctrl+C ends a loop running a program` is skipped on Linux
because the pty harness compares only the process exit status there. The two interactive
`--init-file`/`--rcfile` cases use a quiet `HOME` with `.hushlogin` so the host's system bashrc
does not add its sudo hint to the oracle output.

The nested workspace's `.cargo/config.toml` declares the `ravenbsd` target value for
`--check-cfg`, which removes the vendored `unexpected_cfgs` warnings without `-A warnings`.
See `docs/brush-tests.md` for the warning and target-smoke details.
