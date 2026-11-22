<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Bidirectional QEMU demo network (tap/bridge)

Status: **host helper and proof implemented.** Host→guest, guest→host, guest internet and the
dnsmasq name lookup are proven on this host. The guest's own name lookup is blocked by the current
release image (numeric-only `ping`, empty static `/etc/resolv.conf`); see section 5.

`scripts/qemu-net.sh up` turns the fixed QEMU demo addresses into a real Linux tap/bridge:

```
tap=ravendemo0 bridge=ravenbr0 guest=10.0.2.15 host=10.0.2.2 dns=10.0.2.3 uplink=enp58s0
qemu: -netdev tap,id=n0,ifname=ravendemo0,script=no,downscript=no \
      -device virtio-net-pci,netdev=n0,disable-legacy=on
```

## 1. Usage

```sh
sudo sh scripts/qemu-net.sh up       # create bridge/tap/NAT/dnsmasq; prints the contract
sudo sh scripts/qemu-net.sh down     # remove everything and restore ip_forward
sh scripts/qemu-net.sh status        # print the contract if up
sh tests/scripts/qemu-net-selftest.sh
```

The normal path needs root. Without root the helper refuses with
`qemu-net: root required (rerun with sudo)` and makes no changes.

`up` selects the host uplink from the IPv4 default route (`ip -4 route show default`) and refuses
if it is missing or ambiguous; it never guesses a random interface. The tap is owned by
`$SUDO_USER` so a user-run QEMU can open it.

## 2. Network

- bridge `ravenbr0`, tap `ravendemo0`;
- bridge addresses `10.0.2.2/24` (guest gateway) and `10.0.2.3/24` (DNS);
- guest keeps `10.0.2.15/24`, gateway `10.0.2.2`, DNS `10.0.2.3` — netd is unchanged;
- `net.ipv4.ip_forward=1` (previous value saved and restored);
- dedicated iptables chains `RAVENBAKER-NAT` (MASQUERADE for `10.0.2.0/24` to the uplink) and
  `RAVENBAKER-FWD` (bridge in/out);
- dnsmasq on `10.0.2.3`, plus a DHCP range for the fixed guest address with DNS option
  `10.0.2.3`. The helper never switches netd to DHCP.

## 3. QEMU for K193 `run.sh`

```
-netdev tap,id=n0,ifname=ravendemo0,script=no,downscript=no
-device virtio-net-pci,netdev=n0,disable-legacy=on
```

Use `-nic none` so QEMU does not also create a slirp NIC. The image is a raw `.img`; the helper
does not build or format it.

## 4. Cleanup

`down` kills dnsmasq, removes both iptables chains and the POSTROUTING/FORWARD jumps, restores the
saved `ip_forward` value, deletes the tap and bridge, and removes the state/config files. It is
idempotent and can be run after a failed `up`; `up` refuses a leftover bridge/tap and asks for
`down` first, so no partial setup is silently reused.

## 5. Proof and limits

`tests/scripts/qemu-net-selftest.sh` (bounded 300 s) runs `up`, boots the release image on the
tap, and proves:

- host `ping -c 3 10.0.2.15` — **PASS**;
- guest `ping -c 3 10.0.2.2` — **PASS**;
- guest `ping -c 3 1.1.1.1` — **PASS**;
- guest `ping -c 3 10.0.2.3` (DNS server reachable) — **PASS**;
- host `dig @10.0.2.3 example.com` — **PASS** (`104.20.23.154`);
- `down` leaves no bridge, tap, dnsmasq or NAT rule — **PASS**.

### K227: deterministic host-reachable address and demo credentials

The guest keeps the fixed `10.0.2.15/24`; the bridge owns `10.0.2.2/24`, so the connected route
is deterministic (`ip route get 10.0.2.15` -> `dev ravenbr0 src 10.0.2.2`). The helper also runs a
DHCP range for the same address, but no hostfwd is used. `scripts/qemu-net.sh --check` verifies
`ip`, `iptables`, `dnsmasq`, `sysctl`, the uplink route and that `ravenbr0`/`ravendemo0` are free,
without root or a boot.

`scripts/qemu-ufs-run.sh` prints:

```
run.sh: guest=10.0.2.15 user=raven password=raven
run.sh: ssh raven@10.0.2.15 ls   # requires K195 sshd + K225 TCP; pending
```

The `raven` account and password are the documented demo credential for K195; this ticket does not
change the OpenSSH profile or claim the login. The host TCP/SSH assertion waits for K225/K195; the
`net-host` qtest runs the bounded host-side tap proof (host ping, guest ping/internet, DNS server,
cleanup) and then boots the release image to prove the getty path is unchanged. No hostfwd
substitution.

### K203 handoff: guest name resolution

This ticket does **not** claim that the guest resolves a name. The exact handoff to K203 is:

- `raven-ping` is numeric-only; `ping -c 3 example.com` fails with `ping: invalid IPv4 address`.
- The release image's static `/etc/resolv.conf` has no `nameserver` line, so the guest resolver
  has no server to query.
- `resolvd` normally takes DNS from the QEMU user-mode DHCP lease; with the tap, `netd` keeps the
  fixed `10.0.2.15/24` and no lease is consumed, so resolvd does not write `/etc/resolv.conf`.
- The DNS server itself is real and proven: the guest reaches `10.0.2.3`, and host
  `dig @10.0.2.3 example.com` returns `104.20.23.154`.

K203 must add a guest resolver path (a static nameserver line in the release image, a
resolv-test guest, or an equivalent first-party client) and re-run the name lookup. K194 makes no
netd or `resolv.conf` change.

No Pi 4 board is involved. The original S95 slirp user-network gate remains available by running
`scripts/qemu-net.sh` without a subcommand.

## 6. Donor reconciliation

First-party shell using Linux `ip`/`iptables`/`dnsmasq` outcomes. Illumos/BSD/Linux are behaviour
references only; no donor code was copied. No C, no product/kernel/boot change.
