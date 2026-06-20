use serde::Deserialize;

/// JSON output from `scripts/tokenize-reference.php`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ReferenceTokenStream {
    /// PHP version that produced the token stream.
    pub php_version: String,
    /// PHP_VERSION_ID that produced the token stream.
    pub php_version_id: u32,
    /// Input file path.
    pub file: String,
    /// Whether TOKEN_PARSE was used.
    pub token_parse: bool,
    /// Normalized tokens.
    pub tokens: Vec<ReferenceToken>,
}

impl ReferenceTokenStream {
    /// Parses a reference token stream from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// A normalized `token_get_all()` token.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ReferenceToken {
    /// Zero-based token index.
    pub index: usize,
    /// `T_*` name or single-character symbol.
    pub kind: String,
    /// Original token text as emitted by PHP, with invalid UTF-8 substituted by
    /// the PHP oracle script when JSON encoding requires it.
    pub text: String,
    /// One-based start line.
    pub line: u32,
}

/// JSON output from `scripts/dump-reference-tokens.php`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ReferenceTokenDump {
    /// PHP version that produced the constant dump.
    pub php_version: String,
    /// PHP_VERSION_ID that produced the constant dump.
    pub php_version_id: u32,
    /// UTC generation timestamp from the PHP script.
    pub generated_at: String,
    /// Stable name-sorted token constants.
    pub tokens: Vec<ReferenceTokenConstant>,
}

impl ReferenceTokenDump {
    /// Parses a reference token constant dump from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// One tokenizer `T_*` constant from the PHP reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ReferenceTokenConstant {
    /// Constant name, for example `T_OPEN_TAG`.
    pub name: String,
    /// Numeric value from the current PHP build.
    pub value: i64,
}

#[cfg(test)]
mod tests {
    use super::{ReferenceTokenDump, ReferenceTokenStream};

    #[test]
    fn parses_reference_token_stream_json() {
        let json = r#"{
          "php_version": "8.5.7",
          "php_version_id": 80507,
          "file": "tests/fixtures/lexer/basic.php",
          "token_parse": false,
          "tokens": [
            {"index": 0, "kind": "T_OPEN_TAG", "text": "<?php ", "line": 1},
            {"index": 1, "kind": "T_ECHO", "text": "echo", "line": 1},
            {"index": 2, "kind": ";", "text": ";", "line": 1}
          ]
        }"#;

        let stream = ReferenceTokenStream::from_json(json).expect("valid JSON");
        assert_eq!(stream.php_version, "8.5.7");
        assert!(!stream.token_parse);
        assert_eq!(stream.tokens.len(), 3);
        assert_eq!(stream.tokens[0].kind, "T_OPEN_TAG");
        assert_eq!(stream.tokens[2].kind, ";");
    }

    #[test]
    fn parses_token_parse_reference_stream_json() {
        let json = r#"{
          "php_version": "8.5.7",
          "php_version_id": 80507,
          "file": "tests/fixtures/lexer/contextual.php",
          "token_parse": true,
          "tokens": [
            {"index": 0, "kind": "T_OPEN_TAG", "text": "<?php ", "line": 1},
            {"index": 1, "kind": "T_STRING", "text": "match", "line": 1},
            {"index": 2, "kind": ";", "text": ";", "line": 1}
          ]
        }"#;

        let stream = ReferenceTokenStream::from_json(json).expect("valid JSON");
        assert!(stream.token_parse);
        assert_eq!(stream.tokens[1].kind, "T_STRING");
        assert_eq!(stream.tokens[1].text, "match");
    }

    #[test]
    fn parses_reference_token_dump_json() {
        let json = r#"{
          "php_version": "8.5.7",
          "php_version_id": 80507,
          "generated_at": "2026-06-19T00:00:00+00:00",
          "tokens": [
            {"name": "T_CLOSE_TAG", "value": 392},
            {"name": "T_OPEN_TAG", "value": 391}
          ]
        }"#;

        let dump = ReferenceTokenDump::from_json(json).expect("valid JSON");
        assert_eq!(dump.tokens[0].name, "T_CLOSE_TAG");
        assert_eq!(dump.tokens[1].value, 391);
    }
}
