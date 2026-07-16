<?php
// oracle-probe: id=oracle-builtin-contract-function-php-std-test-probe-8fbd8d7ce0 area=builtin_contract kind=function symbol=__php_std_test_probe source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-php-std-test-probe-8fbd8d7ce0 failure_category=builtin_contract requires_ref_extension=test
$name = "__php_std_test_probe";
echo function_exists($name) ? "available\n" : "missing\n";
