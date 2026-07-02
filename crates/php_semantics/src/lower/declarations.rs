//! First declaration-collection pass.

use crate::FrontendDatabase;
use crate::diagnostics::{
    DiagnosticId, DiagnosticLabel, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity,
    SemanticDiagnostic,
};
use crate::hir::{
    FullyQualifiedName, HirDecl, HirDeclKind, HirNamespaceBlock, ModuleId, NamePart, NamespaceForm,
    NamespaceId, NamespaceName, ScopeId, SymbolId, TopLevelItem, TopLevelItemKind,
};
use crate::lower::context::LoweringContext;
use crate::lower::types::TypeLoweringScope;
use crate::scopes::{
    CaptureBinding, CaptureMode, FunctionLikeContext, FunctionLikeKind, ParameterBinding,
    ScopeKind, StaticLocalBinding, VariableBinding,
};
use crate::symbols::declarations::{DeclarationEntry, DeclarationKind, DuplicateDeclaration};
use crate::symbols::imports::{DuplicateImportAlias, collect_use_imports};
use crate::symbols::resolution::{NameResolver, ResolveContext, ResolvedNameRecord};
use php_ast::{
    ArrowFunctionExpr, AstNode, AstToken, BlockStmt, ClassDecl, ClosureExpr, ConstDecl, EnumDecl,
    FunctionDecl, GlobalStmt, InlineHtmlStmt, InterfaceDecl, MethodDecl, Name, NamespaceDecl,
    Param, ParamList, StatementList, StaticStmt, TokenView, TraitDecl, UseDecl, descendant_nodes,
    descendant_tokens, syntax_child_nodes, syntax_child_tokens,
};
use php_source::TextRange;
use php_syntax::{SyntaxNode, SyntaxToken};

/// Collects namespace blocks and coarse top-level items for one source file.
pub fn collect_module_declarations(
    source_file: php_ast::SourceFile<'_>,
    database: &mut FrontendDatabase,
    module_id: ModuleId,
) -> Vec<SemanticDiagnostic> {
    let mut collector = ModuleDeclarationCollector::new(database, module_id);
    collector.collect_source_file(source_file.syntax());
    let mut diagnostics = collector.finish();
    diagnostics.extend(crate::lower::declares::collect_declare_directives(
        source_file.syntax(),
        database,
        module_id,
    ));
    diagnostics.extend(crate::checks::modifiers::check_source_file(
        source_file.syntax(),
    ));
    diagnostics.extend(crate::scopes::control_context::check_source_file(
        source_file.syntax(),
    ));
    diagnostics.extend(crate::checks::class_context::check_source_file(
        source_file.syntax(),
    ));
    diagnostics
}

struct ModuleDeclarationCollector<'db> {
    database: &'db mut FrontendDatabase,
    module_id: ModuleId,
    context: LoweringContext,
    pending_blocks: Vec<HirNamespaceBlock>,
    namespace_style: Option<NamespaceForm>,
    current_unbraced: Option<usize>,
    file_scope: Option<ScopeId>,
    saw_explicit_namespace: bool,
    saw_non_declare_php_before_namespace: bool,
}

impl<'db> ModuleDeclarationCollector<'db> {
    fn new(database: &'db mut FrontendDatabase, module_id: ModuleId) -> Self {
        Self {
            database,
            module_id,
            context: LoweringContext::new(),
            pending_blocks: Vec::new(),
            namespace_style: None,
            current_unbraced: None,
            file_scope: None,
            saw_explicit_namespace: false,
            saw_non_declare_php_before_namespace: false,
        }
    }

    fn collect_source_file(&mut self, source_file: &SyntaxNode) {
        let file_scope = self.alloc_root_scope(source_file.text_range());
        self.file_scope = Some(file_scope);
        self.ensure_global_block(source_file.text_range());
        for child in syntax_child_nodes(source_file) {
            if InlineHtmlStmt::cast(child).is_some() {
                self.collect_hir_in_node_with_scope(
                    child,
                    TypeLoweringScope::new(None, Default::default()),
                );
                self.push_global_item(TopLevelItem::new(
                    TopLevelItemKind::InlineHtml,
                    child.text_range(),
                ));
            } else if child.kind().name() == "PHP_BLOCK" {
                self.collect_php_block(child);
            }
        }
    }

    fn collect_php_block(&mut self, php_block: &SyntaxNode) {
        for child in syntax_child_nodes(php_block) {
            if StatementList::cast(child).is_some() {
                self.collect_statement_list(child);
            }
        }
    }

    fn collect_statement_list(&mut self, statement_list: &SyntaxNode) {
        for child in syntax_child_nodes(statement_list) {
            if let Some(namespace) = NamespaceDecl::cast(child) {
                self.collect_namespace(namespace);
            } else if let Some(item) = top_level_item(child) {
                self.collect_top_level_node(child, item);
            }
        }
    }

    fn collect_namespace(&mut self, namespace: NamespaceDecl<'_>) {
        if self.saw_non_declare_php_before_namespace {
            self.context.reporter_mut().error(
                DiagnosticId::NamespaceMustBeFirstStatement,
                DiagnosticPhase::DeclarationCollection,
                "namespace declaration must be the first PHP statement or follow declare",
                Some(namespace.text_range()),
            );
        }

        let form = namespace_form(namespace);
        if let Some(previous) = self.namespace_style {
            if previous != form {
                self.context.reporter_mut().error(
                    DiagnosticId::MixedNamespaceDeclarations,
                    DiagnosticPhase::DeclarationCollection,
                    "cannot mix braced and unbraced namespace declarations",
                    Some(namespace.text_range()),
                );
            }
        } else {
            self.namespace_style = Some(form);
        }
        self.saw_explicit_namespace = true;

        let name = namespace_name(namespace);
        let mut block = HirNamespaceBlock::new(name, form, namespace.text_range());
        let namespace_scope = self.alloc_namespace_scope(block.name(), namespace.text_range());
        block.set_scope_id(namespace_scope);
        if form == NamespaceForm::Braced {
            for body in syntax_child_nodes(namespace.syntax())
                .filter(|node| BlockStmt::cast(node).is_some())
            {
                for child in syntax_child_nodes(body) {
                    if let Some(item) = top_level_item(child) {
                        let namespace_name = block.name().cloned();
                        self.collect_declarations_in_node(child, item.kind(), namespace_name);
                        self.collect_scopes_in_node(child, namespace_scope);
                        self.collect_types_in_node(child, &block);
                        self.collect_hir_in_node(child, &block);
                        self.collect_signatures_in_node(child, &block);
                        self.collect_const_expr_in_node(child);
                        self.collect_attributes_in_node(child, &block);
                        self.collect_class_likes_in_node(child, &block);
                        collect_top_level_node_effects(
                            child,
                            item,
                            &mut block,
                            self.context.reporter_mut(),
                        );
                    }
                }
            }
            self.pending_blocks.push(block);
            self.current_unbraced = None;
        } else {
            self.pending_blocks.push(block);
            self.current_unbraced = Some(self.pending_blocks.len() - 1);
        }
    }

    fn collect_top_level_node(&mut self, node: &SyntaxNode, item: TopLevelItem) {
        if !self.saw_explicit_namespace && item.kind() != TopLevelItemKind::Declare {
            self.saw_non_declare_php_before_namespace = true;
        }

        if self.namespace_style == Some(NamespaceForm::Braced) && self.saw_explicit_namespace {
            self.context.reporter_mut().error(
                DiagnosticId::NamespaceMustBeFirstStatement,
                DiagnosticPhase::DeclarationCollection,
                "top-level PHP code must be inside a namespace block after braced namespaces",
                Some(item.span()),
            );
        }

        if let Some(index) = self.current_unbraced {
            let namespace_name = self.pending_blocks[index].name().cloned();
            let namespace_scope = self.pending_blocks[index]
                .scope_id()
                .expect("namespace scope assigned when block is created");
            self.collect_declarations_in_node(node, item.kind(), namespace_name);
            self.collect_scopes_in_node(node, namespace_scope);
            let type_scope = TypeLoweringScope::new(
                self.pending_blocks[index].name().cloned(),
                self.pending_blocks[index].imports().clone(),
            );
            self.collect_types_in_node_with_scope(node, type_scope.clone());
            self.collect_hir_in_node_with_scope(node, type_scope.clone());
            self.collect_signatures_in_node_with_scope(node, type_scope.clone());
            self.collect_const_expr_in_node(node);
            self.collect_attributes_in_node_with_scope(node, type_scope.clone());
            self.collect_class_likes_in_node_with_scope(node, type_scope);
            collect_top_level_node_effects(
                node,
                item,
                &mut self.pending_blocks[index],
                self.context.reporter_mut(),
            );
        } else {
            self.push_global_node(node, item);
        }
    }

    fn push_global_item(&mut self, item: TopLevelItem) {
        self.ensure_global_block(item.span());
        self.pending_blocks[0].push_item(item);
    }

    fn push_global_node(&mut self, node: &SyntaxNode, item: TopLevelItem) {
        self.ensure_global_block(item.span());
        let namespace_name = self.pending_blocks[0].name().cloned();
        let namespace_scope = self.pending_blocks[0]
            .scope_id()
            .expect("global namespace scope assigned when block is created");
        self.collect_declarations_in_node(node, item.kind(), namespace_name);
        self.collect_scopes_in_node(node, namespace_scope);
        let type_scope = TypeLoweringScope::new(
            self.pending_blocks[0].name().cloned(),
            self.pending_blocks[0].imports().clone(),
        );
        self.collect_types_in_node_with_scope(node, type_scope.clone());
        self.collect_hir_in_node_with_scope(node, type_scope.clone());
        self.collect_signatures_in_node_with_scope(node, type_scope.clone());
        self.collect_const_expr_in_node(node);
        self.collect_attributes_in_node_with_scope(node, type_scope.clone());
        self.collect_class_likes_in_node_with_scope(node, type_scope);
        collect_top_level_node_effects(
            node,
            item,
            &mut self.pending_blocks[0],
            self.context.reporter_mut(),
        );
    }

    fn collect_declarations_in_node(
        &mut self,
        node: &SyntaxNode,
        item_kind: TopLevelItemKind,
        namespace_name: Option<NamespaceName>,
    ) {
        match item_kind {
            TopLevelItemKind::Const => {
                if let Some(const_decl) = ConstDecl::cast(node) {
                    for (name, span) in const_decl_names(const_decl) {
                        self.register_declaration(
                            DeclarationKind::Constant,
                            name,
                            span,
                            namespace_name.as_ref(),
                        );
                    }
                }
            }
            TopLevelItemKind::Function => {
                if let Some(function) = FunctionDecl::cast(node)
                    && let Some((name, span)) = named_decl_name(function.syntax())
                {
                    self.register_declaration(
                        DeclarationKind::Function,
                        name,
                        span,
                        namespace_name.as_ref(),
                    );
                    for child in syntax_child_nodes(function.syntax()) {
                        if BlockStmt::cast(child).is_some() {
                            self.collect_conditional_declarations(child, namespace_name.as_ref());
                        }
                    }
                }
            }
            TopLevelItemKind::Class
            | TopLevelItemKind::Interface
            | TopLevelItemKind::Trait
            | TopLevelItemKind::Enum => {
                if let Some((name, span)) = named_decl_name(node) {
                    self.register_declaration(
                        declaration_kind_for_node(node),
                        name,
                        span,
                        namespace_name.as_ref(),
                    );
                }
            }
            TopLevelItemKind::Statement => {
                self.collect_conditional_declarations(node, namespace_name.as_ref());
            }
            _ => {}
        }
    }

    fn collect_types_in_node(&mut self, node: &SyntaxNode, block: &HirNamespaceBlock) {
        let scope = TypeLoweringScope::new(block.name().cloned(), block.imports().clone());
        self.collect_types_in_node_with_scope(node, scope);
    }

    fn collect_types_in_node_with_scope(&mut self, node: &SyntaxNode, scope: TypeLoweringScope) {
        crate::lower::types::collect_types_in_node(
            node,
            &mut *self.database,
            self.module_id,
            self.context.reporter_mut(),
            scope,
        );
    }

    fn collect_signatures_in_node(&mut self, node: &SyntaxNode, block: &HirNamespaceBlock) {
        let scope = TypeLoweringScope::new(block.name().cloned(), block.imports().clone());
        self.collect_signatures_in_node_with_scope(node, scope);
    }

    fn collect_signatures_in_node_with_scope(
        &mut self,
        node: &SyntaxNode,
        scope: TypeLoweringScope,
    ) {
        crate::lower::signatures::collect_signatures_in_node(
            node,
            &mut *self.database,
            self.module_id,
            self.context.reporter_mut(),
            scope,
        );
    }

    fn collect_hir_in_node(&mut self, node: &SyntaxNode, block: &HirNamespaceBlock) {
        let scope = TypeLoweringScope::new(block.name().cloned(), block.imports().clone());
        self.collect_hir_in_node_with_scope(node, scope);
    }

    fn collect_hir_in_node_with_scope(&mut self, node: &SyntaxNode, scope: TypeLoweringScope) {
        crate::lower::hir::collect_hir_in_node(
            node,
            &mut *self.database,
            self.module_id,
            self.context.reporter_mut(),
            scope,
        );
    }

    fn collect_const_expr_in_node(&mut self, node: &SyntaxNode) {
        crate::lower::const_expr::collect_const_expr_in_node(
            node,
            &mut *self.database,
            self.module_id,
            self.context.reporter_mut(),
        );
    }

    fn collect_attributes_in_node(&mut self, node: &SyntaxNode, block: &HirNamespaceBlock) {
        let scope = TypeLoweringScope::new(block.name().cloned(), block.imports().clone());
        self.collect_attributes_in_node_with_scope(node, scope);
    }

    fn collect_attributes_in_node_with_scope(
        &mut self,
        node: &SyntaxNode,
        scope: TypeLoweringScope,
    ) {
        crate::lower::attributes::collect_attributes_in_node(
            node,
            &mut *self.database,
            self.module_id,
            scope,
        );
    }

    fn collect_class_likes_in_node(&mut self, node: &SyntaxNode, block: &HirNamespaceBlock) {
        let scope = TypeLoweringScope::new(block.name().cloned(), block.imports().clone());
        self.collect_class_likes_in_node_with_scope(node, scope);
    }

    fn collect_class_likes_in_node_with_scope(
        &mut self,
        node: &SyntaxNode,
        scope: TypeLoweringScope,
    ) {
        crate::lower::class_likes::collect_class_likes_in_node(
            node,
            &mut *self.database,
            self.module_id,
            self.context.reporter_mut(),
            scope,
        );
    }

    fn collect_conditional_declarations(
        &mut self,
        node: &SyntaxNode,
        namespace_name: Option<&NamespaceName>,
    ) {
        for function in descendant_nodes::<FunctionDecl<'_>>(node) {
            if let Some((name, span)) = named_decl_name(function.syntax()) {
                self.register_declaration(
                    DeclarationKind::ConditionalFunction,
                    name,
                    span,
                    namespace_name,
                );
            }
        }

        for class_decl in descendant_nodes::<ClassDecl<'_>>(node) {
            if let Some((name, span)) = named_decl_name(class_decl.syntax()) {
                self.register_declaration(
                    DeclarationKind::ConditionalClassLike,
                    name,
                    span,
                    namespace_name,
                );
            }
        }
        for interface_decl in descendant_nodes::<InterfaceDecl<'_>>(node) {
            if let Some((name, span)) = named_decl_name(interface_decl.syntax()) {
                self.register_declaration(
                    DeclarationKind::ConditionalClassLike,
                    name,
                    span,
                    namespace_name,
                );
            }
        }
        for trait_decl in descendant_nodes::<TraitDecl<'_>>(node) {
            if let Some((name, span)) = named_decl_name(trait_decl.syntax()) {
                self.register_declaration(
                    DeclarationKind::ConditionalClassLike,
                    name,
                    span,
                    namespace_name,
                );
            }
        }
        for enum_decl in descendant_nodes::<EnumDecl<'_>>(node) {
            if let Some((name, span)) = named_decl_name(enum_decl.syntax()) {
                self.register_declaration(
                    DeclarationKind::ConditionalClassLike,
                    name,
                    span,
                    namespace_name,
                );
            }
        }
    }

    fn register_declaration(
        &mut self,
        kind: DeclarationKind,
        name: String,
        span: TextRange,
        namespace_name: Option<&NamespaceName>,
    ) {
        let fqn = declaration_fqn(namespace_name, &name);
        let decl_id = {
            let module = self
                .database
                .module_mut(self.module_id)
                .expect("module allocated before declaration collection");
            module.declarations_mut().alloc(HirDecl::new(
                hir_decl_kind_for_declaration(kind),
                Some(crate::hir::HirName::new(&name)),
            ))
        };
        let symbol_id = SymbolId::from_raw(decl_id.raw());
        self.database.source_map_mut().insert(decl_id, span);
        self.database.source_map_mut().insert(symbol_id, span);

        let duplicate = {
            let module = self
                .database
                .module_mut(self.module_id)
                .expect("module allocated before declaration collection");
            module
                .declaration_table_mut()
                .insert(DeclarationEntry::new(
                    decl_id, symbol_id, kind, name, fqn, span,
                ))
                .err()
        };
        if let Some(duplicate) = duplicate {
            report_duplicate_declaration(self.context.reporter_mut(), duplicate);
        }
    }

    fn ensure_global_block(&mut self, span: TextRange) {
        if self.pending_blocks.is_empty() {
            let mut block = HirNamespaceBlock::new(None, NamespaceForm::Global, span);
            let scope_id = self.alloc_namespace_scope(block.name(), span);
            block.set_scope_id(scope_id);
            self.pending_blocks.push(block);
        }
    }

    fn alloc_root_scope(&mut self, span: TextRange) -> ScopeId {
        let scope_id = {
            let module = self
                .database
                .module_mut(self.module_id)
                .expect("module allocated before scope collection");
            module.scopes_mut().alloc_root(ScopeKind::File, None, span)
        };
        self.database.source_map_mut().insert(scope_id, span);
        scope_id
    }

    fn alloc_namespace_scope(&mut self, name: Option<&NamespaceName>, span: TextRange) -> ScopeId {
        let file_scope = self
            .file_scope
            .expect("file scope allocated before namespace collection");
        let scope_id = {
            let module = self
                .database
                .module_mut(self.module_id)
                .expect("module allocated before scope collection");
            module.scopes_mut().alloc_child(
                file_scope,
                ScopeKind::Namespace,
                name.map(|name| name.text().to_owned()),
                span,
            )
        };
        self.database.source_map_mut().insert(scope_id, span);
        scope_id
    }

    fn alloc_child_scope(
        &mut self,
        parent: ScopeId,
        kind: ScopeKind,
        name: Option<String>,
        span: TextRange,
        function_like: Option<FunctionLikeContext>,
    ) -> ScopeId {
        let scope_id = {
            let module = self
                .database
                .module_mut(self.module_id)
                .expect("module allocated before scope collection");
            let scope_id = module.scopes_mut().alloc_child(parent, kind, name, span);
            if let Some(function_like) = function_like {
                module
                    .scopes_mut()
                    .get_mut(scope_id)
                    .expect("scope allocated above")
                    .set_function_like(function_like);
            }
            scope_id
        };
        self.database.source_map_mut().insert(scope_id, span);
        scope_id
    }

    fn collect_scopes_in_node(&mut self, node: &SyntaxNode, parent_scope: ScopeId) {
        if let Some(function) = FunctionDecl::cast(node) {
            let parameters = collect_parameters(function.syntax());
            let name = named_decl_name(function.syntax()).map(|(name, _)| name);
            let function_scope = self.alloc_child_scope(
                parent_scope,
                ScopeKind::Function,
                name,
                function.text_range(),
                Some(FunctionLikeContext::new(
                    FunctionLikeKind::Function,
                    parameters,
                )),
            );
            self.collect_scope_children(function.syntax(), function_scope);
        } else if let Some(class_decl) = ClassDecl::cast(node) {
            if !class_decl.is_anonymous() {
                let class_scope = self.alloc_child_scope(
                    parent_scope,
                    ScopeKind::Class,
                    named_decl_name(class_decl.syntax()).map(|(name, _)| name),
                    class_decl.text_range(),
                    None,
                );
                self.collect_scope_children(class_decl.syntax(), class_scope);
            } else {
                self.collect_scope_children(class_decl.syntax(), parent_scope);
            }
        } else if let Some(interface_decl) = InterfaceDecl::cast(node) {
            let interface_scope = self.alloc_child_scope(
                parent_scope,
                ScopeKind::Interface,
                named_decl_name(interface_decl.syntax()).map(|(name, _)| name),
                interface_decl.text_range(),
                None,
            );
            self.collect_scope_children(interface_decl.syntax(), interface_scope);
        } else if let Some(trait_decl) = TraitDecl::cast(node) {
            let trait_scope = self.alloc_child_scope(
                parent_scope,
                ScopeKind::Trait,
                named_decl_name(trait_decl.syntax()).map(|(name, _)| name),
                trait_decl.text_range(),
                None,
            );
            self.collect_scope_children(trait_decl.syntax(), trait_scope);
        } else if let Some(enum_decl) = EnumDecl::cast(node) {
            let enum_scope = self.alloc_child_scope(
                parent_scope,
                ScopeKind::Enum,
                named_decl_name(enum_decl.syntax()).map(|(name, _)| name),
                enum_decl.text_range(),
                None,
            );
            self.collect_scope_children(enum_decl.syntax(), enum_scope);
        } else if let Some(method) = MethodDecl::cast(node) {
            let parameters = collect_parameters(method.syntax());
            let method_scope = self.alloc_child_scope(
                parent_scope,
                ScopeKind::Method,
                named_decl_name(method.syntax()).map(|(name, _)| name),
                method.text_range(),
                Some(FunctionLikeContext::new(
                    FunctionLikeKind::Method,
                    parameters,
                )),
            );
            self.collect_scope_children(method.syntax(), method_scope);
        } else if let Some(closure) = ClosureExpr::cast(node) {
            let parameters = collect_parameters(closure.syntax());
            let captures = collect_closure_captures(closure.syntax());
            let closure_scope = self.alloc_child_scope(
                parent_scope,
                ScopeKind::Closure,
                None,
                closure.text_range(),
                Some(FunctionLikeContext::closure(parameters, captures)),
            );
            self.collect_scope_children(closure.syntax(), closure_scope);
        } else if let Some(arrow) = ArrowFunctionExpr::cast(node) {
            let parameters = collect_parameters(arrow.syntax());
            let arrow_scope = self.alloc_child_scope(
                parent_scope,
                ScopeKind::ArrowFunction,
                None,
                arrow.text_range(),
                Some(FunctionLikeContext::arrow(parameters)),
            );
            self.collect_scope_children(arrow.syntax(), arrow_scope);
        } else if let Some(global_stmt) = GlobalStmt::cast(node) {
            self.record_global_statement(parent_scope, global_stmt);
        } else if let Some(static_stmt) = StaticStmt::cast(node) {
            self.record_static_statement(parent_scope, static_stmt);
        } else {
            self.collect_scope_children(node, parent_scope);
        }
    }

    fn collect_scope_children(&mut self, node: &SyntaxNode, parent_scope: ScopeId) {
        for child in syntax_child_nodes(node) {
            if ParamList::cast(child).is_none() {
                self.collect_scopes_in_node(child, parent_scope);
            }
        }
    }

    fn record_global_statement(&mut self, parent_scope: ScopeId, global_stmt: GlobalStmt<'_>) {
        let bindings = collect_statement_variables(global_stmt.syntax());
        let module = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before scope collection");
        let scope = module
            .scopes_mut()
            .get_mut(parent_scope)
            .expect("current scope is allocated");
        for binding in bindings {
            scope.push_global(binding);
        }
    }

    fn record_static_statement(&mut self, parent_scope: ScopeId, static_stmt: StaticStmt<'_>) {
        let bindings = collect_static_variables(static_stmt.syntax());
        let module = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before scope collection");
        let scope = module
            .scopes_mut()
            .get_mut(parent_scope)
            .expect("current scope is allocated");
        for binding in bindings {
            scope.push_static(StaticLocalBinding::new(binding));
        }
    }

    fn finish(self) -> Vec<SemanticDiagnostic> {
        let mut namespace_spans = Vec::<(NamespaceId, TextRange)>::new();
        let module = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before declaration collection");
        for block in self.pending_blocks {
            let span = block.span();
            let id = module.namespaces_mut().alloc(block);
            namespace_spans.push((id, span));
        }
        for (id, span) in namespace_spans {
            self.database.source_map_mut().insert(id, span);
        }
        self.context.into_diagnostics()
    }
}

fn namespace_form(namespace: NamespaceDecl<'_>) -> NamespaceForm {
    if syntax_child_nodes(namespace.syntax()).any(|node| BlockStmt::cast(node).is_some()) {
        NamespaceForm::Braced
    } else {
        NamespaceForm::Unbraced
    }
}

fn namespace_name(namespace: NamespaceDecl<'_>) -> Option<NamespaceName> {
    syntax_child_nodes(namespace.syntax())
        .find_map(Name::cast)
        .map(|name| {
            let mut text = String::new();
            for token in descendant_tokens::<php_ast::TokenView<'_>>(name.syntax()) {
                if !token.kind().is_trivia() {
                    text.push_str(token.text());
                }
            }
            NamespaceName::new(text)
        })
}

fn const_decl_names(const_decl: ConstDecl<'_>) -> Vec<(String, TextRange)> {
    syntax_child_tokens(const_decl.syntax())
        .filter(|token| token.kind().name() == "T_STRING")
        .map(token_name)
        .collect()
}

fn named_decl_name(node: &SyntaxNode) -> Option<(String, TextRange)> {
    syntax_child_tokens(node)
        .find(|token| token.kind().name() == "T_STRING")
        .map(token_name)
}

fn token_name(token: &SyntaxToken) -> (String, TextRange) {
    (token.text().to_owned(), token.text_range())
}

fn collect_parameters(node: &SyntaxNode) -> Vec<ParameterBinding> {
    if let Some(param_list) = syntax_child_nodes(node).find_map(|child| {
        let param_list = ParamList::cast(child)?;
        (!param_list_is_closure_use(param_list)).then_some(param_list)
    }) {
        let parameters: Vec<_> = syntax_child_nodes(param_list.syntax())
            .filter_map(Param::cast)
            .filter_map(collect_param_binding)
            .collect();
        if !parameters.is_empty() {
            return parameters;
        }
    }

    collect_fallback_parameters(node)
}

fn collect_param_binding(param: Param<'_>) -> Option<ParameterBinding> {
    let variable = descendant_tokens::<TokenView<'_>>(param.syntax())
        .find(|token| token.kind().name() == "T_VARIABLE")?;
    let by_ref = descendant_tokens::<TokenView<'_>>(param.syntax()).any(|token| {
        matches!(
            token.kind().name().as_str(),
            "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG" | "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG"
        )
    });
    let variadic =
        descendant_tokens::<TokenView<'_>>(param.syntax()).any(|token| token.text() == "...");
    Some(ParameterBinding::new(
        variable.text(),
        by_ref,
        variadic,
        variable.text_range(),
    ))
}

fn collect_closure_captures(node: &SyntaxNode) -> Vec<CaptureBinding> {
    let Some(capture_list) = syntax_child_nodes(node).find_map(|child| {
        let param_list = ParamList::cast(child)?;
        param_list_is_closure_use(param_list).then_some(param_list)
    }) else {
        return Vec::new();
    };

    syntax_child_nodes(capture_list.syntax())
        .filter_map(Param::cast)
        .filter_map(|param| {
            let variable = descendant_tokens::<TokenView<'_>>(param.syntax())
                .find(|token| token.kind().name() == "T_VARIABLE")?;
            let by_ref = descendant_tokens::<TokenView<'_>>(param.syntax()).any(|token| {
                matches!(
                    token.kind().name().as_str(),
                    "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG"
                        | "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG"
                )
            });
            Some(CaptureBinding::new(
                variable.text(),
                if by_ref {
                    CaptureMode::ExplicitByReference
                } else {
                    CaptureMode::ExplicitByValue
                },
                variable.text_range(),
            ))
        })
        .collect()
}

fn param_list_is_closure_use(param_list: ParamList<'_>) -> bool {
    syntax_child_tokens(param_list.syntax())
        .find(|token| !token.kind().is_trivia())
        .is_some_and(|token| token.kind().name() == "T_USE")
}

fn collect_fallback_parameters(node: &SyntaxNode) -> Vec<ParameterBinding> {
    let mut parameters = Vec::new();
    let mut in_parameters = false;
    let mut depth = 0usize;
    let mut by_ref = false;
    let mut variadic = false;

    for token in syntax_child_tokens(node).filter(|token| !token.kind().is_trivia()) {
        match token.text() {
            "(" if !in_parameters => {
                in_parameters = true;
                depth = 1;
            }
            "(" if in_parameters => depth += 1,
            ")" if in_parameters => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            "," if in_parameters && depth == 1 => {
                by_ref = false;
                variadic = false;
            }
            "..." if in_parameters && depth == 1 => variadic = true,
            _ if in_parameters
                && depth == 1
                && matches!(
                    token.kind().name().as_str(),
                    "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG"
                        | "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG"
                ) =>
            {
                by_ref = true;
            }
            _ if in_parameters && depth == 1 && token.kind().name() == "T_VARIABLE" => {
                parameters.push(ParameterBinding::new(
                    token.text(),
                    by_ref,
                    variadic,
                    token.text_range(),
                ));
                by_ref = false;
                variadic = false;
            }
            _ => {}
        }
    }

    parameters
}

fn collect_statement_variables(node: &SyntaxNode) -> Vec<VariableBinding> {
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| token.kind().name() == "T_VARIABLE")
        .map(|token| VariableBinding::new(token.text(), token.text_range()))
        .collect()
}

fn collect_static_variables(node: &SyntaxNode) -> Vec<VariableBinding> {
    let mut variables = Vec::new();
    let mut expect_variable = false;
    for token in descendant_tokens::<TokenView<'_>>(node).filter(|token| !token.kind().is_trivia())
    {
        match token.kind().name().as_str() {
            "T_STATIC" => expect_variable = true,
            "T_VARIABLE" if expect_variable => {
                variables.push(VariableBinding::new(token.text(), token.text_range()));
                expect_variable = false;
            }
            _ if token.text() == "," => expect_variable = true,
            _ => {}
        }
    }
    variables
}

fn declaration_kind_for_node(node: &SyntaxNode) -> DeclarationKind {
    if ClassDecl::cast(node).is_some() {
        DeclarationKind::Class
    } else if InterfaceDecl::cast(node).is_some() {
        DeclarationKind::Interface
    } else if TraitDecl::cast(node).is_some() {
        DeclarationKind::Trait
    } else if EnumDecl::cast(node).is_some() {
        DeclarationKind::Enum
    } else {
        DeclarationKind::ConditionalClassLike
    }
}

fn hir_decl_kind_for_declaration(kind: DeclarationKind) -> HirDeclKind {
    match kind {
        DeclarationKind::Function | DeclarationKind::ConditionalFunction => HirDeclKind::Function,
        DeclarationKind::Constant => HirDeclKind::Const,
        DeclarationKind::Class
        | DeclarationKind::Interface
        | DeclarationKind::Trait
        | DeclarationKind::Enum
        | DeclarationKind::ConditionalClassLike => HirDeclKind::ClassLike,
    }
}

fn declaration_fqn(namespace_name: Option<&NamespaceName>, short_name: &str) -> FullyQualifiedName {
    let mut parts = namespace_name
        .map(|namespace| {
            crate::hir::QualifiedName::parse(namespace.text())
                .parts()
                .to_vec()
        })
        .unwrap_or_default();
    parts.push(NamePart::new(short_name));
    FullyQualifiedName::from_parts(parts)
}

fn collect_top_level_node_effects(
    node: &SyntaxNode,
    item: TopLevelItem,
    block: &mut HirNamespaceBlock,
    reporter: &mut DiagnosticReporter,
) {
    if let Some(use_decl) = UseDecl::cast(node) {
        for import in collect_use_imports(use_decl) {
            if let Err(duplicate) = block.imports_mut().insert(import) {
                report_duplicate_import_alias(reporter, duplicate);
            }
        }
    } else {
        for resolved_name in collect_resolved_names(node, block) {
            block.push_resolved_name(resolved_name);
        }
    }
    block.push_item(item);
}

fn report_duplicate_import_alias(
    reporter: &mut DiagnosticReporter,
    duplicate: DuplicateImportAlias,
) {
    reporter.report(
        SemanticDiagnostic::with_span(
            DiagnosticId::DuplicateUseAlias,
            DiagnosticSeverity::Error,
            DiagnosticPhase::DeclarationCollection,
            format!(
                "duplicate {} import alias `{}`",
                duplicate.kind().as_str(),
                duplicate.alias()
            ),
            duplicate.duplicate_span(),
        )
        .with_label(DiagnosticLabel::new(
            duplicate.previous_span(),
            "previous import alias is here",
        )),
    );
}

fn report_duplicate_declaration(
    reporter: &mut DiagnosticReporter,
    duplicate: DuplicateDeclaration,
) {
    reporter.report(
        SemanticDiagnostic::with_span(
            DiagnosticId::DuplicateDeclaration,
            DiagnosticSeverity::Error,
            DiagnosticPhase::DeclarationCollection,
            format!(
                "duplicate {} declaration `{}`",
                duplicate.kind().as_str(),
                duplicate.name()
            ),
            duplicate.duplicate_span(),
        )
        .with_label(DiagnosticLabel::new(
            duplicate.previous_span(),
            "previous declaration is here",
        )),
    );
}

fn collect_resolved_names(node: &SyntaxNode, block: &HirNamespaceBlock) -> Vec<ResolvedNameRecord> {
    let resolver = NameResolver::new(block.name(), block.imports());
    let significant_tokens: Vec<_> = descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| !token.kind().is_trivia())
        .collect();
    descendant_nodes::<Name<'_>>(node)
        .map(|name| {
            let source = crate::hir::QualifiedName::from_ast_name(name);
            let context = infer_resolve_context(&significant_tokens, name.text_range());
            let result = resolver.resolve(&source, context);
            ResolvedNameRecord::new(source, context, result, name.text_range())
        })
        .collect()
}

fn infer_resolve_context(significant_tokens: &[TokenView<'_>], range: TextRange) -> ResolveContext {
    match next_significant_token_text(significant_tokens, range) {
        Some("(") => ResolveContext::FunctionCall,
        Some("::") => ResolveContext::ClassLike,
        _ => ResolveContext::ConstantFetch,
    }
}

fn next_significant_token_text<'tree>(
    significant_tokens: &'tree [TokenView<'tree>],
    range: TextRange,
) -> Option<&'tree str> {
    let end = range.end().to_usize();
    let index =
        significant_tokens.partition_point(|token| token.text_range().start().to_usize() < end);
    significant_tokens.get(index).map(|token| token.text())
}

fn top_level_item(node: &SyntaxNode) -> Option<TopLevelItem> {
    let kind = if InlineHtmlStmt::cast(node).is_some() {
        TopLevelItemKind::InlineHtml
    } else if node.kind().name() == "DECLARE_STMT" {
        TopLevelItemKind::Declare
    } else if UseDecl::cast(node).is_some() {
        TopLevelItemKind::Use
    } else if ConstDecl::cast(node).is_some() {
        TopLevelItemKind::Const
    } else if FunctionDecl::cast(node).is_some() {
        TopLevelItemKind::Function
    } else if ClassDecl::cast(node).is_some() {
        TopLevelItemKind::Class
    } else if InterfaceDecl::cast(node).is_some() {
        TopLevelItemKind::Interface
    } else if TraitDecl::cast(node).is_some() {
        TopLevelItemKind::Trait
    } else if EnumDecl::cast(node).is_some() {
        TopLevelItemKind::Enum
    } else if node.kind().name().ends_with("_STMT") {
        TopLevelItemKind::Statement
    } else {
        return None;
    };
    Some(TopLevelItem::new(kind, node.text_range()))
}

#[cfg(test)]
mod tests {
    use super::collect_module_declarations;
    use crate::FrontendDatabase;
    use crate::diagnostics::DiagnosticId;
    use crate::hir::{HirModule, NamespaceForm, TopLevelItemKind};
    use crate::scopes::{CaptureMode, ScopeKind};
    use crate::symbols::resolution::ResolveContext;
    use php_ast::source_file;
    use php_syntax::parse_source_file;

    #[test]
    fn collects_unbraced_namespace_blocks() {
        let parse =
            parse_source_file("<?php namespace A; function f() {} namespace B; const X = 1;");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);
        let module = database.module(module_id).expect("module");
        let namespaces: Vec<_> = module.namespaces().values().collect();

        assert!(diagnostics.is_empty());
        assert_eq!(namespaces.len(), 3);
        assert_eq!(namespaces[1].name().expect("name").text(), "A");
        assert_eq!(namespaces[1].form(), NamespaceForm::Unbraced);
        assert_eq!(namespaces[1].items()[0].kind(), TopLevelItemKind::Function);
        assert_eq!(namespaces[2].name().expect("name").text(), "B");
    }

    #[test]
    fn collects_braced_namespace_items() {
        let parse =
            parse_source_file("<?php namespace A { use B\\C; class D {} } namespace { echo 1; }");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);
        let module = database.module(module_id).expect("module");
        let namespaces: Vec<_> = module.namespaces().values().collect();

        assert!(diagnostics.is_empty());
        assert_eq!(namespaces.len(), 3);
        assert_eq!(namespaces[1].form(), NamespaceForm::Braced);
        assert!(
            namespaces[1]
                .items()
                .iter()
                .any(|item| item.kind() == TopLevelItemKind::Use)
        );
        assert!(
            namespaces[1]
                .items()
                .iter()
                .any(|item| item.kind() == TopLevelItemKind::Class)
        );
        assert!(namespaces[2].name().is_none());
    }

    #[test]
    fn collects_resolved_name_contexts_with_indexed_token_lookup() {
        let parse = parse_source_file("<?php Foo(); Foo::bar(); echo Foo;");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);
        let module = database.module(module_id).expect("module");
        let namespace = module
            .namespaces()
            .values()
            .find(|namespace| namespace.name().is_none())
            .expect("global namespace");
        let contexts: Vec<_> = namespace
            .resolved_names()
            .iter()
            .map(|record| (record.source().original().to_owned(), record.context()))
            .collect();

        assert!(diagnostics.is_empty());
        assert!(contexts.contains(&("Foo".to_string(), ResolveContext::FunctionCall)));
        assert!(contexts.contains(&("Foo".to_string(), ResolveContext::ClassLike)));
        assert!(contexts.contains(&("Foo".to_string(), ResolveContext::ConstantFetch)));
    }

    #[test]
    fn diagnoses_mixed_namespace_forms() {
        let parse = parse_source_file("<?php namespace A; namespace B { class C {} }");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::MixedNamespaceDeclarations)
        );
    }

    #[test]
    fn diagnoses_code_before_namespace() {
        let parse = parse_source_file("<?php echo 1; namespace A; function f() {}");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::NamespaceMustBeFirstStatement)
        );
    }

    #[test]
    fn records_inline_html_before_php_as_global_item() {
        let parse = parse_source_file("hello\n<?php namespace A; function f() {}");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);
        let module = database.module(module_id).expect("module");
        let namespaces: Vec<_> = module.namespaces().values().collect();

        assert!(diagnostics.is_empty());
        assert_eq!(namespaces.len(), 2);
        assert_eq!(namespaces[0].form(), NamespaceForm::Global);
        assert_eq!(
            namespaces[0].items()[0].kind(),
            TopLevelItemKind::InlineHtml
        );
        assert_eq!(namespaces[1].name().expect("name").text(), "A");
    }

    #[test]
    fn registers_top_level_declarations() {
        let parse = parse_source_file(
            "<?php namespace A; const X = 1, Y = 2; function f() {} class C {} interface I {} trait T {} enum E {}",
        );
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);
        let module = database.module(module_id).expect("module");
        let entries = module.declaration_table().entries();

        assert!(diagnostics.is_empty());
        assert_eq!(entries.len(), 7);
        assert_eq!(entries[0].decl_id().raw(), 0);
        assert_eq!(entries[0].symbol_id().raw(), 0);
        assert_eq!(
            entries[0].fqn().canonical(crate::hir::NameKind::Constant),
            "A\\X"
        );
        assert_eq!(entries[2].kind().as_str(), "function");
        assert_eq!(
            entries[2].fqn().canonical(crate::hir::NameKind::Function),
            "a\\f"
        );
        assert_eq!(entries[3].kind().as_str(), "class");
    }

    #[test]
    fn registers_conditional_declarations_without_duplicate_error() {
        let parse = parse_source_file(
            "<?php namespace A; if (true) { function f() {} class C {} } if (false) { function f() {} }",
        );
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);
        let module = database.module(module_id).expect("module");
        let entries = module.declaration_table().entries();

        assert!(diagnostics.is_empty());
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|entry| entry.kind().is_conditional()));
    }

    #[test]
    fn diagnoses_safe_duplicate_declarations() {
        let parse = parse_source_file("<?php namespace A; function f() {} function f() {}");
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.id() == DiagnosticId::DuplicateDeclaration)
        );
    }

    #[test]
    fn collects_scope_parents_and_function_like_contexts() {
        let parse = parse_source_file(
            "<?php namespace App; function outer($x, &...$rest) { global $g; static $s = 0; $c = function ($y) use (&$x, $g) { return $y; }; $a = fn($z) => $z + $x; } class C { public function m(string $v): void {} }",
        );
        let root = source_file(parse.root()).expect("source file");
        let mut database = FrontendDatabase::new();
        let module_id = database.add_module(HirModule::new("SOURCE_FILE", 0));
        let diagnostics = collect_module_declarations(root, &mut database, module_id);
        let module = database.module(module_id).expect("module");
        let scopes = module.scopes();

        assert!(diagnostics.is_empty());
        let file_id = scopes.root().expect("file scope");
        assert_eq!(scopes.get(file_id).expect("file").kind(), ScopeKind::File);

        let namespace_id = scopes
            .iter()
            .find(|(_, scope)| scope.kind() == ScopeKind::Namespace && scope.name() == Some("App"))
            .map(|(id, _)| id)
            .expect("App namespace scope");
        assert_eq!(
            scopes.get(namespace_id).expect("namespace").parent(),
            Some(file_id)
        );

        let function_id = scopes
            .iter()
            .find(|(_, scope)| scope.kind() == ScopeKind::Function && scope.name() == Some("outer"))
            .map(|(id, _)| id)
            .expect("outer function scope");
        let function = scopes.get(function_id).expect("function");
        let function_like = function.function_like().expect("function-like context");
        assert_eq!(function.parent(), Some(namespace_id));
        assert_eq!(function_like.parameters()[0].name(), "$x");
        assert!(function_like.parameters()[1].is_by_ref());
        assert!(function_like.parameters()[1].is_variadic());
        assert_eq!(function.globals()[0].name(), "$g");
        assert_eq!(function.statics()[0].variable().name(), "$s");

        let closure = scopes
            .iter()
            .find(|(_, scope)| scope.kind() == ScopeKind::Closure)
            .map(|(_, scope)| scope)
            .expect("closure scope");
        assert_eq!(closure.parent(), Some(function_id));
        let closure_like = closure.function_like().expect("closure context");
        assert_eq!(closure_like.parameters()[0].name(), "$y");
        assert_eq!(closure_like.captures()[0].name(), "$x");
        assert_eq!(
            closure_like.captures()[0].mode(),
            CaptureMode::ExplicitByReference
        );
        assert_eq!(closure_like.captures()[1].name(), "$g");
        assert_eq!(
            closure_like.captures()[1].mode(),
            CaptureMode::ExplicitByValue
        );

        let arrow = scopes
            .iter()
            .find(|(_, scope)| scope.kind() == ScopeKind::ArrowFunction)
            .map(|(_, scope)| scope)
            .expect("arrow function scope");
        assert_eq!(arrow.parent(), Some(function_id));
        assert_eq!(
            arrow.function_like().expect("arrow context").capture_mode(),
            Some(CaptureMode::ImplicitByValueDeferred)
        );

        let class_id = scopes
            .iter()
            .find(|(_, scope)| scope.kind() == ScopeKind::Class && scope.name() == Some("C"))
            .map(|(id, _)| id)
            .expect("class scope");
        assert_eq!(
            scopes.get(class_id).expect("class").parent(),
            Some(namespace_id)
        );

        let method = scopes
            .iter()
            .find(|(_, scope)| scope.kind() == ScopeKind::Method && scope.name() == Some("m"))
            .map(|(_, scope)| scope)
            .expect("method scope");
        assert_eq!(method.parent(), Some(class_id));
        assert_eq!(
            method.function_like().expect("method context").parameters()[0].name(),
            "$v"
        );
    }
}
