use crate::SyntaxKind;

/// Stable expected syntax set used by diagnostics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExpectedSet {
    items: Vec<SyntaxKind>,
}

impl ExpectedSet {
    /// Creates an empty set.
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Adds an expected kind.
    pub fn push(&mut self, kind: SyntaxKind) {
        if !self.items.contains(&kind) {
            self.items.push(kind);
        }
    }

    /// Returns true when no syntax kinds are expected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns expected kinds.
    #[must_use]
    pub fn items(&self) -> &[SyntaxKind] {
        &self.items
    }

    /// Returns stable expected syntax names for diagnostics.
    #[must_use]
    pub fn syntax_names(&self) -> Vec<String> {
        self.items.iter().map(|kind| kind.name()).collect()
    }
}
