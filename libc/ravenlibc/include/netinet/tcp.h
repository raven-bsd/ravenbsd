/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/netinet/tcp.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
#ifndef _NETINET_TCP_H
#define _NETINET_TCP_H
#include <sys/cdefs.h>
#include <bits/abi_netinet.h>

enum {
	TCP_NLA_PAD,
	TCP_NLA_BUSY,
	TCP_NLA_RWND_LIMITED,
	TCP_NLA_SNDBUF_LIMITED,
	TCP_NLA_DATA_SEGS_OUT,
	TCP_NLA_TOTAL_RETRANS,
	TCP_NLA_PACING_RATE,
	TCP_NLA_DELIVERY_RATE,
	TCP_NLA_SND_CWND,
	TCP_NLA_REORDERING,
	TCP_NLA_MIN_RTT,
	TCP_NLA_RECUR_RETRANS,
	TCP_NLA_DELIVERY_RATE_APP_LMT,
	TCP_NLA_SNDQ_SIZE,
	TCP_NLA_CA_STATE,
	TCP_NLA_SND_SSTHRESH,
	TCP_NLA_DELIVERED,
	TCP_NLA_DELIVERED_CE,
	TCP_NLA_BYTES_SENT,
	TCP_NLA_BYTES_RETRANS,
	TCP_NLA_DSACK_DUPS,
	TCP_NLA_REORD_SEEN,
	TCP_NLA_SRTT,
	TCP_NLA_TIMEOUT_REHASH,
	TCP_NLA_BYTES_NOTSENT,
	TCP_NLA_EDT,
	TCP_NLA_TTL,
};

#if __BSD_VISIBLE
#define TCPOPT_EOL              0
#define TCPOPT_NOP              1
#define TCPOPT_MAXSEG           2
#define TCPOPT_WINDOW           3
#define TCPOPT_SACK_PERMITTED   4
#define TCPOPT_SACK             5
#define TCPOPT_TIMESTAMP        8
#define TCPOLEN_SACK_PERMITTED  2
#define TCPOLEN_WINDOW          3
#define TCPOLEN_MAXSEG          4
#define TCPOLEN_TIMESTAMP       10

#include <sys/types.h>
#include <sys/socket.h>
#include <stdint.h>

typedef uint32_t tcp_seq;

#define TH_FIN 0x01
#define TH_SYN 0x02
#define TH_RST 0x04
#define TH_PUSH 0x08
#define TH_ACK 0x10
#define TH_URG 0x20

struct tcphdr {
#if __BSD_VISIBLE
#ifdef __GNUC__
	__extension__
#endif
	union { struct {

	uint16_t source;
	uint16_t dest;
	uint32_t seq;
	uint32_t ack_seq;
#if __BYTE_ORDER == __LITTLE_ENDIAN
	uint16_t res1:4;
	uint16_t doff:4;
	uint16_t fin:1;
	uint16_t syn:1;
	uint16_t rst:1;
	uint16_t psh:1;
	uint16_t ack:1;
	uint16_t urg:1;
	uint16_t res2:2;
#else
	uint16_t doff:4;
	uint16_t res1:4;
	uint16_t res2:2;
	uint16_t urg:1;
	uint16_t ack:1;
	uint16_t psh:1;
	uint16_t rst:1;
	uint16_t syn:1;
	uint16_t fin:1;
#endif
	uint16_t window;
	uint16_t check;
	uint16_t urg_ptr;

	}; struct {
#endif

	uint16_t th_sport;
	uint16_t th_dport;
	uint32_t th_seq;
	uint32_t th_ack;
#if __BYTE_ORDER == __LITTLE_ENDIAN
	uint8_t th_x2:4;
	uint8_t th_off:4;
#else
	uint8_t th_off:4;
	uint8_t th_x2:4;
#endif
	uint8_t th_flags;
	uint16_t th_win;
	uint16_t th_sum;
	uint16_t th_urp;

#if __BSD_VISIBLE
	}; };
#endif
};
#endif

#if __BSD_VISIBLE
#define TCPI_OPT_TIMESTAMPS	1
#define TCPI_OPT_SACK		2
#define TCPI_OPT_WSCALE		4
#define TCPI_OPT_ECN		8

enum tcp_fastopen_client_fail {
	TFO_STATUS_UNSPEC,
	TFO_COOKIE_UNAVAILABLE,
	TFO_DATA_NOT_ACKED,
	TFO_SYN_RETRANSMITTED,
};

struct tcp_info {
	uint8_t tcpi_state;
	uint8_t tcpi_ca_state;
	uint8_t tcpi_retransmits;
	uint8_t tcpi_probes;
	uint8_t tcpi_backoff;
	uint8_t tcpi_options;
	uint8_t tcpi_snd_wscale : 4, tcpi_rcv_wscale : 4;
	uint8_t tcpi_delivery_rate_app_limited : 1, tcpi_fastopen_client_fail : 2;
	uint32_t tcpi_rto;
	uint32_t tcpi_ato;
	uint32_t tcpi_snd_mss;
	uint32_t tcpi_rcv_mss;
	uint32_t tcpi_unacked;
	uint32_t tcpi_sacked;
	uint32_t tcpi_lost;
	uint32_t tcpi_retrans;
	uint32_t tcpi_fackets;
	uint32_t tcpi_last_data_sent;
	uint32_t tcpi_last_ack_sent;
	uint32_t tcpi_last_data_recv;
	uint32_t tcpi_last_ack_recv;
	uint32_t tcpi_pmtu;
	uint32_t tcpi_rcv_ssthresh;
	uint32_t tcpi_rtt;
	uint32_t tcpi_rttvar;
	uint32_t tcpi_snd_ssthresh;
	uint32_t tcpi_snd_cwnd;
	uint32_t tcpi_advmss;
	uint32_t tcpi_reordering;
	uint32_t tcpi_rcv_rtt;
	uint32_t tcpi_rcv_space;
	uint32_t tcpi_total_retrans;
	uint64_t tcpi_pacing_rate;
	uint64_t tcpi_max_pacing_rate;
	uint64_t tcpi_bytes_acked;
	uint64_t tcpi_bytes_received;
	uint32_t tcpi_segs_out;
	uint32_t tcpi_segs_in;
	uint32_t tcpi_notsent_bytes;
	uint32_t tcpi_min_rtt;
	uint32_t tcpi_data_segs_in;
	uint32_t tcpi_data_segs_out;
	uint64_t tcpi_delivery_rate;
	uint64_t tcpi_busy_time;
	uint64_t tcpi_rwnd_limited;
	uint64_t tcpi_sndbuf_limited;
	uint32_t tcpi_delivered;
	uint32_t tcpi_delivered_ce;
	uint64_t tcpi_bytes_sent;
	uint64_t tcpi_bytes_retrans;
	uint32_t tcpi_dsack_dups;
	uint32_t tcpi_reord_seen;
	uint32_t tcpi_rcv_ooopack;
	uint32_t tcpi_snd_wnd;
};

struct tcp_md5sig {
	struct sockaddr_storage tcpm_addr;
	uint8_t tcpm_flags;
	uint8_t tcpm_prefixlen;
	uint16_t tcpm_keylen;
	int tcpm_ifindex;
	uint8_t tcpm_key[TCP_MD5SIG_MAXKEYLEN];
};

struct tcp_diag_md5sig {
	uint8_t tcpm_family;
	uint8_t tcpm_prefixlen;
	uint16_t tcpm_keylen;
	uint32_t tcpm_addr[4];
	uint8_t tcpm_key[TCP_MD5SIG_MAXKEYLEN];
};

struct tcp_repair_window {
	uint32_t snd_wl1;
	uint32_t snd_wnd;
	uint32_t max_window;
	uint32_t rcv_wnd;
	uint32_t rcv_wup;
};

struct tcp_zerocopy_receive {
	uint64_t address;
	uint32_t length;
	uint32_t recv_skip_hint;
	uint32_t inq;
	int32_t err;
	uint64_t copybuf_address;
	int32_t copybuf_len;
	uint32_t flags;
	uint64_t msg_control;
	uint64_t msg_controllen;
	uint32_t msg_flags;
	uint32_t reserved;
};

#endif

#endif
