<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K334 RACK misc matrix for the K315 union row

| Function | Rust | Test | Mutation | Status |
|---|---|---|---|---|
| `rack_alloc` | `rack_misc::RackMisc::rack_alloc` | `misc_sendmap_and_lifecycle_matrix` | free-list reuse disabled -> test fails | PASS |
| `rack_alloc_full_limit` | `rack_misc::RackMisc::rack_alloc_full_limit` | `misc_sendmap_and_lifecycle_matrix` | map-entry limit check removed -> test fails | PASS |
| `rack_alloc_limit` | `rack_misc::RackMisc::rack_alloc_limit` | `misc_sendmap_and_lifecycle_matrix` | split limit ignored -> test fails | PASS |
| `rack_free_trim` | `rack_misc::RackMisc::rack_free_trim` | `misc_sendmap_and_lifecycle_matrix` | trim threshold changed -> test fails | PASS |
| `rack_free` | `rack_misc::RackMisc::rack_free` | `misc_sendmap_and_lifecycle_matrix` | unknown-map return changed -> test fails | PASS |
| `rack_clone_rsm` | `rack_misc::RackMisc::rack_clone_rsm` | `misc_sendmap_and_lifecycle_matrix` | clone start not copied -> test fails | PASS |
| `rack_merge_rsm` | `rack_misc::RackMisc::rack_merge_rsm` | `misc_sendmap_and_lifecycle_matrix` | right edge not merged -> test fails | PASS |
| `rack_init_fsb_block` | `rack_misc::RackMisc::rack_init_fsb_block` | `misc_sendmap_and_lifecycle_matrix` | IPv4 header length changed -> test fails | PASS |
| `rack_init_fsb` | `rack_misc::RackMisc::rack_init_fsb` | `misc_sendmap_and_lifecycle_matrix` | mbuf allocation removed -> test fails | PASS |
| `rack_deferred_init` | `rack_misc::RackMisc::rack_deferred_init` | `misc_sendmap_and_lifecycle_matrix` | deferred flag not set -> test fails | PASS |
| `rack_init_retransmit_value` | `rack_misc::RackMisc::rack_init_retransmit_value` | `misc_sendmap_and_lifecycle_matrix` | shape flag mapping changed -> test fails | PASS |
| `rack_init_outstanding` | `rack_misc::RackMisc::rack_init_outstanding` | `misc_sendmap_and_lifecycle_matrix` | outstanding map not created -> test fails | PASS |
| `rack_init` | `rack_misc::RackMisc::rack_init` | `misc_sendmap_and_lifecycle_matrix` | initialized flag not set -> test fails | PASS |
| `rack_fini` | `rack_misc::RackMisc::rack_fini` | `misc_sendmap_and_lifecycle_matrix` | sendmap free count changed -> test fails | PASS |
| `rack_mtu_change` | `rack_misc::RackMisc::rack_mtu_change` | `misc_sendmap_and_lifecycle_matrix` | must-retransmit bit removed -> test fails | PASS |
| `rack_sndbuf_autoscale` | `rack_misc::RackMisc::rack_sndbuf_autoscale` | `misc_sendmap_and_lifecycle_matrix` | autoscale gate removed -> test fails | PASS |
| `rack_set_profile` | `rack_misc::RackMisc::rack_set_profile` | `misc_options_query_and_trace_matrix` | profile 1 flags not set -> test fails | PASS |
| `rack_add_deferred_option` | `rack_misc::RackMisc::rack_add_deferred_option` | `misc_options_query_and_trace_matrix` | queue limit changed -> test fails | PASS |
| `rack_stack_information` | `rack_misc::RackMisc::rack_stack_information` | `misc_options_query_and_trace_matrix` | byte counters swapped -> test fails | PASS |
| `rack_inherit` | `rack_misc::RackMisc::rack_inherit` | `misc_options_query_and_trace_matrix` | copied field count forced zero -> test fails | PASS |
| `rack_apply_deferred_options` | `rack_misc::RackMisc::rack_apply_deferred_options` | `misc_options_query_and_trace_matrix` | deferred queue not drained -> test fails | PASS |
| `rack_set_sockopt` | `rack_misc::RackMisc::rack_set_sockopt` | `misc_options_query_and_trace_matrix` | SET direction rejected -> test fails | PASS |
| `rack_fill_info` | `rack_misc::RackMisc::rack_fill_info` | `misc_options_query_and_trace_matrix` | state field not copied -> test fails | PASS |
| `rack_get_sockopt` | `rack_misc::RackMisc::rack_get_sockopt` | `misc_options_query_and_trace_matrix` | GET value forced zero -> test fails | PASS |
| `rack_ctor` | `rack_misc::RackMisc::rack_ctor` | `misc_options_query_and_trace_matrix` | zero fill removed -> test fails | PASS |
| `rack_dtor` | `rack_misc::RackMisc::rack_dtor` | `misc_options_query_and_trace_matrix` | dtor writes into memory -> test fails | PASS |
| `rack_chg_query` | `rack_misc::RackMisc::rack_chg_query` | `misc_options_query_and_trace_matrix` | sendmap lookup disabled -> test fails | PASS |
| `rack_pru_options` | `rack_misc::RackMisc::rack_pru_options` | `misc_options_query_and_trace_matrix` | OOB rejection removed -> test fails | PASS |
