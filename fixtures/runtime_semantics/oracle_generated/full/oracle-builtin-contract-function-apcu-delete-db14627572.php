<?php
// oracle-probe: id=oracle-builtin-contract-function-apcu-delete-db14627572 area=builtin_contract kind=function symbol=apcu_delete source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-apcu-delete-db14627572 failure_category=builtin_contract requires_ref_extension=apcu
$name = "apcu_delete";
echo function_exists($name) ? "available\n" : "missing\n";
