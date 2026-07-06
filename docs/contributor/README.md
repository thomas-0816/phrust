# Contributor Documentation

These docs are for people changing Phrust itself.

- [Contributor guide](../contributing.md): repository workflow, validation
  gates, reference PHP setup, and artifact policy.
- [Validate a change](../how-to/validate-a-change.md): choose the focused and
  aggregate gate for a change.
- [Work with PHPT](../how-to/work-with-phpt.md): run module batches, debug
  failures, and keep php-src tests read-only.
- [Oracle workflow](../oracle/README.md): turn php-src and reference PHP
  behavior into a prioritized gap queue.
- [PHPT reference](../phpt/README.md): runner, manifests, generated tests, and
  full-regression workflow.
- [WordPress smoke workflow](wordpress-smoke.md): optional real-application
  smoke setup and profiling.

Generated run artifacts, profiler captures, local benchmark reports, and raw
counter output belong under `target/`. Keep committed docs concise and
reproducible.
