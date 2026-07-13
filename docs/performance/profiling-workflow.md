# Performance Optional Profiling Workflow

The performance layer provides maintainer-only profiling recipes. They are not part of
`verify-performance` and they skip by default so normal development stays fast.

## Recipes

Run the recipe first to see the exact local command and tool availability:

```bash
nix develop -c just profile-dispatch
nix develop -c just profile-arrays
nix develop -c just profile-calls
nix develop -c just profile-composer
```

To actually collect local profiler output, opt in explicitly:

```bash
nix develop -c env PHRUST_PERF_PROFILE_RUN=1 just profile-dispatch
```

All outputs go under `target/performance/profiles/` and must not be committed.

## Scenarios

| Recipe | Fixture | Purpose |
| --- | --- | --- |
| `profile-dispatch` | `tests/fixtures/performance/perf_smoke/loops.php` | VM dispatch and loop overhead |
| `profile-arrays` | `tests/fixtures/performance/perf_smoke/arrays_mixed.php` | array-heavy reads, writes, and count paths |
| `profile-calls` | `tests/fixtures/performance/perf_smoke/function_calls.php` | user/internal call dispatch |
| `profile-composer` | `tests/fixtures/stdlib/corpus/container_autoload.php` | local Composer-like container/autoload smoke |

## Supported Tools

The script detects these tools and prints skip messages when they are missing:

- `cargo flamegraph` or `cargo-flamegraph`
- Linux `perf`
- macOS `xctrace` Instruments Time Profiler
- macOS `dtrace` availability for manual privileged probes

The macOS and Linux profilers can require local entitlements, kernel settings,
or elevated permissions. Those failures are local setup issues, not Performance
gate failures.
