use serde::{Deserialize, Serialize};

/// Normalized `php -l` result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhpLintResult {
    /// Source file path.
    pub file: String,
    /// True when PHP accepts the file.
    pub ok: bool,
    /// Process exit code from `php -l`.
    pub exit_code: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// PHP version string.
    pub php_version: String,
}

impl PhpLintResult {
    /// Parses a normalized JSON result.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

/// Normalized Rust parser CLI result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RustParserResult {
    /// Source file path.
    pub file: String,
    /// True when the Rust parser accepts the file and roundtrips it.
    pub ok: bool,
    /// True when parser diagnostics were emitted.
    pub has_errors: bool,
    /// True when CST token reconstruction exactly matches the source.
    pub roundtrip_ok: bool,
    /// Parser diagnostics as normalized JSON values.
    pub diagnostics: Vec<serde_json::Value>,
}

impl RustParserResult {
    /// Parses a normalized Rust parser JSON result.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::{PhpLintResult, RustParserResult};

    #[test]
    fn parses_php_lint_result_json() {
        let json = r#"{
            "file": "fixtures/parser/valid/basic_echo.php",
            "ok": true,
            "exit_code": 0,
            "stdout": "No syntax errors detected",
            "stderr": "",
            "php_version": "8.5.7"
        }"#;

        let result = PhpLintResult::from_json(json).expect("valid lint JSON");
        assert!(result.ok);
        assert_eq!(result.php_version, "8.5.7");
    }

    #[test]
    fn parses_rust_parser_result_json() {
        let json = r#"{
            "engine": "rust-php-parser",
            "file": "fixtures/parser/valid/basic_echo.php",
            "ok": true,
            "has_errors": false,
            "roundtrip_ok": true,
            "diagnostics": []
        }"#;

        let result = RustParserResult::from_json(json).expect("valid parser JSON");
        assert!(result.ok);
        assert!(result.roundtrip_ok);
        assert!(result.diagnostics.is_empty());
    }
}
