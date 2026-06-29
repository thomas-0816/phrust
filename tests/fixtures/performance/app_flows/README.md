# Application-Flow Performance Fixtures

This corpus contains deterministic PHP application-flow fixtures for comparing
Phrust with the pinned reference PHP CLI. The fixtures are intentionally small
and self-contained, but each one models a real application path rather than an
isolated arithmetic microbenchmark.

Admission policy:

- Every fixture must print exactly one stable `app-flow ...` line.
- Every fixture must run correctly on Phrust and on the reference PHP CLI before
  it is admitted to the matrix.
- Fixtures must not use network, databases, filesystem writes, process
  execution, wall-clock time, or external package managers.
- Larger manual runs are controlled by `PHRUST_APP_FLOW_SCALE`; smoke runs use
  scale `1`.
- Unsupported language or builtin behavior should be avoided in the fixture
  design rather than documented as a known gap for this suite.

The suite fails on output, diagnostic, or exit-status divergence. Wall-clock
timings are advisory host-local trend data only.
