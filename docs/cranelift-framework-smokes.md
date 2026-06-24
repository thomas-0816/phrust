# Cranelift Framework-Like Smokes

Optional 07.CL.F adds offline Composer/framework-like Cranelift smokes without
vendoring Composer packages or real frameworks. The smoke exists to answer which
Phase 7 Big-Win paths fire in small application-shaped code, not to claim broad
framework performance.

## Command

```bash
nix develop -c just jit-cranelift-framework-smoke
```

The target builds `php-vm` with `jit-cranelift`, generates local PHP fixtures
under `target/phase7/cranelift/framework-smoke/fixtures/`, runs each fixture
with JIT off and eager Cranelift, compares exit status/stdout/stderr, and writes
`target/phase7/cranelift/framework-smoke.json`.

## Fixture Families

| Fixture | Shape | Expected Big-Win path |
| --- | --- | --- |
| `router_dispatch` | String route branch dispatching to a controller method. | `method_direct_call` via `direct_call_hits`. |
| `dto_hydration` | Construct DTO objects and read a typed public property through an accessor. | `property_load` via `property_load_fast_hits`. |
| `service_method_loop` | Repeated service method calls over a DTO. | `method_direct_call` via `direct_call_hits`. |
| `template_string_concat` | Template-like loop that appends repeated string fragments. | `string_concat` via `string_concat_fast_path_hits`. |
| `config_array_reads` | Repeated reads from a packed config array. | `packed_array_fetch` via `packed_fetch_fast_hits`. |

The generated report fails if a fixture loses JIT-off/JIT-on parity, if the
Cranelift stats JSON is missing, or if the expected Big-Win counter does not
fire for that fixture.

## Report Interpretation

`framework-smoke.json` contains:

- `required_fixture_kinds`, proving the required prompt categories were
  generated;
- `all_triggered_paths`, summarizing which Big-Win paths fired across the
  smoke;
- one row per fixture with `expected_paths`, `triggered_paths`, output parity,
  and key JIT counters.

The smoke is intentionally offline and local. Generated fixtures and reports
remain under `target/` and must not be committed. Real Composer/framework
benchmark suites remain out of Phase 7 scope until a later phase has broader
workload policy, dependency pinning, and benchmark methodology for them.
