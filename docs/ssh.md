<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH (upstream OpenSSH add-on)

SSH is the upstream OpenSSH 10.6p2 add-on (DR-65). The Raven tree does not vendor a second
implementation and does not implement SSH in Rust. K200 owns `addons/ssh/**` and the source pin;
this profile stages its `sshd`, `ssh` and `ssh-keygen` binaries.

## Build and stage

```sh
sh addons/ssh/build.sh --check
sh addons/ssh/build.sh
```

The build output is `build/ssh/x86_64/bin/{sshd,ssh,ssh-keygen}`. `profiles/release.d/80-ssh.toml`
stages them read-only into the release image, together with:

* `/etc/ssh/sshd_config` — demo policy (`AllowUsers root luser`, `PermitRootLogin yes`,
  `PasswordAuthentication yes`, no PAM, no authorized keys, no sftp server beyond the built-in
  subsystem);
* `/etc/services.d/sshd.toml` — the first-boot wrapper creates the host keys with
  `ssh-keygen -A -f` and execs `sshd`; it needs `netd`, `vfsd`, `consd` and `ptyd`;
* `/etc/services.d/ptyd.toml` — the K196 pty scheme for interactive sessions.

`sshd` and its one-shot host-key service are `required = false`: a failed SSH service is
reported but does not turn a successful login/boot into a failed image. The release-login gate is
therefore unaffected until the SSH primitive is complete.

The release account database stages `/etc/passwd`, `/etc/group` and `/etc/shadow` from
`profiles/ssh.d/`. Besides `root` (uid 0) and the privilege-separation `sshd` (uid 74), it has
the unprivileged demo account `luser` (uid/gid 1000, home `/home/luser`, shell `/bin/rsh`). Both
`root` and `luser` use the same documented development password, `raven`, from the existing
`/etc/shadow` hash; the hash is not repeated here and the plain text is only a demo credential.
Root login and the console root recovery account are kept. Do not expose this demo to an
untrusted network.

## Host keys

The one-shot `ssh-hostkeys` service runs a bounded `/bin/rsh -c` command that creates `/tmp/ssh`
and generates an Ed25519 host key; `sshd` starts after it. The released initfs image has a
read-only `/etc`, so the key lives in `/tmp/ssh` on every boot and is **ephemeral**; SSH clients
must accept a changed key on each boot. A persistent UFS root (K193) must stage the same services
with `/etc/ssh`; that image path is not merged in this tree yet, so host-key persistence is not
claimed here. The per-key `ssh-keygen -q -t ed25519 -N '' -f` form is used because the vendored
build's `ssh-keygen -A -f PREFIX` concatenates PREFIX with the absolute configure-time
sysconfdir.

Private keys are never shipped in the image and must never be committed or published.

## Network

The QEMU default `-netdev user` (slirp) is one-way for inbound connections. Host-to-guest SSH
needs the K194 tap/bridge helper:

```sh
sudo -n sh scripts/qemu-net.sh up
# prints tap=... bridge=... guest=10.0.2.15 host=10.0.2.2 dns=10.0.2.3
```

Then boot QEMU with the printed tap contract and connect from the host:

```sh
ssh -o StrictHostKeyChecking=no root@10.0.2.15 uname -a
ssh -tt root@10.0.2.15
```

`ssh-keygen` and the host `ssh` client are the upstream tools; do not replace the transport or
crypto. Tear the bridge down with `sudo -n sh scripts/qemu-net.sh down`.

## Tests

`tests/ssh/ssh-guest-login.sh` (with `tests/ssh/ssh-guest-login.py`) is the host-side test for the
unprivileged demo account over the K227 guest address. It needs `SSH_GUEST_HOST` (and optionally
`SSH_GUEST_PORT`, default 22); without it the script exits 2 and waits for K227. When K227
publishes the guest address it runs `ssh luser@<guest> ls`, an interactive `ssh -tt` session and
a wrong-password refusal, printing the wall time of each session. It does not substitute the
`hostfwd` port.

The `ssh-login` qtest is intended to cover the host-to-guest command, an interactive `ssh -tt`
session, a wrong-password refusal and host-key persistence. The service cannot be claimed until
the accepted-socket transmit path (K225) delivers the banner; `sshd` privilege separation and the
chroot already run. See the K195 report for the exact primitive and the startup transcript.
