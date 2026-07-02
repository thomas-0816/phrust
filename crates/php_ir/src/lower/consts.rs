use std::collections::HashMap;

use crate::constants::IrConstant;
use php_semantics::hir::HirModule;

use super::declarations::{ClassConstantInitializerMap, ClassParentMap};

pub(super) struct DeferredConstExprLoweringInput<'a> {
    pub(super) module: &'a HirModule,
    pub(super) named_constants: &'a HashMap<String, IrConstant>,
    pub(super) current_class: Option<&'a str>,
    pub(super) class_constants: &'a ClassConstantInitializerMap,
    pub(super) class_parents: &'a ClassParentMap,
    pub(super) visiting_class_constants: &'a mut Vec<(String, String)>,
}
