//! Runtime-semantics fixture metadata.

use serde::{Deserialize, Serialize};

/// Canonical runtime-semantics fixture categories.
pub const RUNTIME_SEMANTICS_FIXTURE_CATEGORIES: &[&str] = &[
    "refs",
    "cow",
    "arrays",
    "foreach",
    "functions",
    "closures",
    "callables",
    "objects",
    "traits",
    "enums",
    "magic",
    "properties",
    "property_hooks",
    "clone_with",
    "void_cast",
    "const_expr",
    "generators",
    "fibers",
    "reflection",
    "errors",
    "destructors",
    "gc",
    "include_eval_autoload",
    "globals",
    "superglobals",
    "variables",
    "statics",
    "real_world",
    "wordpress_blockers",
    "regressions",
    "known_gaps",
];

/// Machine-readable runtime-semantics differential summary counters.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeSemanticsDiffSummary {
    /// Number of fixture comparisons selected.
    pub total: usize,
    /// Number of exact matches or expected runtime failures.
    pub pass: usize,
    /// Number of unexpected mismatches or harness errors.
    pub fail: usize,
    /// Number of comparisons skipped because prerequisites were unavailable.
    pub skip: usize,
    /// Number of explicitly marked known-gap fixtures.
    pub known_gap: usize,
}

/// Returns true when `category` is part of the runtime-semantics fixture matrix.
#[must_use]
pub fn is_runtime_semantics_category(category: &str) -> bool {
    RUNTIME_SEMANTICS_FIXTURE_CATEGORIES.contains(&category)
}

#[cfg(test)]
mod tests {
    use super::{
        RUNTIME_SEMANTICS_FIXTURE_CATEGORIES, RuntimeSemanticsDiffSummary,
        is_runtime_semantics_category,
    };

    #[test]
    fn runtime_semantics_categories_match_coverage_matrix() {
        assert_eq!(RUNTIME_SEMANTICS_FIXTURE_CATEGORIES.len(), 31);
        assert!(is_runtime_semantics_category("refs"));
        assert!(is_runtime_semantics_category("errors"));
        assert!(is_runtime_semantics_category("destructors"));
        assert!(is_runtime_semantics_category("gc"));
        assert!(is_runtime_semantics_category("include_eval_autoload"));
        assert!(is_runtime_semantics_category("globals"));
        assert!(is_runtime_semantics_category("superglobals"));
        assert!(is_runtime_semantics_category("variables"));
        assert!(is_runtime_semantics_category("statics"));
        assert!(is_runtime_semantics_category("real_world"));
        assert!(is_runtime_semantics_category("wordpress_blockers"));
        assert!(is_runtime_semantics_category("regressions"));
        assert!(is_runtime_semantics_category("known_gaps"));
        assert!(!is_runtime_semantics_category("syntax"));
    }

    #[test]
    fn runtime_semantics_summary_serializes_machine_readable_counters() {
        let summary = RuntimeSemanticsDiffSummary {
            total: 4,
            pass: 1,
            fail: 1,
            skip: 1,
            known_gap: 1,
        };
        let json = serde_json::to_string(&summary).expect("json");
        assert!(json.contains("\"known_gap\":1"));
    }
}
