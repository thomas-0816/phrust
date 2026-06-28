# Performance Region Profiling

FPE-28 adds opt-in, metadata-only region profiling for framework-like traces.
It does not compile or execute regions. The profiler writes JSON to a caller
selected file through `php-vm run --region-profile-json <path>` or the
`PHRUST_REGION_PROFILE_JSON` environment variable.

The region profile never writes to PHP stdout. Enabling it only forces VM
counter collection so the report can be built after execution.

## Privacy Contract

Region profiles are advisory compiler metadata. They must not contain:

- userland values;
- secrets or request payload data;
- raw source paths;
- source text;
- function, class, method, or property names.

Source identity and callsites are emitted as deterministic FNV-1a hashes.
Function and method IDs are numeric IR/runtime IDs. Array/object shapes use
stable VM shape categories or numeric class IDs, not userland names.

## Recorded Regions

The JSON report records bounded traces for these framework-like region kinds:

- `router_dispatch`
- `middleware_service_chain`
- `container_lookup`
- `template_render`
- `json_response`
- `dto_orm_hydration`
- `array_config_traversal`

Each trace includes stable callsite IDs, function IDs, method IDs where method
profile metadata observed them, bytecode block/instruction ranges, IC states,
branch-bias metadata, array/object shape metadata, reference/COW poison events,
include/autoload events, and control-flow rejection reasons.

Current branch bias is metadata-only and reports observed conditional branch
counts plus guard failures. Taken/not-taken bias remains a future feedback
source.

## Candidate Classes

The report classifies each region as one of:

- `inline-cache-only`
- `superinstruction-candidate`
- `baseline-native-candidate`
- `Cranelift-packed-numeric-candidate`
- `unsupported`

Classification is intentionally conservative. A region can be a future
candidate only when the existing VM counters show the relevant metadata family:
IC stability, output/concat fast paths, builtin/output metadata, object-shape
IC data, or array fast-path metadata without reference/COW poison.

Unsupported regions include a stable reason such as missing callsite feedback,
missing array-shape metadata, or missing template/output metadata.

## Framework Smoke Integration

`nix develop -c just framework-smoke` now runs each framework fixture with both
`--counters-json` and `--region-profile-json`. The generated profile files live
under `target/performance/framework-smoke/` and the generated summary records
their paths and classification summaries.

The smoke still compares opt-off and opt-on stdout, stderr, and exit status
before accepting a run.

## Remaining Compiler Prerequisites

Region profiles do not satisfy executable-region prerequisites. Future region
compilation still needs exact live-state maps, deopt snapshots, source-map
ownership, exception/finally/destructor materialization, generator/fiber
snapshots, trace invalidation, and PHPT/reference proof before any compiled
region can replace VM execution.
