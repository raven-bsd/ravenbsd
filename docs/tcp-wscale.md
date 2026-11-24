# TCP window-scale offer gating

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

RFC 7323 section 2.2: a window-scale option may be sent only when the peer offered it in its SYN.

## Behaviour

- Passive open: the syncache records `window_scale` only when the incoming SYN carried option 3.
  The SYN-ACK therefore has no WS option for a plain SYN and carries our receive scale when the
  SYN offered WS.
- Active open: the SYN offers WS when configured. A SYN-ACK without WS leaves
  `negotiated_wscale = None` and `snd_wscale = 0`; later SYN-ACK emission (`send_syn_ack`) also
  omits WS unless negotiation succeeded.
- Valid WS, SACK, timestamps and MSS negotiation are unchanged.

## Donors

| Area | FreeBSD (transpiled behaviour) | Linux (read-only) | NetBSD (NRL, read-only) |
|---|---|---|---|
| WS offer gate | `sys/netinet/tcp_input.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb`: `tcp_dooptions` sets `TOF_SCALE` only for `TCPOLEN_WINDOW`, and `TF_RCVD_SCALE` only when the peer offered it; the established path scales only with `TF_RCVD_SCALE|TF_REQ_SCALE` | `net/ipv4/tcp_input.c` `TCPOPT_WINDOW`: `wscale_ok` only when the option is present; output adds WS only when negotiated | `sys/netinet/tcp_input.c` comparison only; no code copied |

## Tests

`crates/raven-tcpip/tests/wscale.rs`:
- `plain_syn_gets_a_syn_ack_without_window_scale`
- `syn_with_window_scale_gets_ws_with_our_receive_scale`
- `active_syn_ack_without_ws_leaves_scaling_disabled`

`crates/raven-tcpip/src/tcp_state_ut.rs`:
- `apply_syn_options_without_ws_does_not_negotiate`
- `apply_syn_options_with_ws_negotiates_the_peer_scale`
