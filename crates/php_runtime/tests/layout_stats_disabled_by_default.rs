//! Layout stats recording must be a no-op until a collector opts in.
//!
//! This lives in an integration test so it runs in its own process: unit tests
//! elsewhere in the crate legitimately enable recording via
//! `reset_layout_stats`.

use php_runtime::api::{PhpArray, PhpString, Value};
use php_runtime::experimental::layout_stats::{
    RuntimeLayoutStats, reset_layout_stats, take_layout_stats,
};

#[test]
fn recorders_are_noops_until_reset_enables_recording() {
    // No reset has run in this process: hot-path events must not record.
    let string = PhpString::from("layout-stats-disabled");
    let _string_clone = string.clone();
    let array = PhpArray::from_packed(vec![Value::Int(1), Value::Int(2)]);
    let _array_clone = array.clone();
    let _value_clone = Value::String(string).clone();

    assert_eq!(
        take_layout_stats(),
        RuntimeLayoutStats::default(),
        "layout stats must stay zero before reset_layout_stats enables recording"
    );

    // After reset (how the VM opts in), the same events must record.
    reset_layout_stats();
    let string = PhpString::from("layout-stats-enabled");
    let _value_clone = Value::String(string).clone();
    let stats = take_layout_stats();
    assert!(stats.value_clones >= 1, "{stats:?}");
    assert!(stats.string_allocations >= 1, "{stats:?}");

    let string = PhpString::from("layout-stats-disabled-again");
    let _value_clone = Value::String(string).clone();
    assert_eq!(
        take_layout_stats(),
        RuntimeLayoutStats::default(),
        "taking layout stats must end recording for later uninstrumented work"
    );
}
