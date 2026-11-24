// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The OS-independent RavenBSD TCP/IP stack boundary (contract C15).
//!
//! The native stack is a standalone, OS-independent library. It never calls the kernel, never reads a
//! clock and never allocates frames from a driver. Everything crosses the boundary defined here.
//!
//! **Time.** [`Instant`] and [`Duration`] are plain wrappers over `i64`/`u64` milliseconds. The
//! caller supplies `now` to every entry point; the stack never asks for the time.
//!
//! **Link layer.** The stack owns no driver. It talks to one trait:
//! [`Device`]. [`Device::receive`] hands the next frame to a callback and returns `true`, or returns
//! `false` when the queue is empty. [`Device::transmit`] reserves `len` bytes, lets a callback write
//! them, then queues the frame.
//!
//! **Driving the stack.** [`Stack::poll`] is the only thing that moves packets or runs timers.
//! [`Stack::next_poll_delay`] returns `None` when nothing is scheduled. [`Stack::take_events`]
//! reports readiness **transitions**, not levels.
//!
//! [`Event`] keeps the shape `raven-netcore` already publishes, so the socket layer above does not
//! change when the backend changes. Errors are [`Errno`], mapped exactly as B07 mapped them.
//!
//! **Socket API.** [`Stack::tcp_listen`], [`Stack::tcp_accept`], [`Stack::tcp_connect`],
//! [`Stack::send`], [`Stack::recv`], [`Stack::shutdown_write`], [`Stack::close`],
//! [`Stack::udp_bind`], [`Stack::udp_send_to`] and [`Stack::udp_recv_from`] are all non-blocking,
//! all `Result<_, Errno>`, `EAGAIN` for "would block", and `Ok(0)` from `recv` for an orderly EOF.
//!
//! **Rules.** `no_std` + `alloc`, `#![forbid(unsafe_code)]`, no threads, no interior wall-clock
//! reads, no `unwrap`/`expect` on anything that came off the wire. No public type from any
//! third-party crate appears in the public API.
//!
//! **Memory limits.** Every queue has a byte cap from [`StackConfig`]. Exceeding it drops and counts;
//! it never allocates without bound and never panics. No buffer is ever sized from a value the peer
//! supplied.

#![no_std]
#![forbid(unsafe_code)]
#![allow(missing_docs)] // Socket operation names are documented at module level; C15 is copied above.

extern crate alloc;

mod cc;
mod device;
mod dhcp;
mod dhcp_stack;
mod ecn;
mod event;
mod hostcache;
mod icmp_stack;
mod ipv4_stack;
mod ipv6_stack;
mod isn;
mod listen;
#[cfg(feature = "mptcp")]
pub mod mptcp_bridge;
mod mptcp_hooks;
#[cfg(feature = "mptcp")]
pub mod mptcp_socket;
mod neighbor;
mod observability;
mod plpmtud_live;
mod pmtu;
mod rack_bbr;
mod rack_hpts;
mod rack_log;
mod rack_misc;
mod rack_os;
mod rack_output;
mod rack_pipe;
mod rack_retransmit;
mod rack_sack;
mod rack_state;
mod rack_transport;
mod rack_types;
mod rack_vtable;
mod readiness;
mod reassembly;
mod recovery;
mod route;
mod rto;
mod sack;
mod sack_send;
mod seq;
mod signature;
mod socket;
mod stack;
mod syncache;
mod syncookie;
mod table;
mod tcp_data;
mod tcp_mss;
mod tcp_notify;
pub mod tcp_respond;
mod tcp_state;
mod tcp_timer;
mod time;
mod udp_stack;

pub use raven_abi::errno::Errno;

pub use cc::{CcContext, CongestionControl, Cubic, LossKind, NewReno, Prr, icbrt};
pub use device::{ChecksumOffload, Device, DeviceCaps, Medium, TxError};
pub use dhcp::{
    DhcpTimers, Event as DhcpEvent, Lease as DhcpLease, RetryConfig as DhcpRetryConfig,
};
pub use ecn::{
    ECN_CE, ECN_ECT0, ECN_ECT1, ECN_MASK, ECN_NOTECT, EcnCounters, EcnState, TH_ACK, TH_AE, TH_CWR,
    TH_ECE, TH_SYN, get_ace, set_ace,
};
pub use event::Event;
pub use event::{PollResult, SocketId};
pub use hostcache::{
    DEFAULT_BUCKET_LIMIT, DEFAULT_EXPIRE_MS, DEFAULT_HASHSIZE, DEFAULT_PRUNE_MS, HostKey,
    HostMetrics, Hostcache,
};
pub use ipv6_stack::{
    IpAddr, Ipv6Addr, Ipv6AddressState, Ipv6Cidr, Ipv6Config, Neighbor6Cache, Neighbor6Entry,
    Neighbor6State,
};
pub use isn::{IsnConn, IsnEntropy, IsnState};
pub use listen::{AcceptQueue, HalfOpen, HalfOpenQueue, IsnGenerator, ListenStats, SynCookie};
#[cfg(feature = "mptcp")]
pub use mptcp_bridge::StackSubflow;
pub use mptcp_hooks::{SubflowId, SubflowOps};
#[cfg(feature = "mptcp")]
pub use mptcp_socket::{AF_INET, AF_INET6, IPPROTO_MPTCP, MptcpSocket, SubflowInfo};
pub use neighbor::{NeighborCache, NeighborEntry};
pub use observability::{
    Counters, HostcacheInfo, OptionFlags, PropKind, PropertyInfo, SocketInfo, Stability, Unit,
};
pub use pmtu::{
    IPV4_FLOOR, IPV6_FLOOR, IpFamily, MssUpdate, MtuCache, Plpmtud, PlpmtudState, TCP_MINMSS,
    TCP_MSS_DEFAULT, floor_for, mss_headers, mss_update, mssopt, next_pmtu, next_pmtu6,
    on_icmp_frag_needed, on_icmp6_frag_needed, probe_sequence,
};
pub use rack_bbr::{
    BBR_UNIT, BbrLogEvent, BbrRack, BbrTunables, GpSample, HYBRID_CSPR, HYBRID_CU, HYBRID_ENABLE,
    HYBRID_H_MS, HYBRID_SETMSS, HYBRID_WASSET, HybridRequest, MIN_GP_WIN, Quality, RACK_MIN_BW,
    RACK_REQ_AVG, RACK_TIMELY_CNT_BOOST, SendfileTrack, TF_GPUTINPROG,
};
pub use rack_hpts::{
    HPTS_USECS_PER_SLOT, HptsCallback, NUM_OF_HPTSI_SLOTS, PACE_TMR_DELACK, PACE_TMR_KEEP,
    PACE_TMR_MASK, PACE_TMR_PERSIT, PACE_TMR_RACK, PACE_TMR_RXT, PACE_TMR_TLP, RackHpts,
    RackTimeout, hpts_usec_to_slots, rack_timeout_action,
};
pub use rack_log::{Counter as RackLogCounter, LogEntry as RackLogEntry, RackLog};
pub use rack_misc::{
    ACK_FLAG, DeferredOption, FIN_FLAG, FsbBlock, MiscQuery, RACK_DEFERRED_MAX, RACK_FREE_CACHE,
    RACK_FREE_CNT_MAX, RST_FLAG, RackMisc, RackTcpInfo, TCP_DEFER_OPTIONS, TCP_NO_PRR,
    TCP_RACK_ABC_VAL, TCP_RACK_DSACK_OPT, TCP_RACK_FORCE_MSEG, TCP_RACK_HI_BETA,
    TCP_RACK_MBUF_QUEUE, TCP_RACK_MEASURE_CNT, TCP_RACK_PACE_TO_FILL, TCP_RACK_PROFILE,
    TCP_RACK_SET_RXT_OPTIONS, TCP_RACK_TIMER_SLOP, TCP_USE_CMP_ACKS,
};
pub use rack_os::{
    CounterU64, MbufId, PacingMethod, RackOs, RackOsError, RackStacks, RackTcpcb, SysctlValue,
};
pub use rack_output::{RackOutput, TimerResult};
pub use rack_pipe::{
    CC_NDUPACK, EXIT_RECOVERY_MASK, IN_RECOVERY_MASK, RackPipe, RackPipeState, TF2_ECN_PERMIT,
    TF2_ECN_SND_CWR, TLP_USE_ID, TLP_USE_TWO_ONE, TLP_USE_TWO_TWO, tstmp_ge, tstmp_lt,
};
pub use rack_retransmit::{
    AckOutcome, CUM_ACKED, DUP_ACK_THRESHOLD, EntryResult, FastAck, MAX_LINKHDR, MHLEN, ProcessAck,
    RackRetransmit, RackSendMap, RackTransmission, RetransmitError, SACKED, TH_PUSH,
};
pub use rack_sack::{RACK_DSACK_HANDLING, RackSack};
pub use rack_state::{
    TCPS_CLOSING, TCPS_FIN_WAIT_1, TCPS_FIN_WAIT_2, TCPS_LAST_ACK, TCPS_TIME_WAIT,
    rack_do_close_wait, rack_do_closing, rack_do_compressed_ack_processing, rack_do_established,
    rack_do_fastnewdata, rack_do_fin_wait_1, rack_do_fin_wait_2, rack_do_lastack, rack_do_segment,
    rack_do_segment_nounlock, rack_do_syn_recv, rack_do_syn_sent, rack_do_win_updates,
    rack_handle_delayed_ack, rack_handle_might_revert, rack_handle_probe_response,
    rack_process_data, rack_process_option,
};
pub use rack_transport::{
    AddRackMode, AddRackOutcome, HptsStart, HptsStartOutcome, HwTlsOutcome, MAX_HPTS_TIMEOUT_US,
    RackTransport, SwitchCaps, SwitchFailedOutcome, TimerProcess, TimerProcessResult,
    TimerStopOutcome, rack_ctloutput, rack_hw_tls_change, rack_process_timers, rack_pru_options,
    rack_start_hpts_timer, rack_stop_all_timers, rack_switch_failed, rack_wake_check, tcp_addrack,
};
pub use rack_types::{
    PACE_MAX_IP_BYTES, RACK_ACKED, RACK_APP_LIMITED, RACK_DEFERRED, RACK_DELAYED_ACK_TIME,
    RACK_HAD_PUSH, RACK_HAS_FIN, RACK_HAS_SYN, RACK_IN_GP_WIN, RACK_INITIAL_RTO, RACK_IS_PCM,
    RACK_MERGED, RACK_MUST_RXT, RACK_NUM_OF_RETRANS, RACK_OVERMAX, RACK_PMTU_CHG,
    RACK_PRR_ADDBACKMAX, RACK_REORDER_FADE_DEFAULT, RACK_RTO_MAX, RACK_RTO_MIN,
    RACK_RWND_COLLAPSED, RACK_SACK_PASSED, RACK_SENT_FP, RACK_SENT_SP, RACK_SENT_W_DSACK,
    RACK_SHUFFLED, RACK_STRADDLE, RACK_TLP, RACK_TLP_MIN, RACK_TO_REXT, RACK_WAS_ACKED,
    RACK_WAS_LOST, RACK_WAS_SACKPASS, RackSendmap, TF_CONGRECOVERY, TF_FASTRECOVERY,
    TF_WASCRECOVERY, TF_WASFRECOVERY,
};
pub use rack_vtable::{
    IPPROTO_TCP, MAX_USER_SET_SEG, PACE_PKT_OUTPUT, PRUS_OOB, RACK_DEFAULT_PACING_DIVISOR,
    RACK_RXT_SCOREBOARD_CLEAR_THRESH, RL_MIN_DIVISOR, RackFunctionBlock, RackOptions,
    RackPortState, RackQuery, RackSegment, RackSegmentResult, RackSockopt, RackStack,
    RackStackInfo, RackVtable, SockoptDir, TCP_FUNC_DEFAULT_OK, TCP_FUNC_OUTPUT_CANDROP,
    TCP_FUNCTION_NAME_LEN_MAX, TCP_INFO, TCP_PACING_DND, TCP_QUERY_RACK_TIMES, TCP_QUERY_SENDMAP,
    TCP_QUERY_TIMERS_UP, TCP_RACK_PACE_MAX_SEG, TCP_RACK_PACE_MIN_SEG, TCP_RACK_PACING_BETA,
    TCP_RACK_PACING_DIVISOR, TCPS_CLOSE_WAIT, TCPS_CLOSED, TCPS_ESTABLISHED, TCPS_LISTEN,
    TCPS_SYN_RECEIVED, TCPS_SYN_SENT, TF_SACK_PERMIT, TF_SENTFIN, TF2_MBUF_ACKCMP,
    TF2_SUPPORTS_MBUFQ, TH_FIN, TH_RST,
};
pub use readiness::Readiness;
pub use recovery::{
    Ecn, Rack, RackCongOutcome, RackContext, RackEntry, RackOutcome, RackSignal, SpuriousUndo, Tlp,
    pto,
};
pub use route::{Route, RouteTable};
pub use rto::RtoEstimator;
pub use sack::{NextKind, NextSeg, SackResult, SackScoreboard, receiver_blocks, sack_option_bytes};
#[cfg(feature = "tcp-md5")]
pub use signature::md5;
pub use signature::{AoAlgorithm, AoKey, DigestProvider, NoDigest};
#[cfg(feature = "tcp-ao")]
pub use signature::{hmac_sha1, hmac_sha256};
pub use stack::{Ipv4Addr, Ipv4Config, MAX_TIMERS, Stack, StackConfig, TimerId};
pub use syncookie::{
    CookieAck, CookieConn, CookieRequest, ExpandedCookie, MSS_TABLE, SYNCOOKIE_LIFETIME_MS,
    SYNCOOKIE_SECRET_SIZE, SyncacheSnapshot, Syncookie, SyncookiePolicy, SyncookieSecret,
    TCP_MAX_WINDOW, WSCALE_TABLE, syncookie_cmp,
};
pub use table::{
    ConnectionKey, ConnectionTable, ListenerKey, ListenerTable, PortAllocator, Rebind,
    TimeWaitTable, rebind_decision,
};
pub use tcp_mss::{
    MssOptions, MssState, TCP_MAXOLEN, TCP6_MSS_DEFAULT, mss_from_peer, option_length,
    pad_option_length, tcp_fixed_maxseg, tcp_maxseg,
};
pub use tcp_notify::{TcpNotify, TcpNotifyOutcome, TcpStateChange};
pub use tcp_state::TcpState;
pub use tcp_timer::{
    KeepOutcome, RexmtAction, RexmtOutcome, TimerState, TwoMslOutcome, tcp_timer_2msl,
    tcp_timer_keep, tcp_timer_rexmt,
};
pub use time::{Duration, Instant};

#[cfg(any(test, feature = "testutil"))]
pub mod testutil;

#[cfg(any(test, feature = "testutil"))]
pub use testutil::{LoopbackDevice, NullDevice};
