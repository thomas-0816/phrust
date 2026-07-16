# Native engine presets

Phrust has one execution engine. Every PHP function is lowered from the shared
frontend and Region IR into Cranelift machine code before execution. There is no
runtime backend selector and no interpreted product mode.

The two presets tune the same compiler pipeline:

| Preset | Contract |
| --- | --- |
| `baseline` | Exhaustive native lowering, minimal optimization, and no speculative specialization. Use it for correctness diagnosis. |
| `default` | Optimizing native lowering, adaptive specialization, compiled calls, OSR, and the persistent native cache. |

CLI and server default to `default`. Select `baseline` only with
`--engine-preset=baseline`; both presets execute native code. Native cache
policy is independently controlled with `--native-cache` and
`--native-cache-dir`. Server deployments may set the equivalent
`PHRUST_NATIVE_CACHE` and `PHRUST_NATIVE_CACHE_DIR` environment variables or
the `native_cache` and `native_cache_dir` configuration keys.
