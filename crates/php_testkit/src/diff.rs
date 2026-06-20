use serde::{Deserialize, Serialize};

/// Normalized parser-side result for acceptance comparison.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RustParseResult {
    /// Source file path.
    pub file: String,
    /// True when the Rust parser accepts the file.
    pub ok: bool,
    /// Number of parser diagnostics.
    pub diagnostics: usize,
}

/// Acceptance comparison result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParserAcceptanceDiff {
    /// Source file path.
    pub file: String,
    /// PHP reference acceptance.
    pub reference_ok: bool,
    /// Rust parser acceptance.
    pub rust_ok: bool,
}

impl ParserAcceptanceDiff {
    /// Returns true when both sides agree.
    #[must_use]
    pub const fn matches(&self) -> bool {
        self.reference_ok == self.rust_ok
    }
}

#[cfg(test)]
mod tests {
    use super::ParserAcceptanceDiff;

    #[test]
    fn reports_acceptance_match() {
        let diff = ParserAcceptanceDiff {
            file: "fixture.php".to_owned(),
            reference_ok: true,
            rust_ok: true,
        };
        assert!(diff.matches());
    }
}
