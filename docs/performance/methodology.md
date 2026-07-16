# Native performance methodology

Correctness precedes timing. Compare `baseline` and `default` with identical
source, environment, inputs, and PHP-visible observations. Both presets execute
Cranelift machine code; the comparison isolates optimization policy rather than
different execution engines.

Use the profiling Cargo profile for CPU samples:

```bash
nix develop -c cargo build --profile profiling -p php_vm_cli --bin php-vm
samply record target/profiling/php-vm run --engine-preset=default app.php
```

Collect diagnostic counters separately from clean timing:

```bash
php-vm run --engine-preset=default \
  --timings-json target/timings.json \
  --counters-json target/counters.json app.php
```

Counters must use the families in `counter-families.md`. Native cache A/Bs use
`--native-cache=off` and `--native-cache=read-write` with an explicit cache
directory. Warm-cache claims require a fresh process and startup identity that
reports loaded artifacts.

Canonical gates:

```bash
nix develop -c just default-profile-smoke
nix develop -c just native-smoke
nix develop -c just cranelift-native-cache
nix develop -c just verify-performance
```

Real-application measurements use the WordPress root benchmark. Record source,
compiler identity, target/CPU feature set, preset, cache policy, sample count,
concurrency, latency percentiles, throughput, and correctness observables.
