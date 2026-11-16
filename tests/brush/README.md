<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# brush tests (K26a)

`bash-compat.sh` is the focused bash-compatibility smoke for the brush
adoption. Run it on the host with a brush binary:

```sh
userland/brush/target/debug/brush tests/brush/bash-compat.sh
```

It covers variables, quoting, a pipeline, redirection, a function, `if`/`for`,
command substitution, exit-status propagation, `set -e` and a bash-style `PS1`
placeholder. The script uses only builtins and `/bin/cat`, so the same file is
suitable for the on-target `shell-brush` qtest once the target build and the
shell-profile wiring are unblocked.

The on-target qtest is present as `tests/brush/shell-brush.toml` but is marked
`skip`: C64g supplies the libc 0.2.186 target types; the target build now stops
in the `mio`/`socket2`/`rustix`/`nix`/`uzers` ravenbsd OS modules and in the
remaining libc network/terminal API. The exact delta is in the K26a report. Installing a
`build/brush` artifact in the shared `shell`/`shell-test` profile now would make
the currently passing `shell-session` qtest depend on a binary that cannot be
built. A dedicated brush profile also needs a new top-level
`profiles/shell-brush.toml` and `etc/services.d/brush.toml`; the shell-profile
switch follows when the target binary builds.
