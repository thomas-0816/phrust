//! Exact scalar queries over the authoritative native value plane.

use super::*;

impl NativeRequestFastState {
    /// Implements PHP `is_int` after following direct references by value.
    pub(crate) fn native_value_is_int(&self, encoded: i64) -> bool {
        let Some(encoded) = self.native_by_value_encoding(encoded) else {
            return false;
        };
        if let Some((_, slot)) = self.direct_slot(encoded) {
            return slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                && slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION;
        }
        php_jit::jit_decode_runtime_value(encoded).is_none()
            && php_jit::jit_decode_constant(encoded).is_none()
    }
}
