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

* `/etc/ssh/sshd_config` — root-only demo policy (`PermitRootLogin yes`,
  `PasswordAuthentication yes`, no PAM, no authorized keys, no sftp server beyond the built-in
  subsystem);
* `/etc/services.d/sshd.toml` — the first-boot wrapper creates the host keys with
  `ssh-keygen -A -f` and execs `sshd`; it needs `netd`, `vfsd`, `consd` and `ptyd`;
* `/etc/services.d/ptyd.toml` — the K196 pty scheme for interactive sessions.

The root account uses the existing development password from `/etc/shadow`; the password is not
repeated here. Do not expose this demo to an untrusted network.

## Host keys

The `sshd` service runs a bounded `/bin/rsh -c` first-boot command. The released initfs image has
a read-only `/etc`, so it generates the keys in `/tmp/ssh` on every boot and its host key is
**ephemeral**; SSH clients must accept a changed key on each boot. A persistent UFS root (K193)
must stage the same service with `/etc/ssh`; that image path is not merged in this tree yet, so
host-key persistence is not claimed here. The per-key `ssh-keygen -q -t TYPE -N '' -f` form is
used because the vendored build's `ssh-keygen -A -f PREFIX` concatenates PREFIX with the absolute
configure-time sysconfdir.

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

The `ssh-login` qtest is intended to cover the host-to-guest command, an interactive `ssh -tt`
session, a wrong-password refusal and host-key persistence. The service cannot be claimed until
the AF_UNIX `socketpair` backend used by `sshd` privilege separation is merged and the guest
image boots the service. See the K195 report for the exact primitive and the startup transcript.
