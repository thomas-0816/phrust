# VM Slot Binding Metadata

VM slot binding metadata describes where future optimized tiers would prefer to
spill or materialize values when resuming in the interpreter. It is abstract
metadata only: it does not expose raw frame pointers, does not alter
`Frame.locals` or `Frame.registers`, and does not enable native execution.

The current VM frame model stores locals in `LocalFile` as fixed `LocalId`
slots and registers in `RegisterFile` as fixed `RegId` slots. The region IR
binding layer mirrors that shape with abstract `VmSlotId` descriptors so future
OSR and side-exit metadata can talk about VM-visible locations without coupling
to runtime storage internals.

## Slot Kinds

| Kind | Meaning |
| --- | --- |
| `local` | PHP compiled local slot |
| `register` | VM temporary register |
| `temporary` | Region-local temporary spill target |
| `return_value` | Return-value follow-up slot |
| `call_arg` | Normalized call argument slot |
| `foreach_iterator` | Foreach iterator state |
| `foreach_key` | Foreach key state |
| `foreach_value` | Foreach value state |
| `exception_state` | Pending exception or unwind state |
| `output_buffer_state` | Output buffering state |

## Safety Flags

Bindings are rejected when a slot carries PHP semantic state that cannot be
hidden by a future optimized tier:

| Flag | Reason |
| --- | --- |
| `by_ref_alias` | PHP reference identity must be preserved |
| `escaped_reference` | Reference cell may be observed elsewhere |
| `shared_cow` | COW identity or separation timing may be visible |
| `destructor_sensitive` | Lifetime changes may reorder destructors |
| `generator_or_fiber_state` | Suspension state needs explicit support |
| `try_finally_state` | Finally/unwind state must stay interpreter-owned |
| `uninitialized_typed_property` | Typed-property diagnostics are observable |
| `unknown_dynamic_state` | Dynamic PHP state is not precisely modeled |

## OSR And Deopt Relationship

The `BindMap` attached to region IR records:

- abstract VM slot descriptors;
- node/value to preferred slot bindings;
- value class and initialization metadata;
- alias, reference, COW, destructor, generator/fiber, and dynamic-state flags;
- node-index live ranges used to reject invalid slot reuse.

This is the foundation future OSR entry maps and side-exit resume tables need:
guards can capture snapshots in terms of abstract VM slots, and a future native
tier can plan spills without taking ownership of PHP-visible frame semantics.

The validator rejects invalid slots, invalid nodes, type-incompatible bindings,
non-bindable PHP semantic flags, and overlapping live ranges unless a later
alias-aware implementation explicitly marks the overlap as supported.
