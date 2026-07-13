//! Native PHP execution coordinator.

mod jit_abi;
mod options;
mod result;

pub use options::{JitBlacklistMode, NativeOptimizationPolicy, VmOptions};
pub use result::VmResult;

use crate::compiled_unit::CompiledUnit;
use jit_abi::{
    jit_array_fetch_int_slow_abi, jit_array_len_abi, jit_concat_string_string_fast,
    jit_count_known_abi, jit_native_call_dispatch_abi, jit_native_dynamic_code_abi,
    jit_property_load_monomorphic_fast, jit_record_array_lookup_abi, jit_runtime_helper_table,
    jit_strlen_known_abi,
};
use php_runtime::api::{OutputBuffer, Value};

/// Process-owned state shared by native request coordinators.
#[derive(Clone, Debug, Default)]
pub struct VmWorkerState;

impl VmWorkerState {
    #[must_use]
    pub fn new(_tiering: crate::tiering::TieringOptions) -> Self {
        Self
    }
}

/// Coordinates mandatory native compilation and outer result assembly.
pub struct Vm {
    options: VmOptions,
    _worker_state: VmWorkerState,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(VmOptions::default())
    }

    #[must_use]
    pub fn with_options(options: VmOptions) -> Self {
        let worker_state = VmWorkerState::new(options.tiering.clone());
        Self::with_options_and_worker_state(options, worker_state)
    }

    #[must_use]
    pub fn with_options_and_worker_state(options: VmOptions, worker_state: VmWorkerState) -> Self {
        Self {
            options,
            _worker_state: worker_state,
        }
    }

    /// Compile and publish native entries without entering application code.
    #[must_use]
    pub fn prewarm_cranelift(&self, unit: &CompiledUnit) -> u64 {
        let entry = unit.unit().entry;
        let Some(function) = unit.unit().functions.get(entry.index()) else {
            return 0;
        };
        let mut compiler = php_jit::JitEngine::new();
        compiler
            .compile_unit_with_runtime_helpers(
                unit.unit(),
                php_jit::JitCompileRequest::new(format!("unit.{}", unit.unit().id.raw()))
                    .with_function_name(function.name.clone())
                    .with_opt_level(if self.options.native_optimization.is_optimizing() {
                        2
                    } else {
                        0
                    }),
                runtime_helper_addresses(),
            )
            .map_or(0, |records| {
                records
                    .iter()
                    .filter(|record| {
                        matches!(record.result.status, php_jit::JitCompileStatus::Compiled)
                    })
                    .count() as u64
            })
    }

    /// Compile every function from authoritative IR and enter the published
    /// Cranelift entry. There is no alternate execution engine.
    #[must_use]
    pub fn execute(&self, unit: impl Into<CompiledUnit>) -> VmResult {
        let unit = unit.into();
        let output = OutputBuffer::default();
        let entry = unit.unit().entry;
        let Some(function) = unit.unit().functions.get(entry.index()) else {
            return VmResult::compile_error(output, "entry function is missing");
        };
        if self.options.verify_ir && unit.prepared_ir_verification_errors() > 0 {
            return VmResult::compile_error(
                output,
                format!(
                    "IR verifier failed with {} error(s)",
                    unit.prepared_ir_verification_errors()
                ),
            );
        }

        let mut compiler = php_jit::JitEngine::new();
        let records = match compiler.compile_unit_with_runtime_helpers(
            unit.unit(),
            php_jit::JitCompileRequest::new(format!("unit.{}", unit.unit().id.raw()))
                .with_function_name(function.name.clone())
                .with_opt_level(if self.options.native_optimization.is_optimizing() {
                    2
                } else {
                    0
                }),
            runtime_helper_addresses(),
        ) {
            Ok(compiled) => compiled,
            Err(error) => {
                return VmResult::compile_error(output, format!("E_NATIVE_COMPILE_SETUP: {error}"));
            }
        };
        let Some(entry_record) = records.iter().find(|record| record.function == entry) else {
            return VmResult::compile_error(output, "E_NATIVE_COMPILE_SETUP: entry record missing");
        };
        if let Some(rejected) = records
            .iter()
            .find(|record| !matches!(&record.result.status, php_jit::JitCompileStatus::Compiled))
        {
            let name = unit
                .unit()
                .functions
                .get(rejected.function.index())
                .map_or("<missing>", |function| function.name.as_str());
            let reason = match &rejected.result.status {
                php_jit::JitCompileStatus::Rejected { reason } => reason.as_str(),
                php_jit::JitCompileStatus::Compiled => "compiler reported no native code",
            };
            let detail = rejected
                .result
                .diagnostics
                .first()
                .map_or("", String::as_str);
            return VmResult::compile_error(
                output,
                format!("E_NATIVE_UNSUPPORTED_LOWERING: function={name}: {reason}: {detail}"),
            );
        }
        let compiled = &entry_record.result;
        let Some(handle) = compiled.handle.as_ref() else {
            let reason = match &compiled.status {
                php_jit::JitCompileStatus::Rejected { reason } => reason.clone(),
                php_jit::JitCompileStatus::Compiled => {
                    "compiler reported success without a native entry".to_owned()
                }
            };
            return VmResult::compile_error(output, format!("E_NATIVE_COMPILE: {reason}"));
        };
        match handle.invoke_i64(&[], php_jit::JIT_RUNTIME_ABI_HASH) {
            Ok(value) => VmResult::success(output, Some(Value::Int(value))),
            Err(error) => VmResult::compile_error(
                output,
                format!("E_NATIVE_ENTRY: native entry invocation failed: {error:?}"),
            ),
        }
    }
}

fn runtime_helper_addresses() -> php_jit::JitRuntimeHelperAddresses {
    php_jit::JitRuntimeHelperAddresses {
        helper_table: jit_runtime_helper_table() as *const _ as usize,
        packed_array_len: jit_array_len_abi as *const () as usize,
        packed_array_fetch_int_slow: jit_array_fetch_int_slow_abi as *const () as usize,
        known_strlen: jit_strlen_known_abi as *const () as usize,
        known_count: jit_count_known_abi as *const () as usize,
        string_concat: jit_concat_string_string_fast as *const () as usize,
        property_load: jit_property_load_monomorphic_fast as *const () as usize,
        record_array_lookup: jit_record_array_lookup_abi as *const () as usize,
        native_call_dispatch: jit_native_call_dispatch_abi as *const () as usize,
        native_dynamic_code: jit_native_dynamic_code_abi as *const () as usize,
    }
}
