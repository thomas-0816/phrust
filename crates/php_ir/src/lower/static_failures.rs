//! Statically proven PHP failures that become prepared native throwables.

use super::expressions::{LowerSite, LoweredExpr, call_argument_discard_registers};
use super::*;

pub(super) fn statically_missing_static_property(
    builder: &IrBuilder,
    class_name: &str,
    property: &str,
) -> bool {
    if matches!(
        normalize_class_name(class_name).as_str(),
        "self" | "static" | "parent"
    ) {
        return false;
    }
    let mut class_name = normalize_class_name(class_name);
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(class_name.clone()) {
            return false;
        }
        let Some(class) = builder
            .classes()
            .iter()
            .find(|class| normalize_class_name(&class.name) == class_name)
        else {
            // An unresolved class or parent may be supplied by autoloading.
            return false;
        };
        if class.flags.is_conditional {
            return false;
        }
        if class
            .properties
            .iter()
            .any(|candidate| candidate.flags.is_static && candidate.name == property)
        {
            return false;
        }
        let Some(parent) = &class.parent else {
            return true;
        };
        class_name = normalize_class_name(parent);
    }
}

fn static_builtin_binding_error(
    function: &str,
    args: &[crate::instruction::IrCallArg],
) -> Option<(&'static str, String)> {
    if args.iter().any(|argument| argument.unpack) {
        return None;
    }
    let metadata = php_std::arginfo::function_metadata_indexed(function)?;
    let variadic = metadata
        .params
        .last()
        .is_some_and(|parameter| parameter.variadic);
    let mut bound = vec![false; metadata.params.len()];
    let mut positional = 0_usize;
    for argument in args {
        if let Some(name) = argument.name.as_deref() {
            if let Some(index) = metadata
                .params
                .iter()
                .position(|parameter| parameter.name == name)
            {
                if bound[index] {
                    return Some((
                        "Error",
                        format!("Named parameter ${name} overwrites previous argument"),
                    ));
                }
                bound[index] = true;
            } else if !variadic {
                return Some(("Error", format!("Unknown named parameter ${name}")));
            }
            continue;
        }
        while positional < bound.len() && bound[positional] {
            positional += 1;
        }
        if positional < bound.len() {
            bound[positional] = true;
            positional += 1;
        } else if !variadic {
            let required = metadata
                .params
                .iter()
                .filter(|parameter| !parameter.optional && !parameter.variadic)
                .count();
            let expected = if required == metadata.params.len() {
                format!(
                    "{}() expects exactly {} argument{}, {} given",
                    metadata.name,
                    metadata.params.len(),
                    if metadata.params.len() == 1 { "" } else { "s" },
                    args.len(),
                )
            } else {
                format!(
                    "{}() expects at most {} argument{}, {} given",
                    metadata.name,
                    metadata.params.len(),
                    if metadata.params.len() == 1 { "" } else { "s" },
                    args.len(),
                )
            };
            return Some(("ArgumentCountError", expected));
        }
    }
    let missing_required = metadata
        .params
        .iter()
        .take_while(|parameter| !parameter.variadic)
        .enumerate()
        .any(|(index, parameter)| !parameter.optional && !bound[index]);
    if !missing_required {
        return None;
    }
    let required_total = metadata
        .params
        .iter()
        .filter(|parameter| !parameter.optional && !parameter.variadic)
        .count();
    let expectation = if !variadic && required_total == metadata.params.len() {
        "exactly"
    } else {
        "at least"
    };
    Some((
        "ArgumentCountError",
        format!(
            "{}() expects {expectation} {} argument{}, {} given",
            metadata.name,
            required_total,
            if required_total == 1 { "" } else { "s" },
            args.len(),
        ),
    ))
}

impl LoweringContext<'_> {
    pub(super) fn lower_static_property_fetch_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: super::expressions::StaticPropertyTarget,
    ) -> Option<LoweredExpr> {
        if statically_missing_static_property(builder, &target.class_name, &target.property) {
            let message = builder.intern_constant(IrConstant::String(format!(
                "Access to undeclared static property {}::${}",
                target.class_name.trim_start_matches('\\'),
                target.property,
            )));
            let throwable = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                site.block,
                InstructionKind::MakeException {
                    dst: throwable,
                    class_name: "Error".to_owned(),
                    message: Operand::Constant(message),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                site.block,
                instruction,
                site.expr,
                site.span,
            );
            builder.emit(
                site.function,
                site.block,
                InstructionKind::Throw {
                    value: Operand::Register(throwable),
                },
                site.span,
            );
            let dst = builder.alloc_register(site.function);
            let null = builder.intern_constant(IrConstant::Null);
            builder.emit_load_const(site.function, site.block, dst, null, site.span);
            return Some(LoweredExpr {
                register: dst,
                block: site.block,
            });
        }
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            site.block,
            InstructionKind::FetchStaticProperty {
                dst,
                class_name: target.class_name,
                property: target.property,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: site.block,
        })
    }

    pub(super) fn emit_static_builtin_binding_error(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        function: &str,
        args: &[crate::instruction::IrCallArg],
        dst: RegId,
        block: BlockId,
    ) -> Option<LoweredExpr> {
        let (class_name, message) = static_builtin_binding_error(function, args)?;
        let discard_args = call_argument_discard_registers(args, dst);
        self.emit_register_discards(
            builder,
            site.function,
            block,
            site.expr,
            site.span,
            &discard_args,
        );
        let message = builder.intern_constant(IrConstant::String(message));
        let throwable = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            block,
            InstructionKind::MakeException {
                dst: throwable,
                class_name: class_name.to_owned(),
                message: Operand::Constant(message),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            block,
            instruction,
            site.expr,
            site.span,
        );
        builder.emit(
            site.function,
            block,
            InstructionKind::Throw {
                value: Operand::Register(throwable),
            },
            site.span,
        );
        let null = builder.intern_constant(IrConstant::Null);
        builder.emit_load_const(site.function, block, dst, null, site.span);
        Some(LoweredExpr {
            register: dst,
            block,
        })
    }
}
