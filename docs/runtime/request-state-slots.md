# Typed Request-State Slots

`RequestState` is the sole owner of registered request-local extension values.
An immutable `ExtensionStateLayout` assigns each enabled state a numeric,
type-bound slot during registry assembly. Request creation walks that frozen
layout once; builtin calls use the stored slot index and a checked downcast.
There is no name lookup or state allocation in the call path.

The VM's request-local `BuiltinAdapterState` owns one `BuiltinRequestState` per
execution request. `ExecutionState` owns that adapter as a single subsystem;
it does not expose extension state as flat execution fields. A `BuiltinContext`
created by VM dispatch borrows the typed request-state owner. The owned context
constructor exists for isolated builtin tests, but its enum representation is
exclusive: a context is either the owner or a borrower and never stores both.

## Migrated Views

| Extension | Request state | Implementation view |
| --- | --- | --- |
| JSON | `JsonRequestState` | `JsonBuiltinServices` exposes only JSON error state |
| PCRE | `PcreRequestState` | `PcreBuiltinServices` exposes PCRE cache/error, INI, and diagnostics; `PcreCallbackServices` additionally exposes explicit callback invocation |
| cURL | `CurlState` | `CurlBuiltinServices` exposes cURL handles, output/diagnostics, and the network capability bit |

These states are absent from `BuiltinExtensionState`; their old broad
`BuiltinContext` accessors are removed. Multi-state access uses
`RequestState::get_pair_mut`, which splits the slot vector safely and rejects
identical or foreign-layout slots.

APCu metadata explicitly requests `ProcessSharedState`. Its request slot holds
a handle to process-local cache storage, so dropping one request owner drops
the handle but not the shared cache. The registry tests cover this distinction.

## Allocation Report

Measurements are payload sizes from `ExtensionStateLayout::payload_bytes()`;
they exclude the slot vector and allocator bookkeeping.

| Layout | Enabled states | Slots | Payload bytes (64-bit) |
| --- | --- | ---: | ---: |
| Minimal extension registry | none | 0 | 0 |
| Default external registry | APCu; ctype is stateless | 1 | 8 |
| Migrated core builtin layout | PCRE, JSON, cURL | 3 | 200 |

The values are asserted by `php_extensions` and `php_runtime` unit tests.
Disabled extensions therefore contribute neither a slot nor a state payload.

## Dispatch Measurement

`performance/request_state_json_dispatch` in
`crates/php_bench/benches/perf_hotpaths.rs` invokes the already-resolved
`json_last_error` function pointer repeatedly on one request owner. It isolates
the generic builtin ABI plus direct typed-slot service-view construction; the
empty argument vector has no heap allocation. Run the same benchmark at the
baseline and candidate revisions:

```bash
nix develop -c cargo bench --manifest-path crates/php_bench/Cargo.toml \
  --bench perf_hotpaths -- request_state_json_dispatch
```

Measured on 2026-07-11 on an Apple M4 running macOS 26.5. The candidate was the
Prompt 08 worktree based on `68bef069`; the baseline was `44333dc5`, before the
narrow service-view adapters. Both revisions used the same benchmark source
and lockfile.

| Revision | Estimate interval | Median | Change |
| --- | ---: | ---: | ---: |
| `44333dc5` baseline | 7.0909-7.4670 ns | 7.2737 ns | - |
| Prompt 08 candidate | 5.7843-5.8246 ns | 5.8007 ns | -20.3% |

This is machine-specific evidence, not a permanent threshold. The executable
benchmark prevents the measurement from becoming a prose-only claim and
guards the adapter boundary against accidental per-call lookup or allocation.

## Legacy Adapter Removal List

`BuiltinExtensionState` is a temporary adapter for modules not migrated by
Prompt 08. Each item below names its exact current field ownership; migration
removes both members of every fallback/borrow pair together.

| Owner | Fields to remove together |
| --- | --- |
| BCMath | `bcmath_scale` |
| strtok | `strtok_state` |
| iconv | `iconv_state`, `iconv_state_slot` |
| legacy APCu | `apcu_state`, `apcu_state_slot` |
| OPcache | `opcache_state`, `opcache_state_slot` |
| SOAP | `soap_state`, `soap_state_slot` |
| OpenSSL | `openssl_error_state`, `openssl_error_state_slot` |
| gettext | `gettext_state`, `gettext_state_slot` |
| shmop | `shmop_state`, `shmop_state_slot` |
| readline | `readline_state`, `readline_state_slot` |
| SysV message queues | `sysvmsg_state`, `sysvmsg_state_slot` |
| SysV semaphores | `sysvsem_state`, `sysvsem_state_slot` |
| SysV shared memory | `sysvshm_state`, `sysvshm_state_slot` |
| PCNTL | `pcntl_state`, `pcntl_state_slot` |
| FTP | `ftp_state`, `ftp_state_slot` |
| IMAP | `imap_state`, `imap_state_slot` |
| LDAP | `ldap_state`, `ldap_state_slot` |
| SSH2 | `ssh2_state`, `ssh2_state_slot` |
| sockets | `socket_state`, `socket_state_slot` |
| POSIX | `posix_last_error` |
| mbstring encoding | `mb_internal_encoding`, `mb_internal_encoding_slot` |
| mbstring substitution | `mb_substitute_character`, `mb_substitute_character_slot` |
| MySQL | `mysql_state` |
| PostgreSQL | `postgres_state` |

The `request-state-boundaries` gate derives this field list from the struct and
fails when a field is added without a concrete removal entry. It also fails if
JSON, PCRE, or cURL state returns to the legacy adapter.
