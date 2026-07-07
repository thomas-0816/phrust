//! Type annotation lowering.

use std::collections::HashSet;

use crate::FrontendDatabase;
use crate::diagnostics::{
    DiagnosticId, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity, SemanticDiagnostic,
};
use crate::hir::{
    BuiltinType, FullyQualifiedName, HirModule, HirType, HirTypeKind, ModuleId, NameKind,
    NamespaceName, QualifiedName, TypeContext, TypeId,
};
use crate::symbols::imports::ImportTable;
use crate::symbols::resolution::{NameResolver, ResolveContext, ResolvedName};
use php_ast::{
    ArrowFunctionExpr, AstNode, AstToken, CatchClause, ClassConstDecl, ClosureExpr, EnumDecl,
    FunctionDecl, MethodDecl, Param, PropertyDecl, TokenView, TypeKeyword, TypeView,
    descendant_tokens, syntax_child_nodes, syntax_child_tokens,
};
use php_source::TextRange;
use php_syntax::{SyntaxNode, SyntaxToken};

/// Namespace/import context for type-name resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeLoweringScope {
    namespace_name: Option<NamespaceName>,
    imports: ImportTable,
}

impl TypeLoweringScope {
    /// Creates a type-lowering scope from a namespace block snapshot.
    #[must_use]
    pub const fn new(namespace_name: Option<NamespaceName>, imports: ImportTable) -> Self {
        Self {
            namespace_name,
            imports,
        }
    }

    pub(crate) fn resolver(&self) -> NameResolver<'_> {
        NameResolver::new(self.namespace_name.as_ref(), &self.imports)
    }

    pub(crate) const fn namespace_name(&self) -> Option<&NamespaceName> {
        self.namespace_name.as_ref()
    }
}

/// Collects and lowers type annotations inside one top-level node.
pub fn collect_types_in_node(
    node: &SyntaxNode,
    database: &mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &mut DiagnosticReporter,
    scope: TypeLoweringScope,
) {
    let mut lowerer = TypeLowerer {
        database,
        module_id,
        reporter,
        scope,
        class_like_depth: 0,
    };
    lowerer.collect_node(node);
}

/// Lowers a CST type view into a HIR type ID.
pub fn lower_type(
    type_view: TypeView<'_>,
    database: &mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &mut DiagnosticReporter,
    scope: TypeLoweringScope,
    context: TypeContext,
    in_class_like: bool,
) -> Option<TypeId> {
    let mut lowerer = TypeLowerer {
        database,
        module_id,
        reporter,
        scope,
        class_like_depth: usize::from(in_class_like),
    };
    lowerer.lower_type_view(type_view, context)
}

/// Lowers already-collected type tokens into a HIR type ID.
pub(crate) fn lower_type_tokens(
    tokens: &[TypeToken],
    database: &mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &mut DiagnosticReporter,
    scope: TypeLoweringScope,
    context: TypeContext,
    in_class_like: bool,
) -> Option<TypeId> {
    let mut lowerer = TypeLowerer {
        database,
        module_id,
        reporter,
        scope,
        class_like_depth: usize::from(in_class_like),
    };
    lowerer.lower_tokens(tokens, context)
}

struct TypeLowerer<'a> {
    database: &'a mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &'a mut DiagnosticReporter,
    scope: TypeLoweringScope,
    class_like_depth: usize,
}

impl TypeLowerer<'_> {
    fn collect_node(&mut self, node: &SyntaxNode) {
        if TypeView::cast(node).is_some() {
            return;
        }

        if FunctionDecl::cast(node).is_some()
            || ClosureExpr::cast(node).is_some()
            || ArrowFunctionExpr::cast(node).is_some()
            || MethodDecl::cast(node).is_some()
            || Param::cast(node).is_some()
        {
            // Function-like signatures are lowered by `lower::signatures` so
            // parameters and returns produce one TypeId and one diagnostic.
        } else if let Some(property) = PropertyDecl::cast(node) {
            self.collect_property(property);
        } else if let Some(class_const) = ClassConstDecl::cast(node) {
            self.collect_class_const(class_const);
        } else if let Some(catch_clause) = CatchClause::cast(node) {
            self.collect_catch(catch_clause);
        } else if let Some(enum_decl) = EnumDecl::cast(node) {
            self.collect_enum(enum_decl);
        } else if is_class_like_node(node) {
            self.class_like_depth += 1;
            self.collect_children(node);
            self.class_like_depth -= 1;
        } else {
            self.collect_children(node);
        }
    }

    fn collect_children(&mut self, node: &SyntaxNode) {
        for child in syntax_child_nodes(node) {
            self.collect_node(child);
        }
    }

    fn collect_property(&mut self, property: PropertyDecl<'_>) {
        if let Some(tokens) = property_type_tokens(property.syntax()) {
            self.lower_tokens(&tokens, TypeContext::Property);
        }
    }

    fn collect_class_const(&mut self, class_const: ClassConstDecl<'_>) {
        if let Some(tokens) = class_const_type_tokens(class_const.syntax()) {
            self.lower_tokens(&tokens, TypeContext::ClassConstant);
        }
    }

    fn collect_catch(&mut self, catch_clause: CatchClause<'_>) {
        if let Some(tokens) = catch_type_tokens(catch_clause.syntax()) {
            self.lower_tokens(&tokens, TypeContext::Catch);
        }
        self.collect_children(catch_clause.syntax());
    }

    fn collect_enum(&mut self, enum_decl: EnumDecl<'_>) {
        self.class_like_depth += 1;
        if let Some(tokens) = enum_backing_type_tokens(enum_decl.syntax()) {
            self.lower_tokens(&tokens, TypeContext::EnumBacking);
        }
        self.collect_children(enum_decl.syntax());
        self.class_like_depth -= 1;
    }

    fn lower_type_view(&mut self, type_view: TypeView<'_>, context: TypeContext) -> Option<TypeId> {
        let tokens = tokens_from_node(type_view.syntax());
        self.lower_tokens(&tokens, context)
    }

    fn lower_tokens(&mut self, tokens: &[TypeToken], context: TypeContext) -> Option<TypeId> {
        if tokens.is_empty() {
            return None;
        }
        let mut parser = TypeParser::new(tokens, self, context);
        parser.parse_type()
    }

    fn alloc(&mut self, kind: HirTypeKind, context: TypeContext, tokens: &[TypeToken]) -> TypeId {
        let source_form = token_source(tokens);
        let span = token_range(tokens).expect("non-empty type token slice");
        self.validate_kind(&kind, context, span);
        let id = {
            let module = self
                .database
                .module_mut(self.module_id)
                .expect("module allocated before type lowering");
            module
                .types_mut()
                .alloc(HirType::new(kind, context, source_form))
        };
        self.database.source_map_mut().insert(id, span);
        id
    }

    fn validate_kind(&mut self, kind: &HirTypeKind, context: TypeContext, span: TextRange) {
        match kind {
            HirTypeKind::Void if context != TypeContext::Return => {
                self.error(
                    DiagnosticId::InvalidTypeVoidContext,
                    "`void` type is only allowed as a return type",
                    span,
                );
            }
            HirTypeKind::Never if context != TypeContext::Return => {
                self.error(
                    DiagnosticId::InvalidTypeNeverContext,
                    "`never` type is only allowed as a return type",
                    span,
                );
            }
            HirTypeKind::StaticType
                if context != TypeContext::Return || self.class_like_depth == 0 =>
            {
                self.error(
                    DiagnosticId::InvalidTypeStaticContext,
                    "`static` type is only allowed as a class-like return type",
                    span,
                );
            }
            HirTypeKind::SelfType if self.class_like_depth == 0 => {
                self.error(
                    DiagnosticId::InvalidTypeSelfContext,
                    "`self` type requires a class-like scope",
                    span,
                );
            }
            HirTypeKind::ParentType if self.class_like_depth == 0 => {
                self.error(
                    DiagnosticId::InvalidTypeParentContext,
                    "`parent` type requires a class-like scope",
                    span,
                );
            }
            HirTypeKind::Builtin(BuiltinType::Callable) if context == TypeContext::Property => {
                self.error(
                    DiagnosticId::InvalidTypeCallableContext,
                    "`callable` is not allowed as a property type",
                    span,
                );
            }
            HirTypeKind::Intersection { members }
                if members.iter().any(|member| self.type_is_callable(*member)) =>
            {
                self.error(
                    DiagnosticId::InvalidTypeCallableContext,
                    "Type callable cannot be part of an intersection type",
                    span,
                );
            }
            _ => {}
        }
    }

    fn type_is_callable(&self, ty: TypeId) -> bool {
        matches!(
            self.module().types()[ty].kind(),
            HirTypeKind::Builtin(BuiltinType::Callable)
        )
    }

    fn check_duplicate_members(&mut self, members: &[TypeId]) {
        let mut seen = HashSet::<String>::new();
        for member in members {
            let key = {
                let module = self.module();
                type_duplicate_key(module, *member)
            };
            if !seen.insert(key)
                && let Some(span) = self.database.source_map().span(*member)
            {
                self.error(
                    DiagnosticId::DuplicateTypeAlternative,
                    "duplicate type alternative",
                    span,
                );
            }
        }
    }

    fn resolve_name(&self, name: &QualifiedName) -> Option<FullyQualifiedName> {
        match self
            .scope
            .resolver()
            .resolve(name, ResolveContext::TypeName)
        {
            ResolvedName::FullyQualified(name) => Some(name),
            ResolvedName::MaybeRuntimeFallback { namespaced, .. } => Some(namespaced),
            ResolvedName::Dynamic | ResolvedName::Unresolved => None,
        }
    }

    fn module(&self) -> &HirModule {
        self.database
            .module(self.module_id)
            .expect("module allocated before type lowering")
    }

    fn error(&mut self, id: DiagnosticId, message: impl Into<String>, span: TextRange) {
        self.reporter.report(SemanticDiagnostic::with_span(
            id,
            DiagnosticSeverity::Error,
            DiagnosticPhase::TypeLowering,
            message,
            span,
        ));
    }
}

struct TypeParser<'tokens, 'lowerer, 'db> {
    tokens: &'tokens [TypeToken],
    pos: usize,
    lowerer: &'lowerer mut TypeLowerer<'db>,
    context: TypeContext,
}

impl<'tokens, 'lowerer, 'db> TypeParser<'tokens, 'lowerer, 'db> {
    fn new(
        tokens: &'tokens [TypeToken],
        lowerer: &'lowerer mut TypeLowerer<'db>,
        context: TypeContext,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            lowerer,
            context,
        }
    }

    fn parse_type(&mut self) -> Option<TypeId> {
        self.parse_union()
    }

    fn parse_union(&mut self) -> Option<TypeId> {
        let start = self.pos;
        let mut members = Vec::new();
        let mut has_intersection = false;
        let first = self.parse_intersection()?;
        has_intersection |= matches!(
            self.lowerer.module().types()[first].kind(),
            HirTypeKind::Intersection { .. }
        );
        members.push(first);

        while self.at_text("|") {
            self.pos += 1;
            let Some(member) = self.parse_intersection() else {
                break;
            };
            has_intersection |= matches!(
                self.lowerer.module().types()[member].kind(),
                HirTypeKind::Intersection { .. }
            );
            members.push(member);
        }

        if members.len() == 1 {
            return members.first().copied();
        }

        let end = self.pos;
        self.lowerer.check_duplicate_members(&members);
        let kind = if has_intersection {
            HirTypeKind::Dnf { members }
        } else {
            HirTypeKind::Union {
                members,
                normalized_from_nullable: false,
            }
        };
        Some(
            self.lowerer
                .alloc(kind, self.context, &self.tokens[start..end]),
        )
    }

    fn parse_intersection(&mut self) -> Option<TypeId> {
        let start = self.pos;
        let mut members = Vec::new();
        let first = self.parse_primary()?;
        members.push(first);

        while self.at_text("&") {
            self.pos += 1;
            let Some(member) = self.parse_primary() else {
                break;
            };
            members.push(member);
        }

        if members.len() == 1 {
            return members.first().copied();
        }

        let end = self.pos;
        self.lowerer.check_duplicate_members(&members);
        Some(self.lowerer.alloc(
            HirTypeKind::Intersection { members },
            self.context,
            &self.tokens[start..end],
        ))
    }

    fn parse_primary(&mut self) -> Option<TypeId> {
        if self.at_text("?") {
            let start = self.pos;
            self.pos += 1;
            let inner = self.parse_primary()?;
            let end = self.pos;
            return Some(self.lowerer.alloc(
                HirTypeKind::Nullable {
                    inner,
                    normalized_null: true,
                },
                self.context,
                &self.tokens[start..end],
            ));
        }

        if self.at_text("(") {
            let start = self.pos;
            self.pos += 1;
            let inner = self.parse_type();
            if self.at_text(")") {
                self.pos += 1;
            }
            if self.at_text("[") {
                self.pos += 1;
                if self.at_text("]") {
                    self.pos += 1;
                }
                return Some(self.lowerer.alloc(
                    HirTypeKind::ArrayOf {
                        element_type: inner?,
                    },
                    self.context,
                    &self.tokens[start..self.pos],
                ));
            }
            return inner;
        }

        let start = self.pos;
        let atom = self.parse_atom()?;
        let mut inner = atom;
        while self.at_text("[") {
            self.pos += 1;
            if self.at_text("]") {
                self.pos += 1;
            }
            let end = self.pos;
            inner = self.lowerer.alloc(
                HirTypeKind::ArrayOf {
                    element_type: inner,
                },
                self.context,
                &self.tokens[start..end],
            );
        }
        Some(inner)
    }

    fn parse_atom(&mut self) -> Option<TypeId> {
        let token = self.tokens.get(self.pos)?;
        if !is_type_atom_token(token) {
            self.pos += 1;
            return None;
        }
        self.pos += 1;
        let source = token.text.as_str();
        let lower = source.to_ascii_lowercase();
        let kind = match lower.as_str() {
            "void" => HirTypeKind::Void,
            "never" => HirTypeKind::Never,
            "static" => HirTypeKind::StaticType,
            "self" => HirTypeKind::SelfType,
            "parent" => HirTypeKind::ParentType,
            "false" => HirTypeKind::False,
            "true" => HirTypeKind::True,
            "null" => HirTypeKind::Null,
            "mixed" => HirTypeKind::Mixed,
            "array" => HirTypeKind::Builtin(BuiltinType::Array),
            "callable" => HirTypeKind::Builtin(BuiltinType::Callable),
            "object" => HirTypeKind::Builtin(BuiltinType::Object),
            "iterable" => HirTypeKind::Builtin(BuiltinType::Iterable),
            "bool" | "boolean" => HirTypeKind::Builtin(BuiltinType::Bool),
            "int" | "integer" => HirTypeKind::Builtin(BuiltinType::Int),
            "float" | "double" => HirTypeKind::Builtin(BuiltinType::Float),
            "string" => HirTypeKind::Builtin(BuiltinType::String),
            _ => {
                let name = QualifiedName::parse(source);
                let resolved = self.lowerer.resolve_name(&name);
                HirTypeKind::Named { name, resolved }
            }
        };
        Some(
            self.lowerer
                .alloc(kind, self.context, &self.tokens[self.pos - 1..self.pos]),
        )
    }

    fn at_text(&self, text: &str) -> bool {
        self.tokens
            .get(self.pos)
            .is_some_and(|token| token.text == text)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypeToken {
    pub(crate) text: String,
    pub(crate) kind: String,
    pub(crate) range: TextRange,
}

fn tokens_from_node(node: &SyntaxNode) -> Vec<TypeToken> {
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| !token.kind().is_trivia())
        .map(type_token_from_ast_token)
        .collect()
}

fn type_token_from_ast_token(token: TokenView<'_>) -> TypeToken {
    TypeToken {
        text: token.text().to_owned(),
        kind: token.kind().name(),
        range: token.text_range(),
    }
}

pub(crate) fn type_token_from_syntax_token(token: &SyntaxToken) -> TypeToken {
    TypeToken {
        text: token.text().to_owned(),
        kind: token.kind().name(),
        range: token.text_range(),
    }
}

fn token_source(tokens: &[TypeToken]) -> String {
    tokens.iter().map(|token| token.text.as_str()).collect()
}

pub(crate) fn token_range(tokens: &[TypeToken]) -> Option<TextRange> {
    let first = tokens.first()?;
    let last = tokens.last()?;
    Some(TextRange::new(
        first.range.start().to_usize(),
        last.range.end().to_usize(),
    ))
}

pub(crate) fn is_type_atom_token(token: &TypeToken) -> bool {
    matches!(
        token.kind.as_str(),
        "T_STRING"
            | "T_ARRAY"
            | "T_CALLABLE"
            | "T_STATIC"
            | "T_NAME_FULLY_QUALIFIED"
            | "T_NAME_QUALIFIED"
            | "T_NAME_RELATIVE"
    )
}

fn property_type_tokens(node: &SyntaxNode) -> Option<Vec<TypeToken>> {
    let tokens: Vec<TypeToken> = syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .map(type_token_from_syntax_token)
        .collect();
    let variable = tokens
        .iter()
        .position(|token| token.kind == "T_VARIABLE")
        .unwrap_or(tokens.len());
    let type_tokens: Vec<_> = tokens[..variable]
        .iter()
        .filter(|token| !is_property_modifier_token(token))
        .cloned()
        .collect();
    non_empty_type_tokens(type_tokens)
}

fn class_const_type_tokens(node: &SyntaxNode) -> Option<Vec<TypeToken>> {
    let tokens: Vec<TypeToken> = syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .map(type_token_from_syntax_token)
        .collect();
    let const_index = tokens.iter().position(|token| token.kind == "T_CONST")?;
    let equals = tokens
        .iter()
        .skip(const_index + 1)
        .position(|token| matches!(token.text.as_str(), "=" | ";" | ","))
        .map(|offset| const_index + 1 + offset)
        .unwrap_or(tokens.len());
    let before_name = &tokens[const_index + 1..equals];
    let last_name = before_name
        .iter()
        .rposition(|token| token.kind == "T_STRING")?;
    if last_name == 0 {
        return None;
    }
    non_empty_type_tokens(before_name[..last_name].to_vec())
}

fn catch_type_tokens(node: &SyntaxNode) -> Option<Vec<TypeToken>> {
    let tokens: Vec<TypeToken> = syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .map(type_token_from_syntax_token)
        .collect();
    let open = tokens.iter().position(|token| token.text == "(")?;
    let variable = tokens
        .iter()
        .skip(open + 1)
        .position(|token| token.kind == "T_VARIABLE")
        .map(|offset| open + 1 + offset)?;
    non_empty_type_tokens(tokens[open + 1..variable].to_vec())
}

pub(crate) fn enum_backing_type_tokens(node: &SyntaxNode) -> Option<Vec<TypeToken>> {
    let tokens: Vec<TypeToken> = syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .map(type_token_from_syntax_token)
        .collect();
    let colon = tokens.iter().position(|token| token.text == ":")?;
    let type_tokens = tokens
        .iter()
        .skip(colon + 1)
        .take_while(|token| token.text != "{")
        .cloned()
        .collect();
    non_empty_type_tokens(type_tokens)
}

pub(crate) fn non_empty_type_tokens(tokens: Vec<TypeToken>) -> Option<Vec<TypeToken>> {
    let tokens: Vec<_> = tokens
        .into_iter()
        .filter(|token| {
            is_type_atom_token(token)
                || matches!(token.text.as_str(), "?" | "|" | "&" | "(" | ")" | "[" | "]")
        })
        .collect();
    (!tokens.is_empty()).then_some(tokens)
}

fn is_property_modifier_token(token: &TypeToken) -> bool {
    matches!(
        token.kind.as_str(),
        "T_PUBLIC"
            | "T_PROTECTED"
            | "T_PRIVATE"
            | "T_PUBLIC_SET"
            | "T_PROTECTED_SET"
            | "T_PRIVATE_SET"
            | "T_STATIC"
            | "T_READONLY"
            | "T_VAR"
    )
}

fn is_class_like_node(node: &SyntaxNode) -> bool {
    matches!(
        node.kind().name().as_str(),
        "CLASS_DECL" | "INTERFACE_DECL" | "TRAIT_DECL" | "ENUM_DECL"
    )
}

fn type_duplicate_key(module: &HirModule, id: TypeId) -> String {
    match module.types()[id].kind() {
        HirTypeKind::Named { name, resolved } => resolved
            .as_ref()
            .map(|name| name.canonical(NameKind::ClassLike))
            .unwrap_or_else(|| name.canonical(NameKind::ClassLike)),
        HirTypeKind::Builtin(builtin) => builtin.as_str().to_owned(),
        HirTypeKind::Nullable { inner, .. } => {
            format!("nullable:{}", type_duplicate_key(module, *inner))
        }
        HirTypeKind::Union { members, .. } => {
            let mut keys: Vec<_> = members
                .iter()
                .map(|member| type_duplicate_key(module, *member))
                .collect();
            keys.sort();
            format!("union:{}", keys.join("|"))
        }
        HirTypeKind::Intersection { members } => {
            let mut keys: Vec<_> = members
                .iter()
                .map(|member| type_duplicate_key(module, *member))
                .collect();
            keys.sort();
            format!("intersection:{}", keys.join("&"))
        }
        HirTypeKind::Dnf { members } => {
            let mut keys: Vec<_> = members
                .iter()
                .map(|member| type_duplicate_key(module, *member))
                .collect();
            keys.sort();
            format!("dnf:{}", keys.join("|"))
        }
        HirTypeKind::SelfType => "self".to_owned(),
        HirTypeKind::ParentType => "parent".to_owned(),
        HirTypeKind::StaticType => "static".to_owned(),
        HirTypeKind::Mixed => "mixed".to_owned(),
        HirTypeKind::Never => "never".to_owned(),
        HirTypeKind::Void => "void".to_owned(),
        HirTypeKind::Null => "null".to_owned(),
        HirTypeKind::False => "false".to_owned(),
        HirTypeKind::True => "true".to_owned(),
        HirTypeKind::ArrayOf { element_type } => {
            format!("array<{}>", type_duplicate_key(module, *element_type))
        }
        HirTypeKind::Missing => "missing".to_owned(),
        HirTypeKind::Unlowered => "unlowered".to_owned(),
    }
}

#[allow(dead_code)]
fn keyword_from_source(source: &str) -> Option<TypeKeyword> {
    match source.to_ascii_lowercase().as_str() {
        "void" => Some(TypeKeyword::Void),
        "never" => Some(TypeKeyword::Never),
        "static" => Some(TypeKeyword::Static),
        "self" => Some(TypeKeyword::Self_),
        "parent" => Some(TypeKeyword::Parent),
        "false" => Some(TypeKeyword::False),
        "true" => Some(TypeKeyword::True),
        "null" => Some(TypeKeyword::Null),
        "mixed" => Some(TypeKeyword::Mixed),
        "iterable" => Some(TypeKeyword::Iterable),
        "object" => Some(TypeKeyword::Object),
        "callable" => Some(TypeKeyword::Callable),
        "array" => Some(TypeKeyword::Array),
        _ => None,
    }
}
