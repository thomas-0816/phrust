//! Source-preserving PHP name model.

use php_ast::{AstNode, AstToken, Name, TokenView, descendant_tokens};

/// Semantic category for source-level names.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NameKind {
    /// Class, interface, trait, or enum name.
    ClassLike,
    /// Function name.
    Function,
    /// Constant name.
    Constant,
    /// Namespace name.
    Namespace,
    /// Goto label name.
    Label,
    /// Variable identifier without the leading `$`.
    VariableName,
}

impl NameKind {
    /// Returns true when this category uses ASCII case-insensitive canonical
    /// spelling for the source-level name model.
    #[must_use]
    pub const fn is_case_insensitive(self) -> bool {
        matches!(
            self,
            Self::ClassLike | Self::Function | Self::Namespace | Self::Label
        )
    }
}

/// Source text for a name exactly as reconstructed from non-trivia CST tokens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawName {
    text: String,
}

impl RawName {
    /// Creates a raw name from source text.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// Returns the original source spelling.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One `\`-separated name segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamePart {
    original: String,
    ascii_lowercase: String,
}

impl NamePart {
    /// Creates a name part and precomputes ASCII-only lowercase spelling.
    #[must_use]
    pub fn new(original: impl Into<String>) -> Self {
        let original = original.into();
        let ascii_lowercase = original.to_ascii_lowercase();
        Self {
            original,
            ascii_lowercase,
        }
    }

    /// Returns the original spelling.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Returns the ASCII-only lowercase spelling.
    #[must_use]
    pub fn ascii_lowercase(&self) -> &str {
        &self.ascii_lowercase
    }

    /// Returns the canonical spelling for a semantic name category.
    #[must_use]
    pub fn canonical(&self, kind: NameKind) -> &str {
        if kind.is_case_insensitive() {
            &self.ascii_lowercase
        } else {
            &self.original
        }
    }
}

/// Parsed source name with prefix markers retained separately from parts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualifiedName {
    raw: RawName,
    parts: Vec<NamePart>,
    leading_slash: bool,
    namespace_relative: bool,
}

impl QualifiedName {
    /// Parses source-level name text without adding semantic resolution.
    #[must_use]
    pub fn parse(text: impl Into<String>) -> Self {
        let text = text.into();
        let leading_slash = text.starts_with('\\');
        let without_leading = text.strip_prefix('\\').unwrap_or(&text);
        let raw_parts: Vec<&str> = without_leading
            .split('\\')
            .filter(|part| !part.is_empty())
            .collect();
        let namespace_relative = !leading_slash
            && raw_parts.len() > 1
            && raw_parts
                .first()
                .is_some_and(|part| part.eq_ignore_ascii_case("namespace"));
        let semantic_parts = if namespace_relative {
            &raw_parts[1..]
        } else {
            &raw_parts[..]
        };
        let parts = semantic_parts
            .iter()
            .map(|part| NamePart::new(*part))
            .collect();

        Self {
            raw: RawName::new(text),
            parts,
            leading_slash,
            namespace_relative,
        }
    }

    /// Lowers a CST-backed AST name into the HIR name model.
    #[must_use]
    pub fn from_ast_name(name: Name<'_>) -> Self {
        let mut text = String::new();
        for token in descendant_tokens::<TokenView<'_>>(name.syntax()) {
            if !token.kind().is_trivia() {
                text.push_str(token.text());
            }
        }
        Self::parse(text)
    }

    /// Returns the raw source spelling.
    #[must_use]
    pub fn raw(&self) -> &RawName {
        &self.raw
    }

    /// Returns the raw source spelling.
    #[must_use]
    pub fn original(&self) -> &str {
        self.raw.text()
    }

    /// Returns true for names written with a leading `\`.
    #[must_use]
    pub const fn has_leading_slash(&self) -> bool {
        self.leading_slash
    }

    /// Returns true for names written as `namespace\Foo`.
    #[must_use]
    pub const fn is_namespace_relative(&self) -> bool {
        self.namespace_relative
    }

    /// Returns true for fully-qualified source names.
    #[must_use]
    pub const fn is_fully_qualified(&self) -> bool {
        self.leading_slash
    }

    /// Returns the semantic name parts. The `namespace` relative marker is not
    /// included in this slice.
    #[must_use]
    pub fn parts(&self) -> &[NamePart] {
        &self.parts
    }

    /// Returns a prefix-preserving canonical key for this source name.
    #[must_use]
    pub fn canonical(&self, kind: NameKind) -> String {
        let mut out = String::new();
        if self.leading_slash {
            out.push('\\');
        } else if self.namespace_relative {
            out.push_str("namespace\\");
        }

        for (index, part) in self.parts.iter().enumerate() {
            if index > 0 {
                out.push('\\');
            }
            out.push_str(part.canonical(kind));
        }
        out
    }

    /// Converts a fully-qualified source name into an FQN value.
    #[must_use]
    pub fn to_fully_qualified(&self) -> Option<FullyQualifiedName> {
        self.leading_slash
            .then(|| FullyQualifiedName::from_parts(self.parts.clone()))
    }
}

/// Fully-qualified name parts without a leading slash marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullyQualifiedName {
    parts: Vec<NamePart>,
}

impl FullyQualifiedName {
    /// Creates an FQN from already parsed name parts.
    #[must_use]
    pub fn from_parts(parts: Vec<NamePart>) -> Self {
        Self { parts }
    }

    /// Parses an FQN from source text, accepting an optional leading slash.
    #[must_use]
    pub fn parse(text: impl Into<String>) -> Self {
        let qualified = QualifiedName::parse(text);
        Self {
            parts: qualified.parts,
        }
    }

    /// Returns FQN parts.
    #[must_use]
    pub fn parts(&self) -> &[NamePart] {
        &self.parts
    }

    /// Returns canonical FQN text without a leading slash.
    #[must_use]
    pub fn canonical(&self, kind: NameKind) -> String {
        self.parts
            .iter()
            .map(|part| part.canonical(kind))
            .collect::<Vec<_>>()
            .join("\\")
    }

    /// Returns the resolved FQN with each source name segment's spelling.
    #[must_use]
    pub fn display(&self) -> String {
        self.parts
            .iter()
            .map(NamePart::original)
            .collect::<Vec<_>>()
            .join("\\")
    }
}

/// Source-preserving name text captured for later name resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirName {
    text: String,
}

impl HirName {
    /// Creates a HIR name from source text.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// Returns the original name text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::{FullyQualifiedName, NameKind, QualifiedName};
    use php_ast::{Name, descendant_nodes};
    use php_syntax::parse_source_file;

    #[test]
    fn parses_unqualified_names() {
        let name = QualifiedName::parse("Foo");

        assert_eq!(name.original(), "Foo");
        assert!(!name.has_leading_slash());
        assert!(!name.is_namespace_relative());
        assert_eq!(name.parts()[0].original(), "Foo");
        assert_eq!(name.canonical(NameKind::ClassLike), "foo");
    }

    #[test]
    fn parses_qualified_names() {
        let name = QualifiedName::parse("Foo\\Bar");

        assert_eq!(name.parts().len(), 2);
        assert_eq!(name.parts()[0].original(), "Foo");
        assert_eq!(name.parts()[1].original(), "Bar");
        assert_eq!(name.canonical(NameKind::Function), "foo\\bar");
    }

    #[test]
    fn detects_fully_qualified_names() {
        let name = QualifiedName::parse("\\Foo\\Bar");

        assert!(name.is_fully_qualified());
        assert_eq!(name.canonical(NameKind::ClassLike), "\\foo\\bar");
        assert_eq!(
            name.to_fully_qualified()
                .expect("fqn")
                .canonical(NameKind::ClassLike),
            "foo\\bar"
        );
    }

    #[test]
    fn detects_namespace_relative_names() {
        let name = QualifiedName::parse("namespace\\Foo");

        assert!(name.is_namespace_relative());
        assert_eq!(name.parts().len(), 1);
        assert_eq!(name.parts()[0].original(), "Foo");
        assert_eq!(name.canonical(NameKind::ClassLike), "namespace\\foo");
    }

    #[test]
    fn keeps_case_sensitive_variable_spelling() {
        let name = QualifiedName::parse("Foo");

        assert_eq!(name.canonical(NameKind::VariableName), "Foo");
        assert_eq!(name.canonical(NameKind::ClassLike), "foo");
    }

    #[test]
    fn leaves_non_ascii_bytes_to_lexer_semantics() {
        let name = QualifiedName::parse("Ä\\FOO");

        assert_eq!(name.parts()[0].original(), "Ä");
        assert_eq!(name.parts()[0].ascii_lowercase(), "Ä");
        assert_eq!(name.canonical(NameKind::ClassLike), "Ä\\foo");
    }

    #[test]
    fn parses_fqn_without_leading_marker() {
        let name = FullyQualifiedName::parse("\\Foo\\Bar");

        assert_eq!(name.canonical(NameKind::Namespace), "foo\\bar");
    }

    #[test]
    fn lowers_ast_name_without_relexing() {
        let parse = parse_source_file("<?php Foo\\Bar();");
        let ast_name = descendant_nodes::<Name<'_>>(parse.root())
            .next()
            .expect("name");
        let name = QualifiedName::from_ast_name(ast_name);

        assert_eq!(name.original(), "Foo\\Bar");
        assert_eq!(name.canonical(NameKind::Function), "foo\\bar");
    }
}
