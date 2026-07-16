use crate::compilation::CompilationSession;

use super::*;

/// Lowers a Semantic frontend result through the single-file session adapter.
#[must_use]
pub fn lower_frontend_result(
    frontend: &FrontendResult,
    options: LoweringOptions,
) -> LoweringResult {
    let mut builder = IrBuilder::new(options.unit_id);
    let strict_types = frontend
        .database()
        .module(frontend.module().module_id())
        .and_then(|module| module.file_directives().strict_types())
        .is_some_and(|directive| matches!(directive.value(), DeclareValue::Int(1)));
    builder.set_strict_types(strict_types);
    let file = builder.add_file(options.source_path.clone());
    builder.set_file_strict_types(file, strict_types);
    let (_, diagnostics) = lower_frontend_into_builder(&mut builder, frontend, options, file, true);
    finish_lowering(builder, diagnostics)
}

/// Lowers a typed multi-file session into one linked runtime unit.
///
/// Every source is parsed and analyzed independently. Files are lowered in
/// deterministic dependency order into one ID arena, while file table IDs
/// remain stable insertion-order session IDs.
#[must_use]
pub fn lower_compilation_session(
    session: &CompilationSession,
    options: LoweringOptions,
) -> LoweringResult {
    let mut builder = IrBuilder::new(options.unit_id);
    let files = session
        .files()
        .iter()
        .map(|source| {
            let file = builder.add_file(source.path());
            builder.set_file_strict_types(file, source.strict_types());
            file
        })
        .collect::<Vec<_>>();
    let entry_strict_types = session
        .files()
        .get(session.entry().index())
        .is_some_and(|source| source.strict_types());
    builder.set_strict_types(entry_strict_types);

    let mut diagnostics = Vec::new();
    let mut linked_file_entries = Vec::new();
    for source_id in session.lowering_order() {
        let Some(source) = session.files().get(source_id.index()) else {
            continue;
        };
        let (entry, file_diagnostics) = lower_frontend_into_builder(
            &mut builder,
            source.frontend(),
            LoweringOptions {
                source_path: source.path().to_owned(),
                source_text: Some(source.source().to_owned()),
                ..options.clone()
            },
            files[source_id.index()],
            source_id == session.entry(),
        );
        linked_file_entries.push(entry);
        diagnostics.extend(file_diagnostics);
    }
    builder.set_linked_file_entries(linked_file_entries);
    builder.set_linked_entry_autoload_declarations(
        session
            .lowering_order()
            .iter()
            .map(|source_id| {
                session
                    .autoload_declaration(*source_id)
                    .map(ToOwned::to_owned)
            })
            .collect(),
    );
    finish_lowering(builder, diagnostics)
}

fn finish_lowering(builder: IrBuilder, diagnostics: Vec<LoweringDiagnostic>) -> LoweringResult {
    let unit = builder.finish();
    let verification = verify_unit(&unit);
    LoweringResult {
        unit,
        diagnostics,
        verification,
    }
}

fn lower_frontend_into_builder(
    builder: &mut IrBuilder,
    frontend: &FrontendResult,
    options: LoweringOptions,
    file: FileId,
    is_entry: bool,
) -> (FunctionId, Vec<LoweringDiagnostic>) {
    let module_span = frontend
        .database()
        .source_map()
        .span(frontend.module().module_id())
        .unwrap_or_else(|| TextRange::new(0, frontend.module().source_bytes()));
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        span_from_range(file, module_span),
    );
    let prelude_block = builder.append_block(function);
    let block = builder.append_block(function);
    let null_const = builder.intern_constant(IrConstant::Null);
    let module_ir_span = span_from_range(file, module_span);
    let module_origin = format!("hir:module:{}", frontend.module().module_id().raw());
    builder.add_source_map(
        IrSourceMapTarget::Function { function },
        module_origin.clone(),
        module_ir_span,
    );
    builder.add_source_map(
        IrSourceMapTarget::Block {
            function,
            block: prelude_block,
        },
        module_origin.clone(),
        module_ir_span,
    );
    builder.add_source_map(
        IrSourceMapTarget::Block { function, block },
        module_origin.clone(),
        module_ir_span,
    );

    let mut context = LoweringContext::new(frontend, options, file);
    context.function_names.insert(function, String::new());
    context.namespace_names.insert(function, String::new());
    let block = context.lower_global_constant_declarations(builder, function, block);
    context.lower_function_declarations(builder, function);
    context.lower_class_declarations(builder, function);
    let current_block = context.lower_top_level(builder, function, block);
    if !builder.is_terminated(function, current_block) {
        builder.terminate_return(
            function,
            current_block,
            Some(Operand::Constant(null_const)),
            span_from_range(file, module_span),
        );
        builder.add_source_map(
            IrSourceMapTarget::Terminator {
                function,
                block: current_block,
            },
            module_origin.clone(),
            module_ir_span,
        );
    }
    context.emit_early_diagnostics(builder, function, prelude_block);
    builder.terminate_jump(function, prelude_block, block, module_ir_span);
    builder.add_source_map(
        IrSourceMapTarget::Terminator {
            function,
            block: prelude_block,
        },
        module_origin,
        module_ir_span,
    );
    if is_entry {
        builder.set_entry(function);
    }
    (function, context.diagnostics)
}
