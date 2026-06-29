//! PHP semantic frontend surface.
//!
//! This crate exposes the public semantic analysis API and verification skeleton. Detailed
//! declaration collection, HIR lowering, name resolution, and diagnostics are
//! added in later frontend checks.

pub mod checks;
pub mod db;
pub mod diagnostics;
pub mod hir;
pub mod lower;
pub mod query;
pub mod scopes;
pub mod symbols;

pub use db::{FrontendDatabase, SourceMap, SourceMappedId};
pub use diagnostics::{
    DiagnosticId, DiagnosticLabel, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity,
    SemanticDiagnostic, Severity,
};

use php_syntax::ParseDiagnostic;

/// Target PHP version for semantic analysis.
pub const TARGET_PHP_VERSION: &str = "8.5";

/// Full semantic frontend result for one source file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontendResult {
    parser_diagnostics: Vec<ParseDiagnostic>,
    semantic_diagnostics: Vec<SemanticDiagnostic>,
    module: SemanticModule,
    database: FrontendDatabase,
}

impl FrontendResult {
    /// Returns parser diagnostics emitted before semantic analysis.
    #[must_use]
    pub fn parser_diagnostics(&self) -> &[ParseDiagnostic] {
        &self.parser_diagnostics
    }

    /// Returns semantic diagnostics.
    #[must_use]
    pub fn semantic_diagnostics(&self) -> &[SemanticDiagnostic] {
        &self.semantic_diagnostics
    }

    /// Returns true if parser or semantic diagnostics contain errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.parser_diagnostics.is_empty()
            || self
                .semantic_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity() == Severity::Error)
    }

    /// Returns the analyzed semantic module summary.
    #[must_use]
    pub const fn module(&self) -> &SemanticModule {
        &self.module
    }

    /// Returns the semantic frontend database.
    #[must_use]
    pub const fn database(&self) -> &FrontendDatabase {
        &self.database
    }

    /// Renders stable minimal JSON for frontend and runtime CLI and smoke tests.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"engine\":\"phrust-frontend\",\"target_php_version\":\"");
        out.push_str(TARGET_PHP_VERSION);
        out.push_str("\",\"ok\":");
        out.push_str(if self.has_errors() { "false" } else { "true" });
        out.push_str(",\"module\":{");
        out.push_str("\"root_kind\":\"");
        out.push_str(&escape_json(&self.module.root_kind));
        out.push_str("\",\"source_bytes\":");
        out.push_str(&self.module.source_bytes.to_string());
        out.push_str(",\"module_id\":");
        out.push_str(&self.module.module_id.raw().to_string());
        out.push_str(",\"file_directives\":");
        push_file_directives_json(&mut out, self);
        out.push_str(",\"declares\":");
        push_declares_json(&mut out, self);
        out.push_str(",\"namespaces\":");
        push_namespaces_json(&mut out, self);
        out.push_str(",\"symbols\":");
        push_symbols_json(&mut out, self);
        out.push_str(",\"scopes\":");
        push_scopes_json(&mut out, self);
        out.push_str(",\"types\":");
        push_types_json(&mut out, self);
        out.push_str(",\"signatures\":");
        push_signatures_json(&mut out, self);
        out.push_str(",\"statements\":");
        push_statements_json(&mut out, self);
        out.push_str(",\"expressions\":");
        push_expressions_json(&mut out, self);
        out.push_str(",\"const_exprs\":");
        push_const_exprs_json(&mut out, self);
        out.push_str(",\"attributes\":");
        push_attributes_json(&mut out, self);
        out.push_str(",\"class_likes\":");
        push_class_likes_json(&mut out, self);
        out.push_str(",\"trait_use_decls\":");
        push_trait_use_decls_json(&mut out, self);
        out.push_str(",\"enum_cases\":");
        push_enum_cases_json(&mut out, self);
        out.push_str(",\"methods\":");
        push_methods_json(&mut out, self);
        out.push_str(",\"properties\":");
        push_properties_json(&mut out, self);
        out.push_str(",\"class_consts\":");
        push_class_consts_json(&mut out, self);
        out.push_str("},\"parser_diagnostics\":");
        out.push_str(&self.parser_diagnostics.len().to_string());
        out.push_str(",\"semantic_diagnostics\":[");
        for (index, diagnostic) in self.semantic_diagnostics.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&diagnostic.to_json());
        }
        out.push_str("]}");
        out
    }

    /// Renders a stable declaration table JSON object.
    #[must_use]
    pub fn to_symbols_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"engine\":\"phrust-frontend\",\"target_php_version\":\"");
        out.push_str(TARGET_PHP_VERSION);
        out.push_str("\",\"ok\":");
        out.push_str(if self.has_errors() { "false" } else { "true" });
        out.push_str(",\"symbols\":");
        push_symbols_json(&mut out, self);
        out.push_str(",\"semantic_diagnostics\":[");
        for (index, diagnostic) in self.semantic_diagnostics.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&diagnostic.to_json());
        }
        out.push_str("]}");
        out
    }

    /// Renders the lexical scope tree as stable JSON.
    #[must_use]
    pub fn to_scopes_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"engine\":\"phrust-frontend\",\"target_php_version\":\"");
        out.push_str(TARGET_PHP_VERSION);
        out.push_str("\",\"ok\":");
        out.push_str(if self.has_errors() { "false" } else { "true" });
        out.push_str(",\"scopes\":");
        push_scopes_json(&mut out, self);
        out.push_str(",\"semantic_diagnostics\":[");
        for (index, diagnostic) in self.semantic_diagnostics.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&diagnostic.to_json());
        }
        out.push_str("]}");
        out
    }

    /// Renders parser and semantic diagnostics as stable JSON.
    #[must_use]
    pub fn to_diagnostics_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"engine\":\"phrust-frontend\",\"target_php_version\":\"");
        out.push_str(TARGET_PHP_VERSION);
        out.push_str("\",\"ok\":");
        out.push_str(if self.has_errors() { "false" } else { "true" });
        out.push_str(",\"parser_diagnostics\":");
        out.push_str(&self.parser_diagnostics.len().to_string());
        out.push_str(",\"semantic_diagnostics\":[");
        for (index, diagnostic) in self.semantic_diagnostics.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&diagnostic.to_json());
        }
        out.push_str("]}");
        out
    }

    /// Renders HIR-focused data as stable JSON.
    #[must_use]
    pub fn to_hir_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"engine\":\"phrust-frontend\",\"target_php_version\":\"");
        out.push_str(TARGET_PHP_VERSION);
        out.push_str("\",\"ok\":");
        out.push_str(if self.has_errors() { "false" } else { "true" });
        out.push_str(",\"hir\":{");
        out.push_str("\"file_directives\":");
        push_file_directives_json(&mut out, self);
        out.push_str(",\"declares\":");
        push_declares_json(&mut out, self);
        out.push_str(",\"statements\":");
        push_statements_json(&mut out, self);
        out.push_str(",\"expressions\":");
        push_expressions_json(&mut out, self);
        out.push_str(",\"types\":");
        push_types_json(&mut out, self);
        out.push_str(",\"const_exprs\":");
        push_const_exprs_json(&mut out, self);
        out.push_str(",\"attributes\":");
        push_attributes_json(&mut out, self);
        out.push_str(",\"class_likes\":");
        push_class_likes_json(&mut out, self);
        out.push_str(",\"trait_use_decls\":");
        push_trait_use_decls_json(&mut out, self);
        out.push_str(",\"enum_cases\":");
        push_enum_cases_json(&mut out, self);
        out.push_str(",\"methods\":");
        push_methods_json(&mut out, self);
        out.push_str(",\"properties\":");
        push_properties_json(&mut out, self);
        out.push_str(",\"class_consts\":");
        push_class_consts_json(&mut out, self);
        out.push_str("},\"semantic_diagnostics\":[");
        for (index, diagnostic) in self.semantic_diagnostics.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&diagnostic.to_json());
        }
        out.push_str("]}");
        out
    }

    /// Renders the lexical scope tree as indented text.
    #[must_use]
    pub fn to_scopes_text(&self) -> String {
        let mut out = String::new();
        if let Some(module) = self.database.module(self.module.module_id)
            && let Some(root) = module.scopes().root()
        {
            push_scope_text(&mut out, module.scopes(), root, 0);
        }
        out
    }
}

/// Minimal module summary. Later passes replace this with full HIR data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticModule {
    module_id: hir::ModuleId,
    root_kind: String,
    source_bytes: usize,
}

impl SemanticModule {
    /// Returns the HIR module ID in the frontend database.
    #[must_use]
    pub const fn module_id(&self) -> hir::ModuleId {
        self.module_id
    }

    /// Returns the CST root kind used as the initial module anchor.
    #[must_use]
    pub fn root_kind(&self) -> &str {
        &self.root_kind
    }

    /// Returns source size in bytes.
    #[must_use]
    pub const fn source_bytes(&self) -> usize {
        self.source_bytes
    }
}

/// Analyzes a PHP source string using the Semantic frontend frontend pipeline.
#[must_use]
pub fn analyze_source(source: &str) -> FrontendResult {
    query::frontend::analyze_file(source, &query::frontend::FrontendOptions::default())
}

fn push_namespaces_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, namespace)) in module.namespaces().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"name\":");
            if let Some(name) = namespace.name() {
                out.push('"');
                out.push_str(&escape_json(name.text()));
                out.push('"');
            } else {
                out.push_str("null");
            }
            out.push_str(",\"form\":\"");
            out.push_str(namespace.form().as_str());
            out.push_str("\",\"span\":");
            push_span_json(out, namespace.span());
            out.push_str(",\"imports\":");
            push_imports_json(out, namespace.imports());
            out.push_str(",\"items\":[");
            for (item_index, item) in namespace.items().iter().enumerate() {
                if item_index > 0 {
                    out.push(',');
                }
                out.push_str("{\"kind\":\"");
                out.push_str(item.kind().as_str());
                out.push_str("\",\"span\":");
                push_span_json(out, item.span());
                out.push('}');
            }
            out.push_str("],\"resolved_names\":");
            push_resolved_names_json(out, namespace.resolved_names());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_imports_json(out: &mut String, imports: &symbols::imports::ImportTable) {
    out.push('[');
    for (index, import) in imports.entries().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"kind\":\"");
        out.push_str(import.kind().as_str());
        out.push_str("\",\"name\":\"");
        out.push_str(&escape_json(
            &import.name().canonical(import.kind().alias_name_kind()),
        ));
        out.push_str("\",\"alias\":\"");
        out.push_str(&escape_json(import.alias()));
        out.push_str("\",\"alias_canonical\":\"");
        out.push_str(&escape_json(import.alias_canonical()));
        out.push_str("\",\"explicit_alias\":");
        out.push_str(if import.is_explicit_alias() {
            "true"
        } else {
            "false"
        });
        out.push_str(",\"span\":");
        push_span_json(out, import.span());
        out.push('}');
    }
    out.push(']');
}

fn push_resolved_names_json(out: &mut String, names: &[symbols::resolution::ResolvedNameRecord]) {
    out.push('[');
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let name_kind = symbols::resolution::NameResolver::name_kind(name.context());
        out.push_str("{\"source\":\"");
        out.push_str(&escape_json(name.source().original()));
        out.push_str("\",\"source_canonical\":\"");
        out.push_str(&escape_json(&name.source().canonical(name_kind)));
        out.push_str("\",\"context\":\"");
        out.push_str(name.context().as_str());
        out.push_str("\",\"classification\":\"");
        out.push_str(name.result().classification());
        out.push_str("\",\"span\":");
        push_span_json(out, name.span());
        match name.result() {
            symbols::resolution::ResolvedName::FullyQualified(resolved) => {
                out.push_str(",\"resolved\":\"");
                out.push_str(&escape_json(&resolved.canonical(name_kind)));
                out.push_str("\",\"fallback\":null");
            }
            symbols::resolution::ResolvedName::MaybeRuntimeFallback {
                namespaced,
                fallback,
            } => {
                out.push_str(",\"resolved\":\"");
                out.push_str(&escape_json(&namespaced.canonical(name_kind)));
                out.push_str("\",\"fallback\":\"");
                out.push_str(&escape_json(&fallback.canonical(name_kind)));
                out.push('"');
            }
            symbols::resolution::ResolvedName::Dynamic
            | symbols::resolution::ResolvedName::Unresolved => {
                out.push_str(",\"resolved\":null,\"fallback\":null");
            }
        }
        out.push('}');
    }
    out.push(']');
}

fn push_symbols_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, declaration) in module.declaration_table().entries().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let name_kind = declaration.kind().duplicate_name_kind();
            out.push_str("{\"decl_id\":");
            out.push_str(&declaration.decl_id().raw().to_string());
            out.push_str(",\"symbol_id\":");
            out.push_str(&declaration.symbol_id().raw().to_string());
            out.push_str(",\"kind\":\"");
            out.push_str(declaration.kind().as_str());
            out.push_str("\",\"name\":\"");
            out.push_str(&escape_json(declaration.name()));
            out.push_str("\",\"fqn\":\"");
            out.push_str(&escape_json(&declaration.fqn().canonical(name_kind)));
            out.push_str("\",\"conditional\":");
            out.push_str(if declaration.kind().is_conditional() {
                "true"
            } else {
                "false"
            });
            out.push_str(",\"span\":");
            push_span_json(out, declaration.span());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_scopes_json(out: &mut String, result: &FrontendResult) {
    if let Some(module) = result.database.module(result.module.module_id)
        && let Some(root) = module.scopes().root()
    {
        push_scope_json(out, module.scopes(), root);
    } else {
        out.push_str("null");
    }
}

fn push_types_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, hir_type)) in module.types().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"context\":\"");
            out.push_str(hir_type.context().as_str());
            out.push_str("\",\"source\":\"");
            out.push_str(&escape_json(hir_type.source_form()));
            out.push_str("\",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            push_type_kind_json(out, module, hir_type.kind());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_signatures_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, signature) in module.signatures().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"kind\":\"");
            out.push_str(signature.kind().as_str());
            out.push_str("\",\"name\":");
            if let Some(name) = signature.name() {
                out.push('"');
                out.push_str(&escape_json(name));
                out.push('"');
            } else {
                out.push_str("null");
            }
            out.push_str(",\"by_ref_return\":");
            out.push_str(if signature.by_ref_return() {
                "true"
            } else {
                "false"
            });
            out.push_str(",\"flags\":");
            push_function_like_flags_json(out, signature.flags());
            out.push_str(",\"arrow_body\":");
            if let Some(span) = signature.arrow_body() {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"return_type\":");
            if let Some(return_type) = signature.return_type() {
                out.push_str("{\"type_id\":");
                out.push_str(&return_type.type_id().raw().to_string());
                out.push_str(",\"span\":");
                push_span_json(out, return_type.span());
                out.push('}');
            } else {
                out.push_str("null");
            }
            out.push_str(",\"span\":");
            push_span_json(out, signature.span());
            out.push_str(",\"parameters\":[");
            for (parameter_index, parameter) in signature.parameters().iter().enumerate() {
                if parameter_index > 0 {
                    out.push(',');
                }
                push_signature_parameter_json(out, parameter);
            }
            out.push_str("]}");
        }
    }
    out.push(']');
}

fn push_function_like_flags_json(out: &mut String, flags: &hir::FunctionLikeFlags) {
    out.push_str("{\"returns_by_ref\":");
    out.push_str(if flags.returns_by_ref() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"is_static\":");
    out.push_str(if flags.is_static() { "true" } else { "false" });
    out.push_str(",\"is_generator\":");
    out.push_str(if flags.is_generator() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"has_return_type_void\":");
    out.push_str(if flags.has_return_type_void() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"has_return_type_never\":");
    out.push_str(if flags.has_return_type_never() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"has_tentative_or_deferred_info\":");
    out.push_str(if flags.has_tentative_or_deferred_info() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"this_available\":");
    out.push_str(if flags.this_available() {
        "true"
    } else {
        "false"
    });
    out.push('}');
}

fn push_file_directives_json(out: &mut String, result: &FrontendResult) {
    if let Some(module) = result.database.module(result.module.module_id) {
        let directives = module.file_directives();
        out.push_str("{\"strict_types\":");
        push_optional_declare_directive_json(out, directives.strict_types());
        out.push_str(",\"encoding\":");
        push_optional_declare_directive_json(out, directives.encoding());
        out.push_str(",\"ticks\":");
        push_optional_declare_directive_json(out, directives.ticks());
        out.push_str(",\"unknown\":[");
        for (index, directive) in directives.unknown().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            push_declare_directive_json(out, directive);
        }
        out.push_str("]}");
    } else {
        out.push_str("null");
    }
}

fn push_declares_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, declare) in module.declares().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"span\":");
            push_span_json(out, declare.span());
            out.push_str(",\"directives\":[");
            for (directive_index, directive) in declare.directives().iter().enumerate() {
                if directive_index > 0 {
                    out.push(',');
                }
                push_declare_directive_json(out, directive);
            }
            out.push_str("]}");
        }
    }
    out.push(']');
}

fn push_optional_declare_directive_json(
    out: &mut String,
    directive: Option<&hir::DeclareDirective>,
) {
    if let Some(directive) = directive {
        push_declare_directive_json(out, directive);
    } else {
        out.push_str("null");
    }
}

fn push_declare_directive_json(out: &mut String, directive: &hir::DeclareDirective) {
    out.push_str("{\"name\":\"");
    out.push_str(&escape_json(directive.name()));
    out.push_str("\",\"span\":");
    push_span_json(out, directive.span());
    out.push_str(",\"value_span\":");
    if let Some(span) = directive.value_span() {
        push_span_json(out, span);
    } else {
        out.push_str("null");
    }
    out.push_str(",\"value\":");
    push_declare_value_json(out, directive.value());
    out.push('}');
}

fn push_declare_value_json(out: &mut String, value: &hir::DeclareValue) {
    out.push_str("{\"kind\":\"");
    out.push_str(value.kind());
    out.push('"');
    match value {
        hir::DeclareValue::Int(value) => {
            out.push_str(",\"value\":");
            out.push_str(&value.to_string());
        }
        hir::DeclareValue::String(value) | hir::DeclareValue::Unknown(value) => {
            out.push_str(",\"value\":\"");
            out.push_str(&escape_json(value));
            out.push('"');
        }
    }
    out.push('}');
}

fn push_signature_parameter_json(out: &mut String, parameter: &hir::Parameter) {
    out.push_str("{\"name\":\"");
    out.push_str(&escape_json(parameter.name()));
    out.push_str("\",\"type_id\":");
    if let Some(type_id) = parameter.type_id() {
        out.push_str(&type_id.raw().to_string());
    } else {
        out.push_str("null");
    }
    out.push_str(",\"by_ref\":");
    out.push_str(if parameter.flags().is_by_ref() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"variadic\":");
    out.push_str(if parameter.flags().is_variadic() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"default\":");
    if let Some(default) = parameter.default() {
        out.push_str("{\"span\":");
        push_span_json(out, default.span());
        out.push_str(",\"const_expr_candidate\":");
        out.push_str(if default.is_const_expr_candidate() {
            "true"
        } else {
            "false"
        });
        out.push('}');
    } else {
        out.push_str("null");
    }
    out.push_str(",\"attributes\":[");
    for (index, attribute) in parameter.attributes().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"span\":");
        push_span_json(out, attribute.span());
        out.push('}');
    }
    out.push_str("],\"promoted_property\":");
    if let Some(promotion) = parameter.flags().promoted_property() {
        out.push_str("{\"visibility\":\"");
        out.push_str(promotion.visibility().as_str());
        out.push_str("\",\"readonly\":");
        out.push_str(if promotion.is_readonly() {
            "true"
        } else {
            "false"
        });
        out.push_str(",\"set_visibility\":");
        if let Some(set_visibility) = promotion.set_visibility() {
            out.push('"');
            out.push_str(set_visibility.as_str());
            out.push('"');
        } else {
            out.push_str("null");
        }
        out.push_str(",\"span\":");
        push_span_json(out, promotion.span());
        out.push('}');
    } else {
        out.push_str("null");
    }
    out.push_str(",\"span\":");
    push_span_json(out, parameter.span());
    out.push('}');
}

fn push_statements_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, statement)) in module.statements().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"kind\":\"");
            out.push_str(statement.kind().as_str());
            out.push_str("\",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            push_statement_kind_json(out, statement.kind());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_expressions_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, expression)) in module.expressions().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"kind\":\"");
            out.push_str(expression.kind().as_str());
            out.push_str("\",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            push_expression_kind_json(out, expression.kind());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_const_exprs_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, const_expr)) in module.const_exprs().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"context\":\"");
            out.push_str(const_expr.context().as_str());
            out.push_str("\",\"kind\":\"");
            out.push_str(const_expr.kind().as_str());
            out.push_str("\",\"expr_id\":");
            out.push_str(&const_expr.expr_id().raw().to_string());
            out.push_str(",\"allowed\":");
            out.push_str(if const_expr.is_allowed() {
                "true"
            } else {
                "false"
            });
            out.push_str(",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"folded_value\":");
            if let Some(value) = const_expr.folded_value() {
                push_const_value_json(out, value);
            } else {
                out.push_str("null");
            }
            out.push('}');
        }
    }
    out.push(']');
}

fn push_const_value_json(out: &mut String, value: &hir::ConstValue) {
    out.push_str("{\"kind\":\"");
    out.push_str(value.kind());
    out.push('"');
    match value {
        hir::ConstValue::Null | hir::ConstValue::ClosureConst | hir::ConstValue::CallableConst => {}
        hir::ConstValue::Bool(value) => {
            out.push_str(",\"value\":");
            out.push_str(if *value { "true" } else { "false" });
        }
        hir::ConstValue::Int(value) => {
            out.push_str(",\"value\":");
            out.push_str(&value.to_string());
        }
        hir::ConstValue::String(value) | hir::ConstValue::UnresolvedRef(value) => {
            out.push_str(",\"value\":\"");
            out.push_str(&escape_json(value));
            out.push('"');
        }
    }
    out.push('}');
}

fn push_attributes_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, attribute)) in module.attributes().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"target\":\"");
            out.push_str(attribute.target().as_str());
            out.push_str("\",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"name\":");
            push_name_resolution_json(out, attribute.name());
            out.push_str(",\"args\":");
            push_expr_ids_json(out, attribute.args());
            out.push_str(",\"repeated_on_target\":");
            out.push_str(if attribute.is_repeated_on_target() {
                "true"
            } else {
                "false"
            });
            out.push('}');
        }
    }
    out.push(']');
}

fn push_class_likes_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, class_like)) in module.class_likes().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"kind\":\"");
            out.push_str(class_like.kind().as_str());
            out.push_str("\",\"name\":");
            push_optional_string_json(out, class_like.name());
            out.push_str(",\"fqn\":");
            if let Some(fqn) = class_like.fqn() {
                out.push('"');
                out.push_str(&escape_json(&fqn.canonical(hir::NameKind::ClassLike)));
                out.push('"');
            } else {
                out.push_str("null");
            }
            out.push_str(",\"anonymous_id\":");
            push_optional_string_json(out, class_like.anonymous_id());
            out.push_str(",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"modifiers\":");
            push_modifier_set_json(out, class_like.modifiers());
            out.push_str(",\"extends\":[");
            for (ref_index, name) in class_like.extends().iter().enumerate() {
                if ref_index > 0 {
                    out.push(',');
                }
                push_name_resolution_json(out, name);
            }
            out.push_str("],\"implements\":[");
            for (ref_index, name) in class_like.implements().iter().enumerate() {
                if ref_index > 0 {
                    out.push(',');
                }
                push_name_resolution_json(out, name);
            }
            out.push_str("],\"trait_uses\":[");
            for (ref_index, name) in class_like.trait_uses().iter().enumerate() {
                if ref_index > 0 {
                    out.push(',');
                }
                push_name_resolution_json(out, name);
            }
            out.push_str("],\"members\":[");
            for (member_index, member) in class_like.members().iter().enumerate() {
                if member_index > 0 {
                    out.push(',');
                }
                out.push_str("{\"kind\":\"");
                out.push_str(member.kind().as_str());
                out.push_str("\",\"name\":");
                push_optional_string_json(out, member.name());
                out.push_str(",\"id\":");
                push_class_like_member_id_json(out, member.id());
                out.push('}');
            }
            out.push_str("],\"attributes\":");
            push_attribute_ids_json(out, class_like.attributes());
            out.push_str(",\"backing_type\":");
            if let Some(type_id) = class_like.backing_type() {
                out.push_str(&type_id.raw().to_string());
            } else {
                out.push_str("null");
            }
            out.push('}');
        }
    }
    out.push(']');
}

fn push_class_like_member_id_json(out: &mut String, id: Option<hir::ClassLikeMemberId>) {
    let Some(id) = id else {
        out.push_str("null");
        return;
    };
    match id {
        hir::ClassLikeMemberId::Method(id) => {
            out.push_str("{\"kind\":\"method\",\"id\":");
            out.push_str(&id.raw().to_string());
            out.push('}');
        }
        hir::ClassLikeMemberId::Property(id) => {
            out.push_str("{\"kind\":\"property\",\"id\":");
            out.push_str(&id.raw().to_string());
            out.push('}');
        }
        hir::ClassLikeMemberId::ClassConstant(id) => {
            out.push_str("{\"kind\":\"class_constant\",\"id\":");
            out.push_str(&id.raw().to_string());
            out.push('}');
        }
        hir::ClassLikeMemberId::TraitUse(id) => {
            out.push_str("{\"kind\":\"trait_use\",\"id\":");
            out.push_str(&id.raw().to_string());
            out.push('}');
        }
        hir::ClassLikeMemberId::EnumCase(id) => {
            out.push_str("{\"kind\":\"enum_case\",\"id\":");
            out.push_str(&id.raw().to_string());
            out.push('}');
        }
    }
}

fn push_enum_cases_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, enum_case)) in module.enum_cases().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"class_like\":");
            out.push_str(&enum_case.class_like().raw().to_string());
            out.push_str(",\"name\":");
            push_optional_string_json(out, enum_case.name());
            out.push_str(",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"value\":");
            if let Some(value) = enum_case.value() {
                out.push_str(&value.raw().to_string());
            } else {
                out.push_str("null");
            }
            out.push_str(",\"attributes\":");
            push_attribute_ids_json(out, enum_case.attributes());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_trait_use_decls_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, trait_use)) in module.trait_uses().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"class_like\":");
            out.push_str(&trait_use.class_like().raw().to_string());
            out.push_str(",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"traits\":[");
            for (trait_index, name) in trait_use.traits().iter().enumerate() {
                if trait_index > 0 {
                    out.push(',');
                }
                push_name_resolution_json(out, name);
            }
            out.push_str("],\"adaptations\":[");
            for (adaptation_index, adaptation) in trait_use.adaptations().iter().enumerate() {
                if adaptation_index > 0 {
                    out.push(',');
                }
                push_trait_adaptation_json(out, adaptation);
            }
            out.push_str("]}");
        }
    }
    out.push(']');
}

fn push_trait_adaptation_json(out: &mut String, adaptation: &hir::HirTraitAdaptation) {
    out.push_str("{\"kind\":\"");
    out.push_str(adaptation.kind().as_str());
    out.push_str("\",\"span\":");
    push_span_json(out, adaptation.span());
    out.push_str(",\"method\":");
    push_trait_method_ref_json(out, adaptation.method());
    match adaptation.kind() {
        hir::HirTraitAdaptationKind::Precedence { instead_of } => {
            out.push_str(",\"instead_of\":[");
            for (index, name) in instead_of.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                push_name_resolution_json(out, name);
            }
            out.push(']');
        }
        hir::HirTraitAdaptationKind::Alias { alias, visibility } => {
            out.push_str(",\"alias\":");
            push_optional_string_json(out, alias.as_deref());
            out.push_str(",\"visibility\":");
            push_optional_string_json(out, visibility.as_deref());
        }
    }
    out.push('}');
}

fn push_trait_method_ref_json(out: &mut String, method: &hir::HirTraitMethodRef) {
    out.push_str("{\"trait\":");
    if let Some(trait_name) = method.trait_name() {
        push_name_resolution_json(out, trait_name);
    } else {
        out.push_str("null");
    }
    out.push_str(",\"method\":\"");
    out.push_str(&escape_json(method.method()));
    out.push_str("\"}");
}

fn push_methods_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, method)) in module.methods().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"class_like\":");
            out.push_str(&method.class_like().raw().to_string());
            out.push_str(",\"name\":");
            push_optional_string_json(out, method.name());
            out.push_str(",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"modifiers\":");
            push_modifier_set_json(out, method.modifiers());
            out.push_str(",\"has_body\":");
            out.push_str(if method.has_body() { "true" } else { "false" });
            out.push_str(",\"signature_index\":");
            if let Some(signature_index) = method.signature_index() {
                out.push_str(&signature_index.to_string());
            } else {
                out.push_str("null");
            }
            out.push_str(",\"magic_kind\":");
            if let Some(magic_kind) = method.magic_kind() {
                out.push('"');
                out.push_str(magic_kind.as_str());
                out.push('"');
            } else {
                out.push_str("null");
            }
            out.push_str(",\"attributes\":");
            push_attribute_ids_json(out, method.attributes());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_properties_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, property)) in module.properties().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"class_like\":");
            out.push_str(&property.class_like().raw().to_string());
            out.push_str(",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"modifiers\":");
            push_modifier_set_json(out, property.modifiers());
            out.push_str(",\"type\":");
            push_optional_type_id_json(out, property.type_id());
            out.push_str(",\"items\":[");
            for (item_index, item) in property.items().iter().enumerate() {
                if item_index > 0 {
                    out.push(',');
                }
                out.push_str("{\"name\":\"");
                out.push_str(&escape_json(item.name()));
                out.push_str("\",\"default\":");
                if let Some(default) = item.default() {
                    out.push_str(&default.raw().to_string());
                } else {
                    out.push_str("null");
                }
                out.push_str(",\"promoted\":");
                push_promoted_property_json(out, item.promoted());
                out.push('}');
            }
            out.push_str("],\"hooks\":[");
            for (hook_index, hook) in property.hooks().iter().enumerate() {
                if hook_index > 0 {
                    out.push(',');
                }
                out.push_str("{\"kind\":\"");
                out.push_str(&escape_json(hook.kind()));
                out.push_str("\",\"body\":\"");
                out.push_str(match hook.body() {
                    hir::HirPropertyHookBody::Expression => "expression",
                    hir::HirPropertyHookBody::Block => "block",
                });
                out.push_str("\",\"span\":");
                push_span_json(out, hook.span());
                out.push('}');
            }
            out.push_str("],\"attributes\":");
            push_attribute_ids_json(out, property.attributes());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_class_consts_json(out: &mut String, result: &FrontendResult) {
    out.push('[');
    if let Some(module) = result.database.module(result.module.module_id) {
        for (index, (id, class_const)) in module.class_consts().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            out.push_str(&id.raw().to_string());
            out.push_str(",\"class_like\":");
            out.push_str(&class_const.class_like().raw().to_string());
            out.push_str(",\"name\":");
            push_optional_string_json(out, class_const.name());
            out.push_str(",\"span\":");
            if let Some(span) = result.database.source_map().span(id) {
                push_span_json(out, span);
            } else {
                out.push_str("null");
            }
            out.push_str(",\"modifiers\":");
            push_modifier_set_json(out, class_const.modifiers());
            out.push_str(",\"type\":");
            push_optional_type_id_json(out, class_const.type_id());
            out.push_str(",\"value\":");
            if let Some(value) = class_const.value() {
                out.push_str(&value.raw().to_string());
            } else {
                out.push_str("null");
            }
            out.push_str(",\"attributes\":");
            push_attribute_ids_json(out, class_const.attributes());
            out.push('}');
        }
    }
    out.push(']');
}

fn push_promoted_property_json(out: &mut String, promoted: Option<&hir::PromotedPropertyInfo>) {
    let Some(promoted) = promoted else {
        out.push_str("null");
        return;
    };
    out.push_str("{\"visibility\":\"");
    out.push_str(promoted.visibility().as_str());
    out.push_str("\",\"readonly\":");
    out.push_str(if promoted.is_readonly() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"set_visibility\":");
    if let Some(visibility) = promoted.set_visibility() {
        out.push('"');
        out.push_str(visibility.as_str());
        out.push('"');
    } else {
        out.push_str("null");
    }
    out.push_str(",\"span\":");
    push_span_json(out, promoted.span());
    out.push('}');
}

fn push_modifier_set_json(out: &mut String, modifiers: &hir::ModifierSet) {
    out.push_str("{\"abstract\":");
    out.push_str(if modifiers.is_abstract() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"final\":");
    out.push_str(if modifiers.is_final() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"static\":");
    out.push_str(if modifiers.is_static() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"readonly\":");
    out.push_str(if modifiers.is_readonly() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"visibility\":");
    if let Some(visibility) = modifiers.visibility() {
        out.push('"');
        out.push_str(visibility.as_str());
        out.push('"');
    } else {
        out.push_str("null");
    }
    out.push_str(",\"set_visibility\":");
    if let Some(visibility) = modifiers.set_visibility() {
        out.push('"');
        out.push_str(visibility.as_str());
        out.push('"');
    } else {
        out.push_str("null");
    }
    out.push('}');
}

fn push_name_resolution_json(out: &mut String, name: &hir::HirNameResolution) {
    out.push_str("{\"source\":\"");
    out.push_str(&escape_json(name.source()));
    out.push_str("\",\"context\":\"");
    out.push_str(name.context());
    out.push_str("\",\"classification\":\"");
    out.push_str(name.classification());
    out.push_str("\",\"resolved\":");
    push_optional_string_json(out, name.resolved());
    out.push_str(",\"fallback\":");
    push_optional_string_json(out, name.fallback());
    out.push('}');
}

fn push_statement_kind_json(out: &mut String, kind: &hir::HirStmtKind) {
    match kind {
        hir::HirStmtKind::Missing => {}
        hir::HirStmtKind::Expr { expr }
        | hir::HirStmtKind::Return { expr }
        | hir::HirStmtKind::Throw { expr }
        | hir::HirStmtKind::Break { expr }
        | hir::HirStmtKind::Continue { expr } => {
            out.push_str(",\"expr\":");
            push_optional_expr_id_json(out, *expr);
        }
        hir::HirStmtKind::Block { statements } => {
            out.push_str(",\"statements\":");
            push_stmt_ids_json(out, statements);
        }
        hir::HirStmtKind::Try {
            body,
            catches,
            finally_body,
        } => {
            out.push_str(",\"body\":");
            push_stmt_ids_json(out, body);
            out.push_str(",\"catches\":[");
            for (index, catch) in catches.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str("{\"types\":[");
                for (type_index, ty) in catch.types.iter().enumerate() {
                    if type_index > 0 {
                        out.push(',');
                    }
                    out.push('"');
                    out.push_str(&escape_json(ty));
                    out.push('"');
                }
                out.push_str("],\"variable\":");
                if let Some(variable) = &catch.variable {
                    out.push('"');
                    out.push_str(&escape_json(variable));
                    out.push('"');
                } else {
                    out.push_str("null");
                }
                out.push_str(",\"body\":");
                push_stmt_ids_json(out, &catch.body);
                out.push('}');
            }
            out.push_str("],\"finally_body\":");
            push_stmt_ids_json(out, finally_body);
        }
        hir::HirStmtKind::If {
            condition,
            body,
            elseifs,
            else_body,
        } => {
            out.push_str(",\"condition\":");
            push_optional_expr_id_json(out, *condition);
            out.push_str(",\"body\":");
            push_stmt_ids_json(out, body);
            out.push_str(",\"elseifs\":[");
            for (index, branch) in elseifs.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str("{\"condition\":");
                push_optional_expr_id_json(out, branch.condition);
                out.push_str(",\"body\":");
                push_stmt_ids_json(out, &branch.body);
                out.push('}');
            }
            out.push_str("],\"else_body\":");
            push_stmt_ids_json(out, else_body);
        }
        hir::HirStmtKind::While { condition, body }
        | hir::HirStmtKind::DoWhile { condition, body } => {
            out.push_str(",\"condition\":");
            push_optional_expr_id_json(out, *condition);
            out.push_str(",\"body\":");
            push_stmt_ids_json(out, body);
        }
        hir::HirStmtKind::Switch {
            condition,
            body,
            cases,
        } => {
            out.push_str(",\"condition\":");
            push_optional_expr_id_json(out, *condition);
            out.push_str(",\"body\":");
            push_stmt_ids_json(out, body);
            out.push_str(",\"cases\":[");
            for (index, case) in cases.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str("{\"condition\":");
                push_optional_expr_id_json(out, case.condition);
                out.push_str(",\"body\":");
                push_stmt_ids_json(out, &case.body);
                out.push_str(",\"is_default\":");
                out.push_str(if case.is_default { "true" } else { "false" });
                out.push('}');
            }
            out.push(']');
        }
        hir::HirStmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            out.push_str(",\"init\":");
            push_expr_ids_json(out, init);
            out.push_str(",\"condition\":");
            push_expr_ids_json(out, condition);
            out.push_str(",\"update\":");
            push_expr_ids_json(out, update);
            out.push_str(",\"body\":");
            push_stmt_ids_json(out, body);
        }
        hir::HirStmtKind::Declare { expressions, body } => {
            out.push_str(",\"expressions\":");
            push_expr_ids_json(out, expressions);
            out.push_str(",\"body\":");
            push_stmt_ids_json(out, body);
        }
        hir::HirStmtKind::Foreach {
            source,
            key_target,
            value_target,
            by_ref,
            body,
        } => {
            out.push_str(",\"source\":");
            push_optional_expr_id_json(out, *source);
            out.push_str(",\"key_target\":");
            push_optional_expr_id_json(out, *key_target);
            out.push_str(",\"value_target\":");
            push_optional_expr_id_json(out, *value_target);
            out.push_str(",\"by_ref\":");
            out.push_str(if *by_ref { "true" } else { "false" });
            out.push_str(",\"body\":");
            push_stmt_ids_json(out, body);
        }
        hir::HirStmtKind::Global { variables } | hir::HirStmtKind::Static { variables } => {
            out.push_str(",\"variables\":");
            push_expr_ids_json(out, variables);
        }
        hir::HirStmtKind::Unset { expressions } | hir::HirStmtKind::Echo { expressions } => {
            out.push_str(",\"expressions\":");
            push_expr_ids_json(out, expressions);
        }
        hir::HirStmtKind::InlineHtml { text } => {
            out.push_str(",\"text\":\"");
            out.push_str(&escape_json(text));
            out.push('"');
        }
        hir::HirStmtKind::Label { name } => {
            out.push_str(",\"name\":");
            push_optional_string_json(out, name.as_deref());
        }
        hir::HirStmtKind::Goto { label } => {
            out.push_str(",\"label\":");
            push_optional_string_json(out, label.as_deref());
        }
        hir::HirStmtKind::Unlowered { syntax_kind } => {
            out.push_str(",\"syntax_kind\":\"");
            out.push_str(&escape_json(syntax_kind));
            out.push('"');
        }
    }
}

fn push_expression_kind_json(out: &mut String, kind: &hir::HirExprKind) {
    match kind {
        hir::HirExprKind::Missing => {}
        hir::HirExprKind::Literal { text } => {
            out.push_str(",\"text\":\"");
            out.push_str(&escape_json(text));
            out.push('"');
        }
        hir::HirExprKind::Variable { name } => {
            out.push_str(",\"name\":\"");
            out.push_str(&escape_json(name));
            out.push('"');
        }
        hir::HirExprKind::Name { resolution } => {
            out.push_str(",\"source\":\"");
            out.push_str(&escape_json(resolution.source()));
            out.push_str("\",\"context\":\"");
            out.push_str(resolution.context());
            out.push_str("\",\"classification\":\"");
            out.push_str(resolution.classification());
            out.push_str("\",\"resolved\":");
            push_optional_string_json(out, resolution.resolved());
            out.push_str(",\"fallback\":");
            push_optional_string_json(out, resolution.fallback());
        }
        hir::HirExprKind::Array { elements } | hir::HirExprKind::List { elements } => {
            out.push_str(",\"elements\":");
            push_expr_ids_json(out, elements);
        }
        hir::HirExprKind::ArrayPair {
            key,
            value,
            unpack,
            by_ref,
        } => {
            out.push_str(",\"key\":");
            push_optional_expr_id_json(out, *key);
            out.push_str(",\"value\":");
            push_optional_expr_id_json(out, *value);
            out.push_str(",\"unpack\":");
            out.push_str(if *unpack { "true" } else { "false" });
            out.push_str(",\"by_ref\":");
            out.push_str(if *by_ref { "true" } else { "false" });
        }
        hir::HirExprKind::Unary { operator, expr }
        | hir::HirExprKind::Cast {
            kind: operator,
            expr,
        } => {
            out.push_str(",\"operator\":\"");
            out.push_str(&escape_json(operator));
            out.push_str("\",\"expr\":");
            push_optional_expr_id_json(out, *expr);
        }
        hir::HirExprKind::Binary {
            operator,
            left,
            right,
        }
        | hir::HirExprKind::Assign {
            operator,
            left,
            right,
        } => {
            out.push_str(",\"operator\":\"");
            out.push_str(&escape_json(operator));
            out.push_str("\",\"left\":");
            push_optional_expr_id_json(out, *left);
            out.push_str(",\"right\":");
            push_optional_expr_id_json(out, *right);
        }
        hir::HirExprKind::Ternary {
            condition,
            if_true,
            if_false,
        } => {
            out.push_str(",\"condition\":");
            push_optional_expr_id_json(out, *condition);
            out.push_str(",\"if_true\":");
            push_optional_expr_id_json(out, *if_true);
            out.push_str(",\"if_false\":");
            push_optional_expr_id_json(out, *if_false);
        }
        hir::HirExprKind::Call { callee, args } => {
            out.push_str(",\"callee\":");
            push_optional_expr_id_json(out, *callee);
            out.push_str(",\"args\":");
            push_call_args_json(out, args);
        }
        hir::HirExprKind::BuiltinCall { name, args } => {
            out.push_str(",\"name\":\"");
            out.push_str(&escape_json(name));
            out.push_str("\",\"args\":");
            push_call_args_json(out, args);
        }
        hir::HirExprKind::MethodCall {
            receiver,
            method,
            args,
            nullsafe,
        } => {
            out.push_str(",\"receiver\":");
            push_optional_expr_id_json(out, *receiver);
            out.push_str(",\"method\":");
            push_optional_expr_id_json(out, *method);
            out.push_str(",\"args\":");
            push_call_args_json(out, args);
            out.push_str(",\"nullsafe\":");
            out.push_str(if *nullsafe { "true" } else { "false" });
        }
        hir::HirExprKind::PropertyFetch {
            receiver,
            property,
            nullsafe,
        } => {
            out.push_str(",\"receiver\":");
            push_optional_expr_id_json(out, *receiver);
            out.push_str(",\"property\":");
            push_optional_expr_id_json(out, *property);
            out.push_str(",\"nullsafe\":");
            out.push_str(if *nullsafe { "true" } else { "false" });
        }
        hir::HirExprKind::StaticAccess { target, member } => {
            out.push_str(",\"target\":");
            push_optional_expr_id_json(out, *target);
            out.push_str(",\"member\":");
            push_optional_expr_id_json(out, *member);
        }
        hir::HirExprKind::DimFetch { receiver, dim } => {
            out.push_str(",\"receiver\":");
            push_optional_expr_id_json(out, *receiver);
            out.push_str(",\"dim\":");
            push_optional_expr_id_json(out, *dim);
        }
        hir::HirExprKind::Closure { body } => {
            out.push_str(",\"body_exprs\":");
            push_expr_ids_json(out, body);
        }
        hir::HirExprKind::ArrowFunction { expr }
        | hir::HirExprKind::Clone { expr }
        | hir::HirExprKind::YieldFrom { expr }
        | hir::HirExprKind::Exit { expr } => {
            out.push_str(",\"expr\":");
            push_optional_expr_id_json(out, *expr);
        }
        hir::HirExprKind::Eval {
            expr,
            deferred_effects,
        } => {
            out.push_str(",\"expr\":");
            push_optional_expr_id_json(out, *expr);
            out.push_str(",\"deferred_effects\":");
            push_deferred_effects_json(out, *deferred_effects);
        }
        hir::HirExprKind::New { class, args } => {
            out.push_str(",\"class\":");
            push_optional_expr_id_json(out, *class);
            out.push_str(",\"args\":");
            push_call_args_json(out, args);
        }
        hir::HirExprKind::CloneWith { expr, replacements } => {
            out.push_str(",\"expr\":");
            push_optional_expr_id_json(out, *expr);
            out.push_str(",\"replacements\":");
            push_expr_ids_json(out, replacements);
        }
        hir::HirExprKind::Match { subject, arms } => {
            out.push_str(",\"subject\":");
            push_optional_expr_id_json(out, *subject);
            out.push_str(",\"arms\":[");
            for (index, arm) in arms.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str("{\"conditions\":");
                push_expr_ids_json(out, &arm.conditions);
                out.push_str(",\"result\":");
                push_optional_expr_id_json(out, arm.result);
                out.push_str(",\"is_default\":");
                out.push_str(if arm.is_default { "true" } else { "false" });
                out.push('}');
            }
            out.push(']');
        }
        hir::HirExprKind::Yield { key, value } => {
            out.push_str(",\"key\":");
            push_optional_expr_id_json(out, *key);
            out.push_str(",\"value\":");
            push_optional_expr_id_json(out, *value);
        }
        hir::HirExprKind::Include {
            kind,
            expr,
            deferred_effects,
        } => {
            out.push_str(",\"include_kind\":\"");
            out.push_str(&escape_json(kind));
            out.push_str("\",\"expr\":");
            push_optional_expr_id_json(out, *expr);
            out.push_str(",\"deferred_effects\":");
            push_deferred_effects_json(out, *deferred_effects);
        }
        hir::HirExprKind::Pipe { input, callable } => {
            out.push_str(",\"input\":");
            push_optional_expr_id_json(out, *input);
            out.push_str(",\"callable\":");
            push_optional_expr_id_json(out, *callable);
        }
        hir::HirExprKind::FirstClassCallable { callee } => {
            out.push_str(",\"callee\":");
            push_optional_expr_id_json(out, *callee);
        }
        hir::HirExprKind::Unlowered { syntax_kind } => {
            out.push_str(",\"syntax_kind\":\"");
            out.push_str(&escape_json(syntax_kind));
            out.push('"');
        }
    }
}

fn push_deferred_effects_json(out: &mut String, effects: hir::DeferredEffects) {
    out.push_str("{\"may_load_file\":");
    out.push_str(if effects.may_load_file() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"may_define_symbols\":");
    out.push_str(if effects.may_define_symbols() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"may_execute_code\":");
    out.push_str(if effects.may_execute_code() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"scope_effect_deferred\":");
    out.push_str(if effects.scope_effect_deferred() {
        "true"
    } else {
        "false"
    });
    out.push('}');
}

fn push_expr_ids_json(out: &mut String, ids: &[hir::ExprId]) {
    out.push('[');
    for (index, id) in ids.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&id.raw().to_string());
    }
    out.push(']');
}

fn push_call_args_json(out: &mut String, args: &[hir::HirCallArg]) {
    out.push('[');
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str("\"value\":");
        out.push_str(&arg.value.raw().to_string());
        out.push_str(",\"name\":");
        if let Some(name) = &arg.name {
            out.push('"');
            out.push_str(&escape_json(name));
            out.push('"');
        } else {
            out.push_str("null");
        }
        out.push_str(",\"unpack\":");
        out.push_str(if arg.unpack { "true" } else { "false" });
        out.push('}');
    }
    out.push(']');
}

fn push_stmt_ids_json(out: &mut String, ids: &[hir::StmtId]) {
    out.push('[');
    for (index, id) in ids.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&id.raw().to_string());
    }
    out.push(']');
}

fn push_attribute_ids_json(out: &mut String, ids: &[hir::AttributeId]) {
    out.push('[');
    for (index, id) in ids.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&id.raw().to_string());
    }
    out.push(']');
}

fn push_optional_expr_id_json(out: &mut String, id: Option<hir::ExprId>) {
    if let Some(id) = id {
        out.push_str(&id.raw().to_string());
    } else {
        out.push_str("null");
    }
}

fn push_optional_string_json(out: &mut String, value: Option<&str>) {
    if let Some(value) = value {
        out.push('"');
        out.push_str(&escape_json(value));
        out.push('"');
    } else {
        out.push_str("null");
    }
}

fn push_type_kind_json(out: &mut String, module: &hir::HirModule, kind: &hir::HirTypeKind) {
    match kind {
        hir::HirTypeKind::Missing => out.push_str(",\"kind\":\"missing\""),
        hir::HirTypeKind::Unlowered => out.push_str(",\"kind\":\"unlowered\""),
        hir::HirTypeKind::Named { name, resolved } => {
            out.push_str(",\"kind\":\"named\",\"name\":\"");
            out.push_str(&escape_json(name.original()));
            out.push_str("\",\"resolved\":");
            if let Some(resolved) = resolved {
                out.push('"');
                out.push_str(&escape_json(&resolved.canonical(hir::NameKind::ClassLike)));
                out.push('"');
            } else {
                out.push_str("null");
            }
        }
        hir::HirTypeKind::Builtin(builtin) => {
            out.push_str(",\"kind\":\"builtin\",\"builtin\":\"");
            out.push_str(builtin.as_str());
            out.push('"');
        }
        hir::HirTypeKind::Nullable {
            inner,
            normalized_null,
        } => {
            out.push_str(",\"kind\":\"nullable\",\"inner\":");
            out.push_str(&inner.raw().to_string());
            out.push_str(",\"normalized_null\":");
            out.push_str(if *normalized_null { "true" } else { "false" });
        }
        hir::HirTypeKind::Union {
            members,
            normalized_from_nullable,
        } => {
            out.push_str(",\"kind\":\"union\",\"members\":");
            push_type_member_ids_json(out, members);
            out.push_str(",\"normalized_from_nullable\":");
            out.push_str(if *normalized_from_nullable {
                "true"
            } else {
                "false"
            });
        }
        hir::HirTypeKind::Intersection { members } => {
            out.push_str(",\"kind\":\"intersection\",\"members\":");
            push_type_member_ids_json(out, members);
        }
        hir::HirTypeKind::Dnf { members } => {
            out.push_str(",\"kind\":\"dnf\",\"members\":");
            push_type_member_ids_json(out, members);
        }
        hir::HirTypeKind::SelfType => out.push_str(",\"kind\":\"self\""),
        hir::HirTypeKind::ParentType => out.push_str(",\"kind\":\"parent\""),
        hir::HirTypeKind::StaticType => out.push_str(",\"kind\":\"static\""),
        hir::HirTypeKind::Mixed => out.push_str(",\"kind\":\"mixed\""),
        hir::HirTypeKind::Never => out.push_str(",\"kind\":\"never\""),
        hir::HirTypeKind::Void => out.push_str(",\"kind\":\"void\""),
        hir::HirTypeKind::Null => out.push_str(",\"kind\":\"null\""),
        hir::HirTypeKind::False => out.push_str(",\"kind\":\"false\""),
        hir::HirTypeKind::True => out.push_str(",\"kind\":\"true\""),
    }

    if matches!(
        kind,
        hir::HirTypeKind::Nullable { .. }
            | hir::HirTypeKind::Union { .. }
            | hir::HirTypeKind::Intersection { .. }
            | hir::HirTypeKind::Dnf { .. }
    ) {
        out.push_str(",\"display\":\"");
        out.push_str(&escape_json(&format_type(module, kind)));
        out.push('"');
    }
}

fn push_type_member_ids_json(out: &mut String, members: &[hir::TypeId]) {
    out.push('[');
    for (index, member) in members.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&member.raw().to_string());
    }
    out.push(']');
}

fn push_optional_type_id_json(out: &mut String, type_id: Option<hir::TypeId>) {
    if let Some(type_id) = type_id {
        out.push_str(&type_id.raw().to_string());
    } else {
        out.push_str("null");
    }
}

fn format_type(module: &hir::HirModule, kind: &hir::HirTypeKind) -> String {
    match kind {
        hir::HirTypeKind::Named { name, .. } => name.original().to_owned(),
        hir::HirTypeKind::Builtin(builtin) => builtin.as_str().to_owned(),
        hir::HirTypeKind::Nullable { inner, .. } => {
            format!("?{}", format_type(module, module.types()[*inner].kind()))
        }
        hir::HirTypeKind::Union { members, .. } | hir::HirTypeKind::Dnf { members } => members
            .iter()
            .map(|member| format_type(module, module.types()[*member].kind()))
            .collect::<Vec<_>>()
            .join("|"),
        hir::HirTypeKind::Intersection { members } => members
            .iter()
            .map(|member| format_type(module, module.types()[*member].kind()))
            .collect::<Vec<_>>()
            .join("&"),
        hir::HirTypeKind::SelfType => "self".to_owned(),
        hir::HirTypeKind::ParentType => "parent".to_owned(),
        hir::HirTypeKind::StaticType => "static".to_owned(),
        hir::HirTypeKind::Mixed => "mixed".to_owned(),
        hir::HirTypeKind::Never => "never".to_owned(),
        hir::HirTypeKind::Void => "void".to_owned(),
        hir::HirTypeKind::Null => "null".to_owned(),
        hir::HirTypeKind::False => "false".to_owned(),
        hir::HirTypeKind::True => "true".to_owned(),
        hir::HirTypeKind::Missing => "missing".to_owned(),
        hir::HirTypeKind::Unlowered => "unlowered".to_owned(),
    }
}

fn push_scope_json(out: &mut String, scopes: &scopes::ScopeArena, id: hir::ScopeId) {
    let scope = scopes.get(id).expect("scope child IDs are arena-backed");
    out.push_str("{\"id\":");
    out.push_str(&id.raw().to_string());
    out.push_str(",\"kind\":\"");
    out.push_str(scope.kind().as_str());
    out.push_str("\",\"name\":");
    if let Some(name) = scope.name() {
        out.push('"');
        out.push_str(&escape_json(name));
        out.push('"');
    } else {
        out.push_str("null");
    }
    out.push_str(",\"span\":");
    push_span_json(out, scope.span());
    out.push_str(",\"function_like\":");
    if let Some(function_like) = scope.function_like() {
        push_function_like_json(out, function_like);
    } else {
        out.push_str("null");
    }
    out.push_str(",\"globals\":");
    push_variable_bindings_json(out, scope.globals());
    out.push_str(",\"statics\":");
    out.push('[');
    for (index, binding) in scope.statics().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_variable_binding_json(out, binding.variable());
    }
    out.push(']');
    out.push_str(",\"children\":[");
    for (index, child) in scope.children().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_scope_json(out, scopes, *child);
    }
    out.push_str("]}");
}

fn push_function_like_json(out: &mut String, function_like: &scopes::FunctionLikeContext) {
    out.push_str("{\"kind\":\"");
    out.push_str(function_like.kind().as_str());
    out.push_str("\",\"parameters\":[");
    for (index, parameter) in function_like.parameters().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":\"");
        out.push_str(&escape_json(parameter.name()));
        out.push_str("\",\"by_ref\":");
        out.push_str(if parameter.is_by_ref() {
            "true"
        } else {
            "false"
        });
        out.push_str(",\"variadic\":");
        out.push_str(if parameter.is_variadic() {
            "true"
        } else {
            "false"
        });
        out.push_str(",\"span\":");
        push_span_json(out, parameter.span());
        out.push('}');
    }
    out.push_str("],\"captures\":[");
    for (index, capture) in function_like.captures().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":\"");
        out.push_str(&escape_json(capture.name()));
        out.push_str("\",\"mode\":\"");
        out.push_str(capture.mode().as_str());
        out.push_str("\",\"span\":");
        push_span_json(out, capture.span());
        out.push('}');
    }
    out.push_str("],\"capture_mode\":");
    if let Some(capture_mode) = function_like.capture_mode() {
        out.push('"');
        out.push_str(capture_mode.as_str());
        out.push('"');
    } else {
        out.push_str("null");
    }
    out.push('}');
}

fn push_variable_bindings_json(out: &mut String, bindings: &[scopes::VariableBinding]) {
    out.push('[');
    for (index, binding) in bindings.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_variable_binding_json(out, binding);
    }
    out.push(']');
}

fn push_variable_binding_json(out: &mut String, binding: &scopes::VariableBinding) {
    out.push_str("{\"name\":\"");
    out.push_str(&escape_json(binding.name()));
    out.push_str("\",\"span\":");
    push_span_json(out, binding.span());
    out.push('}');
}

fn push_scope_text(out: &mut String, scopes: &scopes::ScopeArena, id: hir::ScopeId, depth: usize) {
    let scope = scopes.get(id).expect("scope child IDs are arena-backed");
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(scope.kind().as_str());
    out.push('#');
    out.push_str(&id.raw().to_string());
    if let Some(name) = scope.name() {
        out.push(' ');
        out.push_str(name);
    }
    if let Some(function_like) = scope.function_like() {
        out.push_str(" params=[");
        for (index, parameter) in function_like.parameters().iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            if parameter.is_by_ref() {
                out.push('&');
            }
            if parameter.is_variadic() {
                out.push_str("...");
            }
            out.push_str(parameter.name());
        }
        out.push(']');
        if !function_like.captures().is_empty() {
            out.push_str(" captures=[");
            for (index, capture) in function_like.captures().iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                if capture.mode() == scopes::CaptureMode::ExplicitByReference {
                    out.push('&');
                }
                out.push_str(capture.name());
            }
            out.push(']');
        }
        if let Some(capture_mode) = function_like.capture_mode() {
            out.push_str(" capture_mode=");
            out.push_str(capture_mode.as_str());
        }
    }
    if !scope.globals().is_empty() {
        out.push_str(" globals=[");
        for (index, binding) in scope.globals().iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(binding.name());
        }
        out.push(']');
    }
    if !scope.statics().is_empty() {
        out.push_str(" statics=[");
        for (index, binding) in scope.statics().iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(binding.variable().name());
        }
        out.push(']');
    }
    out.push('\n');
    for child in scope.children() {
        push_scope_text(out, scopes, *child, depth + 1);
    }
}

fn push_span_json(out: &mut String, span: php_source::TextRange) {
    out.push_str("{\"start\":");
    out.push_str(&span.start().to_usize().to_string());
    out.push_str(",\"end\":");
    out.push_str(&span.end().to_usize().to_string());
    out.push('}');
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0C}' => escaped.push_str("\\f"),
            ch if ch <= '\u{1F}' => escaped.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticId, FrontendDatabase, TARGET_PHP_VERSION, analyze_source, hir};
    use php_source::TextRange;

    #[test]
    fn analyze_source_reports_minimal_module() {
        let result = analyze_source("<?php echo 1;\n");

        assert!(!result.has_errors());
        assert_eq!(result.module().root_kind(), "SOURCE_FILE");
        assert_eq!(result.module().source_bytes(), "<?php echo 1;\n".len());
        assert_eq!(result.module().module_id(), hir::ModuleId::from_raw(0));
        assert!(
            result
                .database()
                .module(result.module().module_id())
                .is_some()
        );
        assert!(result.semantic_diagnostics().is_empty());
    }

    #[test]
    fn json_contains_stable_initial_fields() {
        let result = analyze_source("<?php echo 1;\n");
        let json = result.to_json();

        assert!(json.contains("\"engine\":\"phrust-frontend\""));
        assert!(json.contains(TARGET_PHP_VERSION));
        assert!(json.contains("\"root_kind\":\"SOURCE_FILE\""));
        assert!(json.contains("\"module_id\":0"));
        assert!(json.contains("\"parser_diagnostics\":0"));
        assert!(json.contains("\"semantic_diagnostics\":[]"));
    }

    #[test]
    fn json_contains_namespace_summary() {
        let result = analyze_source("<?php namespace App; function run(): void {}");
        let json = result.to_json();

        assert!(json.contains("\"namespaces\":["));
        assert!(json.contains("\"name\":\"App\""));
        assert!(json.contains("\"form\":\"unbraced\""));
        assert!(json.contains("\"kind\":\"function\""));
    }

    #[test]
    fn scopes_text_contains_function_tree() {
        let result = analyze_source("<?php namespace App; function run($x): void {}");
        let text = result.to_scopes_text();

        assert!(text.contains("file#0"));
        assert!(text.contains("namespace#"));
        assert!(text.contains(" App"));
        assert!(text.contains("function#"));
        assert!(text.contains("run params=[$x]"));
    }

    #[test]
    fn typed_ids_allocate_and_index_arenas() {
        let mut arena = hir::Arena::<hir::HirExpr, hir::ExprId>::new();
        let id = arena.alloc(hir::HirExpr::missing());

        assert_eq!(id, hir::ExprId::from_raw(0));
        assert_eq!(format!("{id:?}"), "ExprId(0)");
        assert_eq!(arena[id].kind(), &hir::HirExprKind::Missing);
        assert_eq!(arena.get(id), Some(&hir::HirExpr::missing()));
    }

    #[test]
    fn source_map_maps_dummy_expr_to_span() {
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(hir::HirModule::new("SOURCE_FILE", 14));
        let expr_id = database
            .module_mut(module_id)
            .expect("module")
            .expressions_mut()
            .alloc(hir::HirExpr::new(hir::HirExprKind::Unlowered {
                syntax_kind: "EXPR".to_owned(),
            }));
        let range = TextRange::new(6, 10);

        database.source_map_mut().insert(expr_id, range);

        assert_eq!(database.source_map().span(expr_id), Some(range));
    }

    #[test]
    fn frontend_result_debug_includes_database() {
        let result = analyze_source("<?php echo 1;\n");
        let debug = format!("{result:?}");

        assert!(debug.contains("FrontendResult"));
        assert!(debug.contains("FrontendDatabase"));
        assert!(debug.contains("ModuleId(0)"));
    }

    #[test]
    fn constant_expression_candidates_are_marked_in_hir() {
        let result = analyze_source("<?php const A = 1; function f($x = A) {}");
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(!result.has_errors());
        assert!(module.const_exprs().iter().any(|(_, candidate)| {
            candidate.context() == hir::ConstExprContext::GlobalConstInitializer
                && candidate.kind() == hir::ConstExprKind::ScalarLiteral
                && candidate.is_allowed()
        }));
        assert!(module.const_exprs().iter().any(|(_, candidate)| {
            candidate.context() == hir::ConstExprContext::ParameterDefault
                && candidate.kind() == hir::ConstExprKind::Name
                && candidate.is_allowed()
        }));
        assert!(result.to_json().contains("\"const_exprs\":["));
    }

    #[test]
    fn invalid_constant_expression_forms_emit_diagnostics() {
        let result = analyze_source("<?php const BAD = strlen('x');");
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(result.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::InvalidConstExpr
                && diagnostic.phase().as_str() == "const_expression"
        }));
        assert!(module.const_exprs().iter().any(|(_, candidate)| {
            candidate.context() == hir::ConstExprContext::GlobalConstInitializer
                && candidate.kind() == hir::ConstExprKind::Disallowed
                && !candidate.is_allowed()
        }));
    }

    #[test]
    fn interpolated_string_constant_expression_is_rejected() {
        let result = analyze_source("<?php class C { const BAD = \"$name\"; }");
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(result.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::InvalidConstExpr
                && diagnostic.phase().as_str() == "const_expression"
        }));
        assert!(module.const_exprs().iter().any(|(_, candidate)| {
            candidate.context() == hir::ConstExprContext::ClassConstInitializer
                && candidate.kind() == hir::ConstExprKind::Disallowed
                && !candidate.is_allowed()
        }));
    }

    #[test]
    fn pure_constant_expression_folding_is_conservative() {
        let result = analyze_source(
            "<?php const A = 42; const B = -42; const C = 'ph' . \"rust\"; const D = __DIR__; class K { public const V = 1; } const E = K::V;",
        );
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(!result.has_errors());
        let folded: Vec<_> = module
            .const_exprs()
            .iter()
            .filter_map(|(_, candidate)| candidate.folded_value())
            .collect();

        assert!(folded.contains(&&hir::ConstValue::Int(42)));
        assert!(folded.contains(&&hir::ConstValue::Int(-42)));
        assert!(folded.contains(&&hir::ConstValue::String("phrust".to_owned())));
        assert!(module.const_exprs().iter().any(|(_, candidate)| {
            matches!(
                candidate.kind(),
                hir::ConstExprKind::ClassConstFetch | hir::ConstExprKind::ScalarLiteral
            ) && candidate.folded_value().is_none()
        }));
        assert!(result.to_json().contains("\"folded_value\":"));
    }

    #[test]
    fn attributes_are_lowered_with_targets_args_and_repetition() {
        let result = analyze_source(
            "<?php #[Attr('a'), Attr('b')] class C { #[Prop] public string $p = 'x'; public function m(#[Param(1)] $v): void {} }",
        );
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(!result.has_errors());
        assert!(module.attributes().iter().any(|(_, attribute)| {
            attribute.target() == hir::AttributeTarget::Class
                && attribute.name().context() == "attribute_class"
                && !attribute.args().is_empty()
        }));
        assert!(
            module
                .attributes()
                .iter()
                .any(|(_, attribute)| attribute.is_repeated_on_target())
        );
        assert!(
            module
                .attributes()
                .iter()
                .any(|(_, attribute)| attribute.target() == hir::AttributeTarget::Parameter)
        );
        assert!(result.to_json().contains("\"attributes\":["));
    }

    #[test]
    fn modifier_set_records_visibility_flags_and_hook_metadata() {
        let mut set = hir::ModifierSet::new();
        set.push(hir::ModifierOccurrence::new(
            hir::Modifier::Public,
            TextRange::new(1, 7),
        ));
        set.push(hir::ModifierOccurrence::new(
            hir::Modifier::PrivateSet,
            TextRange::new(8, 20),
        ));
        set.push(hir::ModifierOccurrence::new(
            hir::Modifier::Readonly,
            TextRange::new(21, 29),
        ));
        set.push(hir::ModifierOccurrence::new(
            hir::Modifier::HookRelated,
            TextRange::new(30, 40),
        ));

        assert_eq!(set.visibility(), Some(hir::Visibility::Public));
        assert_eq!(set.set_visibility(), Some(hir::Visibility::Private));
        assert!(set.is_readonly());
        assert!(set.is_hook_related());
    }

    #[test]
    fn modifier_validation_reports_invalid_combinations() {
        let result = analyze_source(
            "<?php abstract final class C {} class D { public private int $x; abstract private function f(): void; public static readonly int $bad; }",
        );

        assert!(result.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::IncompatibleModifiers
                && diagnostic.phase().as_str() == "modifier_validation"
        }));
        assert!(result.has_errors());
    }

    #[test]
    fn class_like_hir_records_names_members_and_anonymous_classes() {
        let result = analyze_source(
            "<?php namespace App; use Vendor\\Base; #[A] final class C extends Base implements I { use T; public const K = 1; public string $p; public function m(): void {} } $x = new class extends Base {}; enum E: string { case A = 'a'; }",
        );
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(!result.has_errors());
        let class = module
            .class_likes()
            .iter()
            .find_map(|(id, class_like)| {
                (class_like.name() == Some("C")).then_some((id, class_like))
            })
            .expect("class C");
        assert_eq!(class.1.kind(), hir::ClassLikeKind::Class);
        assert_eq!(
            class
                .1
                .fqn()
                .map(|name| name.canonical(hir::NameKind::ClassLike)),
            Some("app\\c".to_owned())
        );
        assert!(class.1.modifiers().is_final());
        assert_eq!(class.1.extends().len(), 1);
        assert_eq!(class.1.implements().len(), 1);
        assert_eq!(class.1.trait_uses().len(), 1);
        assert!(class.1.members().iter().any(|member| {
            member.kind() == hir::ClassLikeMemberKind::Method && member.name() == Some("m")
        }));
        assert!(result.database().source_map().span(class.0).is_some());
        assert!(class.1.attributes().len() == 1);
        assert!(module.class_likes().iter().any(|(_, class_like)| {
            class_like.kind() == hir::ClassLikeKind::AnonymousClass
                && class_like.anonymous_id() == Some("anonymous#0")
        }));
        assert!(module.class_likes().iter().any(|(_, class_like)| {
            class_like.kind() == hir::ClassLikeKind::Enum && class_like.backing_type().is_some()
        }));
        assert!(result.to_json().contains("\"class_likes\":["));
    }

    #[test]
    fn class_member_hir_records_methods_properties_and_constants() {
        let result = analyze_source(
            "<?php class C { public const string K = 'v'; protected int $p = 1; public function __construct(public string $name) {} abstract protected function m(): void; }",
        );
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(!result.has_errors());
        let class = module
            .class_likes()
            .iter()
            .find_map(|(id, class_like)| {
                (class_like.name() == Some("C")).then_some((id, class_like))
            })
            .expect("class C");

        assert_eq!(module.methods().iter().count(), 2);
        assert!(module.methods().iter().any(|(_, method)| {
            method.class_like() == class.0
                && method.name() == Some("__construct")
                && method.has_body()
                && method.signature_index().is_some()
        }));
        assert!(module.properties().iter().any(|(_, property)| {
            property.class_like() == class.0
                && property.type_id().is_some()
                && property.items().iter().any(|item| item.name() == "$p")
        }));
        assert!(module.class_consts().iter().any(|(_, class_const)| {
            class_const.class_like() == class.0
                && class_const.name() == Some("K")
                && class_const.type_id().is_some()
                && class_const.value().is_some()
        }));
        assert!(class.1.members().iter().any(|member| {
            member.kind() == hir::ClassLikeMemberKind::Method && member.id().is_some()
        }));

        let json = result.to_json();
        assert!(json.contains("\"methods\":["));
        assert!(json.contains("\"properties\":["));
        assert!(json.contains("\"class_consts\":["));
    }

    #[test]
    fn duplicate_class_members_are_diagnosed_by_family() {
        let result = analyze_source(
            "<?php class C { public function m() {} public function M() {} public $p; private $p; public const K = 1; private const K = 2; }",
        );

        assert!(result.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::DuplicateClassMember
                && diagnostic.message().contains("method")
        }));
        assert!(result.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::DuplicateClassMember
                && diagnostic.message().contains("property")
        }));
        assert!(result.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::DuplicateClassMember
                && diagnostic.message().contains("class constant")
        }));
    }

    #[test]
    fn enum_hir_records_unit_and_backed_cases() {
        let result = analyze_source(
            "<?php enum Status { case Draft; } enum Priority: string { #[High] case Low = 'low'; }",
        );
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(!result.has_errors());
        let enum_cases: Vec<_> = module.enum_cases().iter().collect();
        assert_eq!(enum_cases.len(), 2);
        assert!(
            enum_cases
                .iter()
                .any(|(_, enum_case)| enum_case.name() == Some("Draft")
                    && enum_case.value().is_none())
        );
        assert!(enum_cases.iter().any(|(id, enum_case)| {
            enum_case.name() == Some("Low")
                && enum_case.value().is_some()
                && !enum_case.attributes().is_empty()
                && result.database().source_map().span(*id).is_some()
        }));
        assert!(module.class_likes().iter().any(|(_, class_like)| {
            class_like.kind() == hir::ClassLikeKind::Enum
                && class_like.members().iter().any(|member| {
                    member.kind() == hir::ClassLikeMemberKind::EnumCase && member.id().is_some()
                })
        }));
        assert!(result.to_json().contains("\"enum_cases\":["));
    }

    #[test]
    fn enum_case_value_rules_are_diagnosed() {
        let result = analyze_source(
            "<?php enum BadUnit { case A = 'a'; } enum BadBacked: string { case A; } enum D { case A; case A; }",
        );

        assert!(
            result
                .semantic_diagnostics()
                .iter()
                .any(|diagnostic| { diagnostic.id() == DiagnosticId::EnumCaseValueOnUnitEnum })
        );
        assert!(result.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::EnumCaseMissingValueOnBackedEnum
        }));
        assert!(result.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.id() == DiagnosticId::DuplicateClassMember
                && diagnostic.message().contains("enum case")
        }));
    }

    #[test]
    fn trait_use_hir_records_alias_and_precedence_adaptations() {
        let result = analyze_source(
            "<?php namespace App; trait A { public function run() {} } trait B { public function run() {} } class C { use A, B { A::run insteadof B; B::run as private renamed; } }",
        );
        let module = result
            .database()
            .module(result.module().module_id())
            .expect("module");

        assert!(!result.has_errors());
        let trait_use = module
            .trait_uses()
            .iter()
            .map(|(_, trait_use)| trait_use)
            .find(|trait_use| trait_use.traits().len() == 2)
            .expect("trait use");

        assert_eq!(trait_use.adaptations().len(), 2);
        assert!(trait_use.adaptations().iter().any(|adaptation| {
            matches!(
                adaptation.kind(),
                hir::HirTraitAdaptationKind::Precedence { instead_of }
                    if instead_of.len() == 1
            ) && adaptation.method().trait_name().is_some()
                && adaptation.method().method() == "run"
        }));
        assert!(trait_use.adaptations().iter().any(|adaptation| {
            matches!(
                adaptation.kind(),
                hir::HirTraitAdaptationKind::Alias {
                    alias: Some(alias),
                    visibility: Some(visibility),
                } if alias == "renamed" && visibility == "private"
            )
        }));
        assert!(result.to_json().contains("\"trait_use_decls\":["));
    }

    #[test]
    fn invalid_trait_adaptation_shape_is_diagnosed() {
        let result = analyze_source(
            "<?php trait A { public function run() {} } class C { use A { run as; } }",
        );

        assert!(
            result
                .semantic_diagnostics()
                .iter()
                .any(|diagnostic| { diagnostic.id() == DiagnosticId::TraitAdaptationInvalidShape })
        );
    }
}
