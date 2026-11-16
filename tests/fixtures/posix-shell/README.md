<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# posix-shell fixture (P4a)

`posix-shell.sh` is the first-party POSIX shell conformance script. It prints one marker per
requirement family and uses shell builtins only.

## Host

```sh
userland/brush/target/debug/brush --posix tests/fixtures/posix-shell/posix-shell.sh
```

On the P4a base the output matches `bash --posix` (GNU bash 5.1.16) line for line except `$$`,
which is the running pid:

```
simple:1:2
status:1
and:ok
...
split:3:a:b:c
read:redir
here:here:body
errexit:1
nounset:1
xtrace:0
eval:ok
ro:1
umask:0022
getopt:a:
getopt:b:val
optind:4
pid:<pid>
done
trap:exit
```

`read:redir` and the here-doc line are host evidence; the target qtest omits the file-output and
here-doc groups until the target `>` write and script/source stdout findings in
`docs/posix/P4-shell.md` are fixed.

## Target

The QEMU case is `tests/qtest/cases/posix-shell.toml`. It boots the opt-in
`modern-shell-test` image (rsh owns the console, brush is installed), runs four
`/bin/brush --posix -c` groups and checks the same markers for grammar, expansions,
input/exec redirection, functions, loops, `set -e/-u/-x`, EXIT traps, `eval`, `readonly`,
`umask`, `getopts` and `$$`.

```sh
cargo xtask qtest --case posix-shell --arch x86_64
```

The `posix-check` row overlay is `docs/posix/evidence/P4.toml` (P0b); it is not merged at this
base. The proposed row matrix and waivers are in `docs/posix/P4-shell.md`.
