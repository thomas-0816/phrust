# Specialized Call Frames

This note defines the current interpreter policy for call-frame layout
classification and the narrow direct-argument fast path. The generic PHP call
path remains the source of truth for argument binding, diagnostics, references,
visibility, generators, fibers, include/eval execution, and debug/call-context
introspection.

## Layout Classes

The VM records one layout class for each userland function activation:

| Layout | Shape |
| --- | --- |
| `tiny_leaf_frame` | Plain known function with exact positional arguments, no captures, no by-reference params or return, no variadics, no try/finally, no include/eval, no nested call, no generator/fiber state, and no destructor-sensitive allocation body. |
| `known_function_frame` | Known user function that is not eligible for the tiny leaf layout. |
| `known_method_frame` | Method activation or activation with class context such as `$this`, scope class, called class, or declaring class. |
| `closure_frame` | Closure activation or activation with closure captures. |
| `variadic_named_argument_frame` | Named-argument or variadic callee shape. |
| `generator_frame` | Generator activation or generator continuation. |
| `fiber_frame` | Fiber activation or fiber continuation. |
| `include_eval_frame` | Top-level include/require/eval activation sharing caller locals. |
| `dynamic_reflection_call_frame` | Reserved for callable/reflection-style paths that require special warning attribution and cannot use direct tiny-frame assumptions. |

## Fast Path

The only executable specialization is for `tiny_leaf_frame`. For this shape the
VM still runs the normal argument preparation logic first. After that succeeds,
the VM may avoid cloning the active argument snapshot stored on the frame. This
is safe because the eligibility predicate rejects bodies that can observe the
call argument array through `func_get_args`, `func_num_args`, nested calls,
include/eval, exceptions, generators, fibers, closures, class context,
by-reference slots, or destructor-sensitive values.

Frame/register reuse is still controlled by the existing request-local frame
pool. When a tiny frame reuses a pooled frame, the VM records that the heap frame
allocation was avoided. Complex shapes always push the existing generic frame
layout and keep their full argument snapshots.

## Counters

The VM exposes these counters in `--counters-json` and performance reports:

| Counter | Meaning |
| --- | --- |
| `call_frame_layout_observed` | Per-layout activation counts. |
| `tiny_frame_candidates` | Activations classified as tiny leaf candidates. |
| `specialized_frame_hits` | Tiny leaf activations that used the specialized frame path. |
| `generic_frame_fallback_by_reason` | Reasons a classified activation used the generic frame path. |
| `arg_array_avoided` | Tiny leaf activations that avoided cloning the active argument array. |
| `heap_frame_avoided` | Tiny leaf activations served by request-local frame reuse. |

## Generic Fallbacks

Fallbacks are fail-closed and attributed before the generic path runs:

| Reason | Examples |
| --- | --- |
| `not_tiny_leaf` | Known functions with nested calls, call-context introspection, control flow, or other non-leaf behavior. |
| `class_context` | Methods and any activation that carries `$this` or class scope metadata. |
| `closure` | Closures and captured variables. |
| `named_or_variadic` | Named arguments and variadic callees. |
| `by_ref_param`, `by_ref_return`, `by_ref_argument` | Reference-sensitive call shapes. |
| `generator`, `fiber` | Suspended or suspendable activations. |
| `include_eval` | Include/require/eval top-level activations that share caller locals. |
| `dynamic_reflection` | Callable/reflection-style paths requiring special warning or metadata handling. |

Method and function inline caches consume this metadata through the shared
counter stream and VM call path. They do not duplicate argument binding or call
semantics.
