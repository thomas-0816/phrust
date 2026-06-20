//! Test utilities for differential PHP compatibility work.
//!
//! This crate owns reference-data formats and future process helpers for
//! `token_get_all()`, `php -l`, and runtime behavior. It intentionally contains
//! no PHP engine implementation.

pub mod diff;
pub mod fixtures;
pub mod lexer_reference;
pub mod parser_reference;

/// Returns the expected local checkout path for the pinned PHP reference.
#[must_use]
pub const fn reference_checkout_path() -> &'static str {
    "third_party/php-src"
}

#[cfg(test)]
mod tests {
    use super::reference_checkout_path;

    #[test]
    fn exposes_reference_checkout_path() {
        assert_eq!(reference_checkout_path(), "third_party/php-src");
    }
}
