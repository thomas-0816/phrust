use super::*;

fn is_phar(class_name: &str) -> bool {
    normalize_class_name(class_name) == "phar"
}

/// Handles the native PHAR capability query used by upstream skip probes.
///
/// The runtime archive reader supports the four digest signatures below but
/// intentionally has no OpenSSL signing backend. Reporting only those exact
/// capabilities keeps PHPT skip decisions honest.
pub(in crate::vm::jit_abi) fn execute_native_phar_instruction(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallStaticMethod {
        class_name, method, ..
    } = &instruction.kind
    else {
        return None;
    };
    if !is_phar(class_name) || !method.eq_ignore_ascii_case("getSupportedSignatures") {
        return None;
    }
    Some((|| {
        expect_arity("Phar::getSupportedSignatures", arguments.len(), 0, 0)?;
        let signatures = ["MD5", "SHA-1", "SHA-256", "SHA-512"]
            .into_iter()
            .map(|signature| Value::String(PhpString::from_bytes(signature.as_bytes().to_vec())))
            .collect();
        context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(
            signatures,
        )))
    })())
}
