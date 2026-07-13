//! Private imports shared by VM submodules.
//!
//! This is intentionally scoped to `php_vm::vm` implementation modules. Public
//! callers should use the stable `php_vm::api` facade.

pub(super) use super::*;
pub(super) use php_runtime::api::{
    PHP_E_WARNING, PhpDiagnosticChannel, PhpReferenceClassification,
};
