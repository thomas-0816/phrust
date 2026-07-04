# Performance Mode Matrix

Performance modes are product settings, not compatibility claims. All modes must
preserve PHP-visible output, diagnostics, exit status, and side-effect order.

| Mode | CLI/profile spelling | Optimizer | Dense bytecode | Quickening / IC | JIT setting | Native execution claim |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline oracle | `baseline` | `O0` | off | off | off | none |
| Managed fast default | `default`, `fast` | `O2` for entry unit, `O0` for runtime includes | auto fallback | on | Cranelift tier requested when available | constrained experiment only, never a production JIT claim |
| Explicit JIT experiment | `experimental-jit` | `O2` | auto fallback | on | Cranelift tier requested when available | constrained experiment only, guarded by feature support and runtime eligibility |

Default builds must remain honest about unavailable acceleration. If the
Cranelift feature or runtime preconditions are absent, `JitMode::Cranelift`
selects reporting and eligibility plumbing but execution stays on managed VM
paths. Any future production native-code tier needs separate safety ownership,
W^X/executable-memory policy, and dedicated smoke gates.
