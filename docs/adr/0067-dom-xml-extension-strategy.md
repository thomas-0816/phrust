# ADR 0067: DOM/XML Extension Strategy

## Status

Accepted

## Context

The XML-family PHPT corpus includes `dom`, `xml`, `simplexml`, `xsl`, and
`soap`. These extensions are high-risk because they are not just function
collections. They require parser dependencies, XML/HTML object models,
resource/object lifetimes, stream/file integration, Reflection metadata, and
security-sensitive behavior.

The current core-runtime PHPT green path is focused on the language frontend,
runtime values, VM execution, standard-library primitives, streams, JSON, PCRE,
date, SPL, Reflection, performance gates, and PHPT tooling. A partial DOM/XML
surface would create false positives for frameworks and Composer-style platform
checks without satisfying PHP-visible behavior.

## Decision

DOM/XML-family real implementation is not part of the current core-runtime PHPT
green path.

The current strategy is:

- Keep XML-family PHPTs indexed and visible in baseline accounting.
- Keep `extension_loaded()` and representative class/function/constant probes
  negative until a real implementation is selected.
- Add only platform-check PHPTs for the current policy.
- Do not add a parser dependency for this branch.
- Do not fake successful parsing, DOM nodes, SimpleXML objects, XSL transforms,
  or SOAP requests.

## Future Safe MVP Requirements

A future DOM/XML MVP needs all of the following before enabling extension
visibility:

- An approved XML parser dependency with licensing, maintenance, and security
  review.
- A DOM object model that can represent documents, nodes, attributes,
  namespaces, ownership, mutation, liveness, and serialization.
- Error reporting that can preserve parser diagnostics separately from runtime
  diagnostics.
- Stream and file integration for `load()`, `loadXML()`, file-based parsing,
  and deterministic local filesystem policy.
- A resource/object ownership model for XML parser handles where needed.
- Reflection metadata for classes, functions, constants, methods, properties,
  and extension ownership.
- PHPT fixtures that compare reference behavior by output, diagnostics, exit
  status, and source positions where relevant.

## Parser Dependency Policy

Parser dependency selection must be explicit. The project must not copy php-src
C implementation into Rust. The parser layer must expose deterministic errors,
byte-oriented spans where applicable, and stable ownership semantics for runtime
objects.

Any dependency must be added with a focused justification and validated through
the repository dependency gates.

## Object Model Requirements

The DOM and SimpleXML layers need real runtime objects, not ad hoc string
matching. Required capabilities include:

- Class hierarchy and internal class metadata.
- Node identity and document ownership.
- Parent/child/sibling mutation.
- Attribute and namespace behavior.
- Iterator and array-like views where PHP exposes them.
- Error behavior for invalid hierarchy and wrong-document operations.

## Stream and File Integration Requirements

File-backed XML operations must use the existing runtime stream/filesystem
capability model. Network access, external entity loading, schema resolution,
and stylesheet includes require explicit policy and must not be enabled
implicitly.

## Reflection Implications

When an XML-family extension becomes enabled, Reflection must report extension
names, internal classes, methods, properties, constants, function signatures,
and parameter metadata from the same registry used by runtime introspection.

## PHPT Gate Strategy

Initial gates should remain narrow:

1. Platform checks for extension/class/function/constant visibility.
2. Parser creation and deterministic failure fixtures.
3. Minimal parse/load fixture with no external entities.
4. DOM object identity and mutation fixtures.
5. Serialization fixtures.
6. Selected upstream module batches.

The full upstream XML-family corpora remain non-goals until the MVP proves the
object model and parser dependency are stable.
