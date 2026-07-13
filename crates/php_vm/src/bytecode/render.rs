use super::*;

pub(super) fn render_operands(operands: &DenseOperands) -> String {
    match operands {
        DenseOperands::None => "-".to_string(),
        DenseOperands::RegConst { dst, constant } => format!("r{dst} c{constant}"),
        DenseOperands::RegOperand { dst, src } => format!("r{dst} {}", render_operand(*src)),
        DenseOperands::LocalOperand { local, src } => format!("l{local} {}", render_operand(*src)),
        DenseOperands::Local { local } => format!("l{local}"),
        DenseOperands::StaticLocal {
            local,
            name,
            default,
        } => format!("l{local} n{name} default={}", render_operand(*default)),
        DenseOperands::LocalName { local, name } => format!("l{local} n{name}"),
        DenseOperands::RegName { dst, name } => format!("r{dst} n{name}"),
        DenseOperands::Cast { dst, kind, src } => {
            format!("r{dst} {kind:?} {}", render_operand(*src))
        }
        DenseOperands::Binary { dst, lhs, rhs } => {
            format!("r{dst} {} {}", render_operand(*lhs), render_operand(*rhs))
        }
        DenseOperands::Call { dst, name, args } => {
            let rendered_args: Vec<_> = args.iter().map(render_call_arg).collect();
            format!("r{dst} n{name} ({})", rendered_args.join(", "))
        }
        DenseOperands::NewObject {
            dst,
            class_name,
            display_class_name,
            args,
        } => {
            let rendered_args: Vec<_> = args.iter().map(render_call_arg).collect();
            format!(
                "r{dst} new n{class_name} (display n{display_class_name}) ({})",
                rendered_args.join(", ")
            )
        }
        DenseOperands::LoadConstFetchDim {
            key_dst,
            key_constant,
            dst,
            array,
            quiet,
        } => {
            format!(
                "r{key_dst} c{key_constant}; r{dst} {}[r{key_dst}]{}",
                render_operand(*array),
                if *quiet { " quiet" } else { "" }
            )
        }
        DenseOperands::LoadLocalLoadConst {
            first_dst,
            local,
            second_dst,
            constant,
        } => {
            format!(
                "r{first_dst} {}; r{second_dst} c{constant}",
                render_operand(*local)
            )
        }
        DenseOperands::CallableCall { dst, callee, args } => {
            let rendered_args: Vec<_> = args.iter().map(render_call_arg).collect();
            format!(
                "r{dst} callable {} ({})",
                render_operand(*callee),
                rendered_args.join(", ")
            )
        }
        DenseOperands::ResolveCallable { dst, kind, target } => {
            let kind = match kind {
                DenseCallableKind::FunctionName => "function",
                DenseCallableKind::MethodPlaceholder => "method_placeholder",
                DenseCallableKind::UnresolvedDynamic => "unresolved_dynamic",
            };
            format!("r{dst} {kind} n{target}")
        }
        DenseOperands::LoadConstPair {
            first_dst,
            first_constant,
            second_dst,
            second_constant,
        } => {
            format!("r{first_dst} c{first_constant}; r{second_dst} c{second_constant}")
        }
        DenseOperands::LoadConstArrayInsert {
            value_dst,
            value_constant,
            array,
            key,
        } => {
            let key = key.map_or("-".to_string(), render_operand);
            format!("r{value_dst} c{value_constant}; r{array} key={key}")
        }
        DenseOperands::Pipe {
            dst,
            input,
            callable,
        } => {
            format!(
                "r{dst} {} |> {}",
                render_operand(*input),
                render_operand(*callable)
            )
        }
        DenseOperands::MakeClosure {
            dst,
            function,
            captures,
        } => {
            let rendered: Vec<_> = captures
                .iter()
                .map(|capture| {
                    format!(
                        "n{}{}={}",
                        capture.name,
                        if capture.by_ref { " by_ref" } else { "" },
                        render_operand(capture.src)
                    )
                })
                .collect();
            format!(
                "r{dst} closure function:{function} [{}]",
                rendered.join(", ")
            )
        }
        DenseOperands::MethodCall {
            dst,
            object,
            method,
            args,
        } => {
            let rendered_args: Vec<_> = args.iter().map(render_call_arg).collect();
            format!(
                "r{dst} {}->n{method} ({})",
                render_operand(*object),
                rendered_args.join(", ")
            )
        }
        DenseOperands::StaticCall {
            dst,
            class_name,
            method,
            args,
        } => {
            let rendered_args: Vec<_> = args.iter().map(render_call_arg).collect();
            format!(
                "r{dst} n{class_name}::n{method} ({})",
                rendered_args.join(", ")
            )
        }
        DenseOperands::Dst { dst } => format!("r{dst}"),
        DenseOperands::ArrayInsert {
            array,
            key,
            value,
            by_ref_local,
        } => {
            let key = key.map_or_else(|| "[]".to_string(), render_operand);
            let suffix = by_ref_local.map_or_else(String::new, |local| format!(" by_ref=l{local}"));
            format!("r{array} {key} {}{suffix}", render_operand(*value))
        }
        DenseOperands::FetchDim {
            dst,
            array,
            key,
            quiet,
        } => format!(
            "r{dst} {} {} quiet={quiet}",
            render_operand(*array),
            render_operand(*key)
        ),
        DenseOperands::AssignDim {
            dst,
            local,
            dims,
            value,
        } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!(
                "r{dst} l{local} [{}] {}",
                dims.join(", "),
                render_operand(*value)
            )
        }
        DenseOperands::AssignPropertyDim {
            dst,
            object,
            property,
            dims,
            append,
            value,
        } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!(
                "r{dst} {} n{property} [{}]{} {}",
                render_operand(*object),
                dims.join(", "),
                if *append { " append" } else { "" },
                render_operand(*value)
            )
        }
        DenseOperands::AssignStaticProperty {
            dst,
            class_name,
            property,
            value,
        } => {
            format!(
                "r{dst} n{class_name} n{property} {}",
                render_operand(*value)
            )
        }
        DenseOperands::AssignDynamicProperty {
            dst,
            object,
            property,
            value,
        } => {
            format!(
                "r{dst} {} {} {}",
                render_operand(*object),
                render_operand(*property),
                render_operand(*value)
            )
        }
        DenseOperands::UnsetProperty { object, property } => {
            format!("{} n{property}", render_operand(*object))
        }
        DenseOperands::UnsetPropertyDim {
            object,
            property,
            dims,
        } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!(
                "{} n{property} [{}]",
                render_operand(*object),
                dims.join(", ")
            )
        }
        DenseOperands::InstanceOf {
            dst,
            object,
            class_name,
        } => {
            format!(
                "r{dst} = {} instanceof n{class_name}",
                render_operand(*object)
            )
        }
        DenseOperands::PropertyDimProbe {
            dst,
            object,
            property,
            dims,
        } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!(
                "r{dst} = probe {}->n{property}[{}]",
                render_operand(*object),
                dims.join(", ")
            )
        }
        DenseOperands::BindReferenceDim {
            local,
            dims,
            append,
            source,
        } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!(
                "l{local} [{}] append={append} =& l{source}",
                dims.join(", ")
            )
        }
        DenseOperands::IssetDim { dst, local, dims } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!("r{dst} l{local} [{}]", dims.join(", "))
        }
        DenseOperands::EmptyDim { dst, local, dims } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!("r{dst} l{local} [{}]", dims.join(", "))
        }
        DenseOperands::UnsetDim { local, dims } => {
            let dims: Vec<_> = dims.iter().copied().map(render_operand).collect();
            format!("l{local} [{}]", dims.join(", "))
        }
        DenseOperands::ForeachInit { iterator, source } => {
            format!("r{iterator} {}", render_operand(*source))
        }
        DenseOperands::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            let key = key.map_or_else(|| "-".to_string(), |key| format!("r{key}"));
            format!("r{has_value} r{iterator} key={key} value=r{value}")
        }
        DenseOperands::ForeachCleanup { iterator } => format!("r{iterator}"),
        DenseOperands::FetchProperty {
            dst,
            object,
            property,
        } => format!("r{dst} {} n{property}", render_operand(*object)),
        DenseOperands::AssignProperty {
            dst,
            object,
            property,
            value,
        } => format!(
            "r{dst} {} n{property} {}",
            render_operand(*object),
            render_operand(*value)
        ),
        DenseOperands::Operand { src } => render_operand(*src),
        DenseOperands::Jump { target } => format!("b{target}"),
        DenseOperands::JumpIf { condition, target } => {
            format!("{} b{target}", render_operand(*condition))
        }
        DenseOperands::JumpIfElse {
            condition,
            if_true,
            if_false,
        } => format!("{} b{if_true} b{if_false}", render_operand(*condition)),
        DenseOperands::Return { value } => value.map_or_else(|| "-".to_string(), render_operand),
        DenseOperands::Exit { value } => value.map_or_else(|| "-".to_string(), render_operand),
        DenseOperands::Include { dst, kind, path } => {
            format!("r{dst} {kind:?} {}", render_operand(*path))
        }
        DenseOperands::DeclareFunction { name, function } => {
            format!("n{name} fn{function}")
        }
        DenseOperands::DeclareClass { name } => format!("n{name}"),
        DenseOperands::FetchClassConstant {
            dst,
            class_name,
            constant,
        } => format!("r{dst} n{class_name} n{constant}"),
    }
}
