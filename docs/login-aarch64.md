<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 release login qtest (K410)

The `login-aarch64` case was stale after K374: it expected the pre-styled single-line banner

```text
RavenBSD 0.1 (aarch64) (tty:console)
```

while the current release image prints the styled screen followed by the normal login prompt. The
case therefore timed out even though the guest booted, logged in and exited cleanly (K096b
evidence, 24.1 s).

## Fixed expectation

`tests/qtest/cases/login-aarch64.toml` now asserts the styled screen, then the login-last contract:

```diff
-[[expect]]
-regex = '^RavenBSD 0\.1 \(aarch64\) \(tty:console\)$'
+[[expect]]
+regex = '^R A V E N   B S D$'
+
+[[expect]]
+regex = '^-{10,}$'
+
+[[expect]]
+regex = '^Welcome to RavenBSD 0\.1 aarch64$'
```

The existing login/password/`raven#` and `init exited (0)` expectations are unchanged. This is not
a weaker "any output" check: it requires the three styled-screen markers, the login prompt, a
successful password, the root prompt and a clean exit.

## Result

```sh
timeout 600 cargo xtask qtest --case login-aarch64 --arch aarch64 --no-build
```

`login-aarch64: pass`, **34.6 s**, all expectations matched, no forbidden lines. Serial
(`tests/evidence/pi4/<sha>/login-aarch64-fixed.log`):

```text
loader: RavenBSD loader 0.1 (aarch64)
RavenBSD kernel 0.1 (aarch64) b04e71ad 2026-11-21
R A V E N   B S D
Welcome to RavenBSD 0.1 aarch64
login: root
Password:
raven#
init exited (0)
```

The boot log also prints the service table (`10 up, 3 failed` on QEMU: pcid/netd/devd, not the Pi 4
board) and `Welcome to the RavenBSD console.`; neither replaces the authentication checks.

## Evidence

Under `tests/evidence/pi4/0b3bb7d1…/`: the fixed serial/JSON, the stale pre-fix serial, the exact
case diff and `SHA256SUMS`. The image is the K096b release image
(`sha256 c6c2527f…83b2a2`); this ticket does not rebuild the guest.

## Scope

Test case and evidence only; the guest, kernel, netd and login implementation are untouched.
