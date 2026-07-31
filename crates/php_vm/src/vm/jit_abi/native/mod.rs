//! Optimizing/exact native execution namespace.
//!
//! This crate-root module deliberately imports only the authoritative
//! fast-state representation and immutable publication records. Request
//! coordination and the Rust compatibility value plane live below `vm`, whose
//! restricted items are therefore inaccessible from this sibling namespace.

use crate::vm::jit_abi::{
    NativeComparisonObject, NativeComparisonTraversal, NativeComparisonValue,
    NativeDynamicFunction, NativeDynamicUnit, NativeExecutionScope, NativeFrameArena,
    NativeFunctionNameScope, NativeLastError, NativePreparedCallableOwner, NativePreparedClosure,
    PreparedNativeRuntimeClass, PreparedNativeThrowableSite, native_comparison_truthy,
    native_comparison_values_order, native_fixed_callable_plan, native_reference_state,
};
use std::rc::Rc;
use std::sync::Arc;

pub(super) static NATIVE_TEMPNAM_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(super) fn native_direct_string_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(super) fn php_constant_category(extension: &str) -> &str {
    match extension {
        "core" => "Core",
        "pdo" => "PDO",
        "phar" => "Phar",
        "spl" => "SPL",
        extension => extension,
    }
}

pub(super) fn php_core_runtime_constant(name: &str) -> bool {
    matches!(name, "STDIN" | "STDOUT" | "STDERR")
}

#[derive(Clone, Copy)]
pub(crate) struct NativeFixedCallablePlan {
    pub(crate) function: php_ir::FunctionId,
    pub(crate) visible_arity: u32,
    pub(crate) has_receiver: bool,
    pub(crate) first_parameter_by_reference: bool,
    pub(crate) returns_int: bool,
    pub(crate) returns_string: bool,
    pub(crate) returns_releasable_scalar: bool,
}

/// Narrow live capability used by exact symbol-query builtins.
///
/// The pointers address only symbol/class/constant publication fields inside
/// the stable request owner. They continue to observe include/eval updates
/// because those fields are mutated in place, while keeping the complete
/// execution coordinator, value compatibility plane, output, frames, and
/// extension state unreachable from successful exact queries.
#[derive(Default)]
pub(super) struct NativeSymbolQueryCapability {
    pub(super) active_compiled: *const crate::compiled_unit::CompiledUnit,
    pub(super) current_dynamic_unit: *const Option<usize>,
    pub(super) dynamic_units: *const Vec<NativeDynamicUnit>,
    pub(super) dynamic_functions: *const std::collections::BTreeMap<String, php_ir::FunctionId>,
    pub(super) external_functions: *const std::collections::HashMap<String, NativeDynamicFunction>,
    pub(super) external_class_units: *const std::collections::HashMap<String, usize>,
    pub(super) deployment_functions:
        *const std::sync::Arc<std::collections::HashMap<std::sync::Arc<str>, php_ir::FunctionId>>,
    pub(super) deployment_classes:
        *const std::sync::Arc<std::collections::HashSet<std::sync::Arc<str>>>,
    pub(super) visible_function_names: *const Rc<NativeFunctionNameScope>,
    pub(super) native_dynamic_constants: *mut std::collections::BTreeMap<String, i64>,
    pub(super) trusted_dynamic_constant_sites:
        *const std::collections::BTreeMap<String, Vec<usize>>,
    pub(super) dynamic_classes: *const std::collections::BTreeSet<String>,
    pub(super) class_aliases: *const std::collections::BTreeMap<String, String>,
}

/// Narrow live capability for exact request configuration builtins.
///
/// All pointers address stable fields in the separately boxed request owner.
/// Mutations therefore remain visible to baseline/include code without
/// exposing the cold execution coordinator or materializing Rust `Value`s.
#[derive(Default)]
pub(super) struct NativeConfigurationCapability {
    pub(super) ini_registry: *mut php_runtime::api::IniRegistry,
    pub(super) include_path: *mut Arc<Vec<std::path::PathBuf>>,
    pub(super) display_errors: *mut bool,
    pub(super) default_timezone: *mut String,
}

/// Narrow live capability for exact HTTP response builtins.
///
/// The response owner is separately boxed and remains stable for the request.
/// Exact handlers mutate only this published response state and cannot recover
/// the complete cold execution coordinator.
#[derive(Default)]
pub(super) struct NativeHttpResponseCapability {
    pub(super) response: *mut php_runtime::api::RuntimeHttpResponseState,
}

/// Narrow live capability for exact request/environment query builtins.
///
/// These pointers address only the three request-owned collections/strings
/// required by the family. Mutating baseline operations update those owners
/// in place, so later exact reads remain current without recovering the cold
/// execution coordinator or materializing Rust `Value`s.
#[derive(Default)]
pub(super) struct NativeRequestQueryCapability {
    pub(super) environment: *const std::sync::Arc<Vec<(String, String)>>,
    pub(super) included_files: *const std::collections::BTreeSet<std::path::PathBuf>,
    pub(super) sapi_name: *const String,
}

/// Narrow live capability for the cooperative execution deadline.
///
/// Optimizing loop headers must preserve `max_execution_time` semantics, but
/// polling must not recover the complete cold execution coordinator. Both
/// pointers address stable fields in the separately boxed request owner.
#[derive(Default)]
pub(super) struct NativeExecutionDeadlineCapability {
    pub(super) deadline: *const Option<std::time::Instant>,
    pub(super) diagnostic: *mut Option<php_runtime::api::RuntimeDiagnostic>,
}

/// Narrow request-stable capability for generated native call-frame storage.
///
/// The arena remains owned by the request buffers, but optimizing frame
/// allocation reaches only this native allocator and its diagnostic sink. It
/// never recovers the cold request coordinator from the fast-state pointer.
#[derive(Default)]
pub(super) struct NativeFrameArenaCapability {
    pub(super) arena: *mut NativeFrameArena,
    pub(super) diagnostic: *mut Option<php_runtime::api::RuntimeDiagnostic>,
}

/// Narrow request-stable mbstring capability.
///
/// Exact mbstring handlers can observe and update only the two PHP-visible
/// request settings. They cannot recover the registered-extension state or
/// the Rust `Value` execution plane that owns it.
#[repr(C)]
#[derive(Default)]
pub(super) struct NativeMbstringCapability {
    pub(super) internal_encoding: *mut String,
    pub(super) substitute_character: *mut php_runtime::api::MbSubstituteCharacter,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct NativeBcmathCapability {
    pub(super) scale: *mut usize,
}

/// Explicit request-published access to the platform CSPRNG.
///
/// Exact handlers cannot call the ambient random source without this
/// capability; tests and nested owners can publish a different fill function.
#[repr(C)]
#[derive(Default)]
pub(super) struct NativeRandomCapability {
    pub(super) fill: Option<fn(&mut [u8]) -> bool>,
}

/// Five immutable request-input roots published at owner construction.
///
/// The bitset distinguishes an absent source from a present empty array.
#[repr(C)]
#[derive(Default)]
pub(super) struct NativeFilterCapability {
    pub(super) roots: [i64; 5],
    pub(super) present: u8,
}

/// Value-free session control capability.
///
/// `SessionState` physically keeps `PhpArray` payloads in a sibling private
/// record. Exact handlers receive only this pointer and cannot recover the
/// baseline session graph.
#[repr(C)]
#[derive(Default)]
pub(super) struct NativeSessionCapability {
    pub(super) control: *mut php_runtime::api::NativeSessionControlState,
    /// Canonical request-global reference for `$_SESSION`.
    pub(super) global_reference: i64,
    /// Independently owned native COW snapshot used by commit/abort/reset.
    pub(super) committed: i64,
    /// Transport callbacks are cold capabilities and force the one baseline
    /// continuation only when the requested operation actually needs them.
    pub(super) has_loader: u8,
    pub(super) has_id_generator: u8,
}

/// Authoritative request-local stream-context option owners.
///
/// Every handle is an independently owned direct native array. The runtime
/// resource keeps no parallel `PhpArray`; a baseline continuation
/// materializes and immediately republishes one compatibility snapshot.
#[derive(Default)]
pub(super) struct NativeStreamContextState {
    pub(super) default_options: i64,
    pub(super) resource_options: std::collections::BTreeMap<u64, i64>,
}

#[derive(Clone)]
pub(crate) struct NativeRegisteredAutoloadCallback {
    pub(crate) callable: i64,
    pub(crate) transient_export: bool,
}

#[derive(Clone)]
pub(crate) enum NativeRegisteredCallbackSource {
    Cold(php_ir::Instruction),
    NativeContinuation { function: u32, continuation: u32 },
}

#[derive(Clone)]
pub(crate) struct NativeRegisteredShutdownCallback {
    pub(crate) callable: i64,
    pub(crate) arguments: Vec<i64>,
    pub(crate) source: NativeRegisteredCallbackSource,
    pub(crate) transient_export: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct NativeRegisteredErrorHandler {
    pub(crate) callback: i64,
    pub(crate) levels: i64,
}

/// Authoritative request-owned callback roots. Every handle owns one direct
/// native value; compatibility callback carriers remain cold-only.
#[derive(Default)]
pub(crate) struct NativeRegisteredCallbackState {
    pub(crate) autoload_callbacks: Vec<NativeRegisteredAutoloadCallback>,
    pub(crate) shutdown_callbacks: Vec<NativeRegisteredShutdownCallback>,
    pub(crate) error_handlers: Vec<NativeRegisteredErrorHandler>,
    pub(crate) exception_handlers: Vec<i64>,
}

/// Compact stable prefix passed through every generated entry and compiled
/// call. Exact native operations can reach only explicitly published
/// request-owned capabilities from this type.
#[repr(C)]
#[derive(Default)]
pub(crate) struct NativeRequestFastState {
    pub(super) header: php_jit::JitNativeFastStateHeader,
    pub(super) output: *mut php_runtime::api::OutputBuffer,
    pub(super) json_state: *mut php_runtime::api::JsonRequestState,
    pub(super) pcre_state: *mut php_runtime::api::PcreRequestState,
    pub(super) gc_state: *mut php_runtime::api::GcRequestState,
    pub(super) cwd: *mut std::path::PathBuf,
    pub(super) filesystem_capabilities: *const php_runtime::api::FilesystemCapabilities,
    pub(super) filesystem_state: *mut php_runtime::api::FilesystemRuntimeState,
    pub(super) stdin: *const std::sync::Arc<[u8]>,
    pub(super) resources: *mut php_runtime::api::ResourceTable,
    pub(super) upload_registry: *mut php_runtime::api::UploadRegistry,
    pub(super) last_error: *mut Option<NativeLastError>,
    pub(super) direct_resource_handles: *mut std::collections::HashMap<u64, u32>,
    pub(super) direct_closure_handles: *mut std::collections::HashMap<u64, u32>,
    pub(super) execution_scope: *const NativeExecutionScope,
    pub(super) symbol_query: NativeSymbolQueryCapability,
    pub(super) configuration: NativeConfigurationCapability,
    pub(super) http_response: NativeHttpResponseCapability,
    pub(super) request_query: NativeRequestQueryCapability,
    pub(super) mbstring: NativeMbstringCapability,
    pub(super) bcmath: NativeBcmathCapability,
    pub(super) random: NativeRandomCapability,
    pub(super) filter: NativeFilterCapability,
    pub(super) session: NativeSessionCapability,
    pub(super) stream_context: *mut NativeStreamContextState,
    pub(super) callback_handlers: *mut NativeRegisteredCallbackState,
    pub(super) callback_transient_export: u8,
    /// Request-stable immutable absence cell returned only by non-mutating
    /// dynamic-property tests on classes proven not to implement `__isset`.
    pub(super) absent_dynamic_property_slot: php_runtime::api::NativeDeclaredPropertySlot,
    pub(super) execution_deadline: NativeExecutionDeadlineCapability,
    pub(super) frame_arena: NativeFrameArenaCapability,
}

/// Transactional writer for an unpublished range in the authoritative native
/// array arena. Every pushed entry transfers one key owner and one value
/// owner; publication commits the written prefix, while failure releases it
/// in reverse order.
struct NativeOwnedDirectArrayWriter {
    entries: *mut php_jit::JitNativeDirectArrayEntry,
    start: usize,
    capacity: u32,
    length: usize,
    maximum_length: usize,
}

impl NativeOwnedDirectArrayWriter {
    fn len(&self) -> usize {
        self.length
    }

    fn get(&self, index: usize) -> Option<php_jit::JitNativeDirectArrayEntry> {
        if index >= self.length {
            return None;
        }
        // SAFETY: the writer owns a reserved stable arena range and `index`
        // was checked against its initialized prefix.
        #[allow(unsafe_code)]
        Some(unsafe { *self.entries.add(index) })
    }

    fn push_owned(
        &mut self,
        entry: php_jit::JitNativeDirectArrayEntry,
    ) -> Result<(), &'static str> {
        if self.length >= self.maximum_length {
            return Err("native direct array writer exceeded its reserved range");
        }
        if self.length >= self.capacity as usize {
            return Err("native direct array writer requires growth");
        }
        // SAFETY: the reserved range has room for this next initialized entry.
        #[allow(unsafe_code)]
        unsafe {
            *self.entries.add(self.length) = entry;
        }
        self.length += 1;
        Ok(())
    }

    fn replace_owned(
        &mut self,
        index: usize,
        entry: php_jit::JitNativeDirectArrayEntry,
    ) -> Option<php_jit::JitNativeDirectArrayEntry> {
        if index >= self.length {
            return None;
        }
        // SAFETY: `index` addresses one initialized entry in the reserved
        // unpublished range.
        #[allow(unsafe_code)]
        Some(unsafe { std::mem::replace(&mut *self.entries.add(index), entry) })
    }
}

/// Bounded direct publisher for the scalar/array subset of PHP's serialized
/// wire format. Object construction, reference records, malformed input
/// warnings, and option-dependent semantics retain one baseline continuation.
pub(crate) struct NativeSerializedParser<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) offset: usize,
    pub(crate) parsed_items: usize,
}

impl NativeSerializedParser<'_> {
    const MAX_DEPTH: usize = 64;
    const MAX_ITEMS: usize = 16_384;
    const MAX_BYTES: usize = 1_048_576;

    pub(crate) fn parse(mut self, publisher: &mut NativeRequestFastState) -> Option<i64> {
        if self.bytes.len() > Self::MAX_BYTES {
            return None;
        }
        let value = self.parse_value(publisher, 0)?;
        if self.offset != self.bytes.len() {
            let _ = publisher.discard_owned_direct_value(value);
            return None;
        }
        Some(value)
    }

    fn parse_prefix(mut self, publisher: &mut NativeRequestFastState) -> Option<(i64, usize)> {
        if self.bytes.len() > Self::MAX_BYTES {
            return None;
        }
        let value = self.parse_value(publisher, 0)?;
        Some((value, self.offset))
    }

    fn parse_value(&mut self, publisher: &mut NativeRequestFastState, depth: usize) -> Option<i64> {
        if depth > Self::MAX_DEPTH {
            return None;
        }
        match self.take_byte()? {
            b'N' => {
                self.expect(b';')?;
                Some(php_jit::jit_encode_constant(u32::MAX))
            }
            b'b' => {
                self.expect(b':')?;
                let value = match self.take_byte()? {
                    b'0' => false,
                    b'1' => true,
                    _ => return None,
                };
                self.expect(b';')?;
                Some(php_jit::jit_encode_constant(if value {
                    php_jit::JIT_VALUE_TRUE
                } else {
                    php_jit::JIT_VALUE_FALSE
                }))
            }
            b'i' => {
                self.expect(b':')?;
                let value = self.take_ascii_until(b';')?.parse().ok()?;
                publisher.publish_direct_int(value).ok()
            }
            b'd' => {
                self.expect(b':')?;
                let value = match self.take_ascii_until(b';')? {
                    "NAN" => f64::NAN,
                    "INF" => f64::INFINITY,
                    "-INF" => f64::NEG_INFINITY,
                    value => value.parse().ok()?,
                };
                publisher.publish_direct_float(value).ok()
            }
            b's' => {
                let (start, length) = self.parse_string_range()?;
                publisher
                    .publish_direct_string_bytes(self.bytes.get(start..start.checked_add(length)?)?)
                    .ok()
            }
            b'a' => self.parse_array(publisher, depth),
            // Native object publication and reference graphs are separate
            // semantic families, so their wire tags take the baseline once.
            b'O' | b'R' | b'r' => None,
            _ => None,
        }
    }

    fn parse_array(&mut self, publisher: &mut NativeRequestFastState, depth: usize) -> Option<i64> {
        self.expect(b':')?;
        let length = self.take_ascii_until(b':')?.parse::<usize>().ok()?;
        self.parsed_items = self.parsed_items.checked_add(length)?;
        if self.parsed_items > Self::MAX_ITEMS {
            return None;
        }
        self.expect(b'{')?;
        publisher
            .publish_owned_direct_array_dynamic(length, |publisher, writer| {
                for _ in 0..length {
                    let key = self
                        .parse_key(publisher, depth + 1)
                        .ok_or("native serialized array key is malformed")?;
                    let Some(value) = self.parse_value(publisher, depth + 1) else {
                        let _ = publisher.discard_owned_direct_value(key);
                        return Err("native serialized array value is malformed");
                    };
                    let existing = (0..writer.len()).find(|&index| {
                        writer.get(index).is_some_and(|entry| {
                            publisher.native_compare_array_keys(entry.key, key)
                                == Some(std::cmp::Ordering::Equal)
                        })
                    });
                    let entry = php_jit::JitNativeDirectArrayEntry { key, value };
                    if let Some(index) = existing {
                        let previous = writer
                            .get(index)
                            .ok_or("native serialized array entry disappeared")?;
                        let _ = publisher.discard_owned_direct_value(key);
                        let Some(replaced) = writer.replace_owned(
                            index,
                            php_jit::JitNativeDirectArrayEntry {
                                key: previous.key,
                                value,
                            },
                        ) else {
                            let _ = publisher.discard_owned_direct_value(value);
                            return Err("native serialized array replacement failed");
                        };
                        let _ = publisher.discard_owned_direct_value(replaced.value);
                    } else if let Err(error) = writer.push_owned(entry) {
                        let _ = publisher.discard_owned_direct_value(value);
                        let _ = publisher.discard_owned_direct_value(key);
                        return Err(error);
                    }
                }
                self.expect(b'}')
                    .ok_or("native serialized array is not terminated")
            })
            .ok()
    }

    fn parse_key(&mut self, publisher: &mut NativeRequestFastState, depth: usize) -> Option<i64> {
        if depth > Self::MAX_DEPTH {
            return None;
        }
        match self.take_byte()? {
            b'i' => {
                self.expect(b':')?;
                let value = self.take_ascii_until(b';')?.parse().ok()?;
                publisher.publish_direct_int(value).ok()
            }
            b's' => {
                let (start, length) = self.parse_string_range()?;
                let bytes = self.bytes.get(start..start.checked_add(length)?)?;
                if let Some(value) = php_runtime::api::array_key_integer_bytes(bytes) {
                    publisher.publish_direct_int(value).ok()
                } else {
                    publisher.publish_direct_string_bytes(bytes).ok()
                }
            }
            _ => None,
        }
    }

    fn parse_string_range(&mut self) -> Option<(usize, usize)> {
        self.expect(b':')?;
        let length = self.take_ascii_until(b':')?.parse::<usize>().ok()?;
        self.expect(b'"')?;
        let start = self.offset;
        let end = self.offset.checked_add(length)?;
        self.bytes.get(self.offset..end)?;
        self.offset = end;
        self.expect(b'"')?;
        self.expect(b';')?;
        Some((start, length))
    }

    fn take_ascii_until(&mut self, delimiter: u8) -> Option<&str> {
        let start = self.offset;
        while self.bytes.get(self.offset).copied()? != delimiter {
            self.offset = self.offset.checked_add(1)?;
        }
        let value = std::str::from_utf8(self.bytes.get(start..self.offset)?).ok()?;
        self.offset += 1;
        Some(value)
    }

    fn take_byte(&mut self) -> Option<u8> {
        let byte = self.bytes.get(self.offset).copied()?;
        self.offset += 1;
        Some(byte)
    }

    fn expect(&mut self, expected: u8) -> Option<()> {
        (self.take_byte()? == expected).then_some(())
    }
}

/// Allocation-free structural cursor over the native serialized subset.
/// Session decoding uses it to determine the exact top-level entry count
/// before reserving the authoritative result array.
struct NativeSerializedCursor<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) offset: usize,
    pub(crate) parsed_items: usize,
}

impl<'a> NativeSerializedCursor<'a> {
    fn skip_prefix(bytes: &'a [u8]) -> Option<usize> {
        if bytes.len() > NativeSerializedParser::MAX_BYTES {
            return None;
        }
        let mut cursor = Self {
            bytes,
            offset: 0,
            parsed_items: 0,
        };
        cursor.skip_value(0)?;
        Some(cursor.offset)
    }

    fn skip_value(&mut self, depth: usize) -> Option<()> {
        if depth > NativeSerializedParser::MAX_DEPTH {
            return None;
        }
        match self.take_byte()? {
            b'N' => self.expect(b';'),
            b'b' => {
                self.expect(b':')?;
                if !matches!(self.take_byte()?, b'0' | b'1') {
                    return None;
                }
                self.expect(b';')
            }
            b'i' => {
                self.expect(b':')?;
                self.take_ascii_until(b';')?.parse::<i64>().ok()?;
                Some(())
            }
            b'd' => {
                self.expect(b':')?;
                match self.take_ascii_until(b';')? {
                    "NAN" | "INF" | "-INF" => {}
                    value => {
                        value.parse::<f64>().ok()?;
                    }
                }
                Some(())
            }
            b's' => self.skip_string(),
            b'a' => self.skip_array(depth),
            b'O' | b'R' | b'r' => None,
            _ => None,
        }
    }

    fn skip_array(&mut self, depth: usize) -> Option<()> {
        self.expect(b':')?;
        let length = self.take_ascii_until(b':')?.parse::<usize>().ok()?;
        self.parsed_items = self.parsed_items.checked_add(length)?;
        if self.parsed_items > NativeSerializedParser::MAX_ITEMS {
            return None;
        }
        self.expect(b'{')?;
        for _ in 0..length {
            self.skip_key(depth + 1)?;
            self.skip_value(depth + 1)?;
        }
        self.expect(b'}')
    }

    fn skip_key(&mut self, depth: usize) -> Option<()> {
        if depth > NativeSerializedParser::MAX_DEPTH {
            return None;
        }
        match self.take_byte()? {
            b'i' => {
                self.expect(b':')?;
                self.take_ascii_until(b';')?.parse::<i64>().ok()?;
                Some(())
            }
            b's' => self.skip_string(),
            _ => None,
        }
    }

    fn skip_string(&mut self) -> Option<()> {
        self.expect(b':')?;
        let length = self.take_ascii_until(b':')?.parse::<usize>().ok()?;
        self.expect(b'"')?;
        self.offset = self.offset.checked_add(length)?;
        self.bytes.get(..self.offset)?;
        self.expect(b'"')?;
        self.expect(b';')
    }

    fn take_ascii_until(&mut self, delimiter: u8) -> Option<&str> {
        let start = self.offset;
        while self.bytes.get(self.offset).copied()? != delimiter {
            self.offset = self.offset.checked_add(1)?;
        }
        let value = std::str::from_utf8(self.bytes.get(start..self.offset)?).ok()?;
        self.offset += 1;
        Some(value)
    }

    fn take_byte(&mut self) -> Option<u8> {
        let byte = self.bytes.get(self.offset).copied()?;
        self.offset += 1;
        Some(byte)
    }

    fn expect(&mut self, expected: u8) -> Option<()> {
        (self.take_byte()? == expected).then_some(())
    }
}

include!("fast_state_impl.rs");

#[path = "exact_call_dispatch.rs"]
pub(super) mod exact_call_dispatch;
#[path = "exact_runtime_ops.rs"]
pub(super) mod exact_runtime_ops;
