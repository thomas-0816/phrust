use std::path::{Path, PathBuf};

/// Parser fixture category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParserFixtureKind {
    /// Expected to be accepted by the reference parser.
    Valid,
    /// Expected to be rejected by the reference parser.
    Invalid,
    /// Expected to exercise parser recovery.
    Recovery,
    /// PHP 8.5-specific syntax fixture.
    Php85,
    /// Any other fixture group.
    Other,
}

/// Parser fixture metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParserFixture {
    /// Fixture path.
    pub path: PathBuf,
    /// Fixture kind inferred from its path.
    pub kind: ParserFixtureKind,
}

impl ParserFixture {
    /// Creates fixture metadata.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let kind = infer_kind(&path);
        Self { path, kind }
    }
}

fn infer_kind(path: &Path) -> ParserFixtureKind {
    let path = path.to_string_lossy();
    if path.contains("/valid/") {
        ParserFixtureKind::Valid
    } else if path.contains("/invalid/") {
        ParserFixtureKind::Invalid
    } else if path.contains("/recovery/") {
        ParserFixtureKind::Recovery
    } else if path.contains("/php85/") {
        ParserFixtureKind::Php85
    } else {
        ParserFixtureKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::{ParserFixture, ParserFixtureKind};
    use std::path::PathBuf;

    #[test]
    fn infers_fixture_kind_from_path() {
        let fixture = ParserFixture::new(PathBuf::from("fixtures/parser/invalid/missing.php"));
        assert_eq!(fixture.kind, ParserFixtureKind::Invalid);
    }
}
